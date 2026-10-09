//! Updates on Linux. A copy installed from the tarball (its `install.sh`
//! puts it in `~/.local/lib/gasp`, beside its notices) replaces its own
//! binary: the release's tarball is downloaded, checked against the
//! SHA-256 GitHub gives for it, unpacked, and its `gasp` asked its version
//! before it's moved over the running one, which goes on running from
//! the old file until the restart. Linux has no code signing to lean on,
//! so a release without a checksum isn't installed.
//!
//! A copy a package manager installed (the `.deb`, in `/usr`) is updated
//! the way it was installed, so its notice opens the download page.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use semver::Version;

use super::bundle::folder_is_writable;
use super::install::{InstallError, shell_quote};
use super::release::{Release, parse_version};
use super::verify::{Refusal, check_digest};

/// The file that sits beside the binary in the tarball and where
/// `install.sh` puts it, and never beside a build in `target/`.
const NOTICES: &str = "THIRD_PARTY_NOTICES.txt";

/// Where to get a new version by hand.
pub const DOWNLOAD_PAGE: &str = "https://gaspmd.com/download";

/// How long the restart helper waits for the app to quit, in tenths of a
/// second.
const QUIT_WAIT_TENTHS: u32 = 600;

/// How the running copy was installed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Install {
    /// From the tarball, in a folder it may write to: it updates itself.
    Tarball { binary: PathBuf },
    /// By a package manager, or somewhere it can't write: the person
    /// installs the new version.
    Managed,
}

/// How the running copy was installed, or `None` for a build that isn't
/// installed at all, such as `cargo run`.
pub fn running_install() -> Option<Install> {
    let exe = std::env::current_exe().ok()?;
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    install_of(&exe)
}

/// How the binary at `exe` was installed.
pub fn install_of(exe: &Path) -> Option<Install> {
    if exe.starts_with("/usr") {
        return Some(Install::Managed);
    }
    let folder = exe.parent()?;
    if !folder.join(NOTICES).is_file() {
        return None;
    }
    Some(if folder_is_writable(exe) {
        Install::Tarball {
            binary: exe.to_path_buf(),
        }
    } else {
        Install::Managed
    })
}

/// Checks the tarball at `tarball` and the binary in it, then moves that
/// binary over `binary`. The download is deleted either way.
pub fn prepare(
    tarball: &Path,
    release: &Release,
    binary: &Path,
    running_version: &Version,
) -> Result<PathBuf, InstallError> {
    let prepared = check_and_replace(tarball, release, binary, running_version);
    if let Some(folder) = tarball.parent() {
        let _ = std::fs::remove_dir_all(folder);
    }
    prepared
}

fn check_and_replace(
    tarball: &Path,
    release: &Release,
    binary: &Path,
    running_version: &Version,
) -> Result<PathBuf, InstallError> {
    if release.sha256.is_none() {
        return Err(Refusal::NoChecksum.into());
    }
    check_digest(tarball, release.sha256.as_deref())?;
    let unpacked = tarball.with_extension("unpacked");
    let new = unpack(tarball, &unpacked)?;
    let version = version_of(&new).ok_or(Refusal::WontRun)?;
    if version != release.version || version <= *running_version {
        return Err(Refusal::NotNewer.into());
    }
    replace(&new, binary)?;
    Ok(binary.to_path_buf())
}

/// Unpacks `tarball` into `folder` and finds the `gasp` in it.
fn unpack(tarball: &Path, folder: &Path) -> Result<PathBuf, InstallError> {
    let failed = |error: &dyn std::fmt::Display| {
        InstallError::Failed(format!("couldn’t unpack the download: {error}"))
    };
    let _ = std::fs::remove_dir_all(folder);
    std::fs::create_dir_all(folder).map_err(|error| failed(&error))?;
    let status = Command::new("tar")
        .arg("-xzf")
        .arg(tarball)
        .arg("-C")
        .arg(folder)
        .arg("--no-same-owner")
        .stdin(Stdio::null())
        .status()
        .map_err(|error| failed(&error))?;
    if !status.success() {
        return Err(failed(&status));
    }
    find_binary(folder).ok_or(Refusal::NoApp.into())
}

/// The `gasp` in an unpacked tarball: in its one top folder, or at its
/// top.
pub fn find_binary(folder: &Path) -> Option<PathBuf> {
    let name = gasp_config::COMMAND_NAME;
    let at_top = folder.join(name);
    if at_top.is_file() {
        return Some(at_top);
    }
    std::fs::read_dir(folder)
        .ok()?
        .flatten()
        .map(|entry| entry.path().join(name))
        .find(|path| path.is_file())
}

/// The version the binary at `binary` says it is, which also shows it
/// runs on this computer.
fn version_of(binary: &Path) -> Option<Version> {
    let output = Command::new(binary)
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_version_output(&String::from_utf8_lossy(&output.stdout))
}

/// The version in `gasp --version`'s output, such as `gasp 0.2.4`.
pub fn parse_version_output(output: &str) -> Option<Version> {
    let version = output.split_whitespace().nth(1)?;
    parse_version(version).ok()
}

/// Moves `new` over `binary`: copied beside it first, so the move is a
/// rename on one filesystem, which the running app survives.
fn replace(new: &Path, binary: &Path) -> Result<(), InstallError> {
    use std::os::unix::fs::PermissionsExt;
    let failed = |error: std::io::Error| {
        InstallError::Failed(format!("couldn’t put the new version in place: {error}"))
    };
    let staged = staged_path(binary);
    std::fs::copy(new, &staged).map_err(failed)?;
    std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755)).map_err(failed)?;
    std::fs::rename(&staged, binary).map_err(|error| {
        let _ = std::fs::remove_file(&staged);
        failed(error)
    })
}

/// Where the new binary is copied before it's moved over `binary`.
pub fn staged_path(binary: &Path) -> PathBuf {
    let name = binary
        .file_name()
        .map_or_else(|| "gasp".into(), |name| name.to_string_lossy());
    binary.with_file_name(format!(".{name}-update"))
}

/// The helper's shell script: it waits for the app to quit, then opens
/// the binary, which by now is the new version.
pub fn restart_script(pid: u32, binary: &Path) -> String {
    format!(
        r#"pid={pid}
tries=0
while kill -0 "$pid" 2>/dev/null; do
  tries=$((tries + 1))
  [ "$tries" -gt {QUIT_WAIT_TENTHS} ] && exit 1
  sleep 0.1
done
exec {binary}
"#,
        binary = shell_quote(&binary.to_string_lossy()),
    )
}

/// Starts the restart helper on its own, so it outlives the app.
pub fn spawn_restart_helper(binary: &Path) -> std::io::Result<()> {
    use std::os::unix::process::CommandExt;
    Command::new("/bin/sh")
        .arg("-c")
        .arg(restart_script(std::process::id(), binary))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    /// A stand-in `gasp` that answers `--version` with `version`.
    fn fake_gasp(path: &Path, version: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, format!("#!/bin/sh\necho \"gasp {version}\"\n")).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// A release tarball laid out as `package-linux.sh` makes it, with
    /// its SHA-256.
    fn make_tarball(folder: &Path, version: &str) -> (PathBuf, String) {
        let top = format!("Gasp-{version}-linux-x86_64");
        let source = folder.join("source");
        fake_gasp(&source.join(&top).join("gasp"), version);
        std::fs::write(source.join(&top).join(NOTICES), "notices").unwrap();
        let download = folder.join("download");
        std::fs::create_dir_all(&download).unwrap();
        let tarball = download.join(format!("{top}.tar.gz"));
        let status = Command::new("tar")
            .arg("-czf")
            .arg(&tarball)
            .arg("-C")
            .arg(&source)
            .arg(&top)
            .status()
            .unwrap();
        assert!(status.success());
        let digest = super::super::verify::sha256_of(&tarball).unwrap();
        (tarball, digest)
    }

    fn release(version: &str, sha256: Option<String>) -> Release {
        Release {
            version: Version::parse(version).unwrap(),
            url: "https://example.com/Gasp.tar.gz".into(),
            notes: "https://example.com/notes".into(),
            published: "2026-10-01T09:00:00Z".into(),
            size: 1,
            sha256,
        }
    }

    fn installed(folder: &Path, version: &str) -> PathBuf {
        let binary = folder.join("lib/gasp/gasp");
        fake_gasp(&binary, version);
        std::fs::write(folder.join("lib/gasp").join(NOTICES), "notices").unwrap();
        binary
    }

    #[test]
    fn where_the_binary_lives_says_how_it_updates() {
        let folder = tempfile::tempdir().unwrap();
        let binary = installed(folder.path(), "0.2.0");
        assert_eq!(
            install_of(&binary),
            Some(Install::Tarball {
                binary: binary.clone()
            })
        );
        assert_eq!(
            install_of(Path::new("/usr/bin/gasp")),
            Some(Install::Managed)
        );
        let build = folder.path().join("target/release/gasp");
        fake_gasp(&build, "0.2.0");
        assert_eq!(install_of(&build), None);
    }

    #[test]
    fn a_checked_tarball_replaces_the_binary() {
        let folder = tempfile::tempdir().unwrap();
        let binary = installed(folder.path(), "0.2.0");
        let (tarball, digest) = make_tarball(folder.path(), "0.3.0");
        let running = Version::new(0, 2, 0);
        let prepared = prepare(&tarball, &release("0.3.0", Some(digest)), &binary, &running);
        assert_eq!(prepared, Ok(binary.clone()));
        assert_eq!(version_of(&binary), Some(Version::new(0, 3, 0)));
        assert!(!staged_path(&binary).exists());
        assert!(!tarball.parent().unwrap().exists(), "the download is gone");
    }

    #[test]
    fn a_tarball_that_doesnt_match_or_isnt_newer_is_refused() {
        let folder = tempfile::tempdir().unwrap();
        let binary = installed(folder.path(), "0.2.0");
        let running = Version::new(0, 2, 0);
        let (tarball, _) = make_tarball(folder.path(), "0.3.0");
        let wrong = Some("0".repeat(64));
        assert_eq!(
            prepare(&tarball, &release("0.3.0", wrong), &binary, &running),
            Err(Refusal::DigestMismatch.into())
        );
        let (tarball, _) = make_tarball(folder.path(), "0.3.0");
        assert_eq!(
            prepare(&tarball, &release("0.3.0", None), &binary, &running),
            Err(Refusal::NoChecksum.into())
        );
        let (tarball, digest) = make_tarball(folder.path(), "0.2.0");
        assert_eq!(
            prepare(&tarball, &release("0.2.0", Some(digest)), &binary, &running),
            Err(Refusal::NotNewer.into())
        );
        assert_eq!(version_of(&binary), Some(Version::new(0, 2, 0)));
    }

    #[test]
    fn the_version_comes_from_the_binarys_answer() {
        assert_eq!(
            parse_version_output("gasp 0.2.4\n"),
            Some(Version::new(0, 2, 4))
        );
        assert_eq!(parse_version_output("gasp"), None);
        assert_eq!(parse_version_output(""), None);
    }

    #[test]
    fn the_restart_waits_for_the_app_then_opens_the_new_binary() {
        let script = restart_script(42, Path::new("/home/a b/.local/lib/gasp/gasp"));
        assert!(script.starts_with("pid=42\n"));
        assert!(script.ends_with("exec '/home/a b/.local/lib/gasp/gasp'\n"));
    }
}
