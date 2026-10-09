//! Getting a verified new version in place: downloading the disk image,
//! checking the app in it, staging it beside the running bundle, and the
//! helper that swaps the two once the app has quit.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use semver::Version;

use super::bundle::{aside_path, folder_is_writable, staged_path};
use super::release::Release;
use super::tools::SystemTools;
use super::verify::{Refusal, SigningIdentity, check_digest, find_app, verify_app};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// How long the helper waits for the app to quit before giving up.
const QUIT_WAIT_TENTHS: u32 = 600;

/// Where a verified version ended up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Prepared {
    /// Copied beside the running bundle, ready for the restart helper.
    Staged(PathBuf),
    /// The app's folder takes no new files, so the person drags the app
    /// from this disk image themselves.
    DragToFinish(PathBuf),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstallError {
    Refused(Refusal),
    Failed(String),
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallError::Refused(refusal) => refusal.fmt(f),
            InstallError::Failed(message) => f.write_str(message),
        }
    }
}

impl From<Refusal> for InstallError {
    fn from(refusal: Refusal) -> InstallError {
        InstallError::Refused(refusal)
    }
}

/// The folder a download of `version` goes to, under the system's
/// temporary folder.
pub fn download_folder(version: &Version) -> PathBuf {
    std::env::temp_dir().join(format!("gasp-update-{version}"))
}

/// Downloads `release`'s disk image or tarball into `folder`, calling
/// `on_progress` with each new whole percent.
pub fn download(
    release: &Release,
    folder: &Path,
    on_progress: impl FnMut(u8),
) -> Result<PathBuf, InstallError> {
    let failed = |error: &dyn std::fmt::Display| {
        InstallError::Failed(format!("the download failed: {error}"))
    };
    std::fs::create_dir_all(folder).map_err(|error| failed(&error))?;
    let extension = if release.url.ends_with(".tar.gz") {
        "tar.gz"
    } else {
        "dmg"
    };
    let dmg = folder.join(format!("Gasp-{}.{extension}", release.version));
    let response = reqwest::blocking::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(DOWNLOAD_TIMEOUT)
        .build()
        .and_then(|client| client.get(&release.url).send())
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|error| failed(&error))?;
    let mut file = std::fs::File::create(&dmg).map_err(|error| failed(&error))?;
    let copied = copy_with_progress(response, &mut file, release.size, on_progress);
    match copied {
        Ok(bytes) if release.size == 0 || bytes == release.size => Ok(dmg),
        Ok(_) => Err(InstallError::Failed("the download stopped early".into())),
        Err(error) => Err(failed(&error)),
    }
}

/// Copies `reader` into `writer`, calling `on_progress` with each new
/// whole percent of `total` bytes. Returns how many bytes went across.
pub fn copy_with_progress(
    mut reader: impl Read,
    writer: &mut impl Write,
    total: u64,
    mut on_progress: impl FnMut(u8),
) -> std::io::Result<u64> {
    let mut buffer = vec![0; 64 * 1024];
    let mut copied = 0u64;
    let mut reported = None;
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            return Ok(copied);
        }
        writer.write_all(&buffer[..read])?;
        copied += read as u64;
        let percent = percent_of(copied, total);
        if reported != Some(percent) {
            reported = Some(percent);
            on_progress(percent);
        }
    }
}

fn percent_of(done: u64, total: u64) -> u8 {
    if total == 0 {
        return 0;
    }
    (done.saturating_mul(100) / total).min(100) as u8
}

/// Checks the disk image at `dmg` and the app in it, then stages that app
/// beside `bundle`. The image is deleted unless the person still needs
/// it to drag the app across themselves.
pub fn prepare(
    dmg: &Path,
    release: &Release,
    bundle: &Path,
    running_version: &Version,
    tools: &dyn SystemTools,
) -> Result<Prepared, InstallError> {
    let prepared = check_and_stage(dmg, release, bundle, running_version, tools);
    if !matches!(prepared, Ok(Prepared::DragToFinish(_))) {
        remove_download(dmg);
    }
    prepared
}

fn check_and_stage(
    dmg: &Path,
    release: &Release,
    bundle: &Path,
    running_version: &Version,
    tools: &dyn SystemTools,
) -> Result<Prepared, InstallError> {
    check_digest(dmg, release.sha256.as_deref())?;
    let running = tools
        .signing_identity(bundle)
        .ok_or(Refusal::RunningUnsigned)?;
    let mount = tools
        .attach(dmg)
        .map_err(|error| InstallError::Failed(format!("the disk image wouldn’t open: {error}")))?;
    let staged = stage_from(&mount, &running, running_version, bundle, tools);
    tools.detach(&mount);
    match staged? {
        Some(staged) => Ok(Prepared::Staged(staged)),
        None => Ok(Prepared::DragToFinish(dmg.to_path_buf())),
    }
}

/// Verifies the app on the mounted image and copies it beside `bundle`;
/// `None` when the folder takes no new files.
fn stage_from(
    mount: &Path,
    running: &SigningIdentity,
    running_version: &Version,
    bundle: &Path,
    tools: &dyn SystemTools,
) -> Result<Option<PathBuf>, InstallError> {
    let app = find_app(mount).ok_or(Refusal::NoApp)?;
    verify_app(&app, running, running_version, tools)?;
    if !folder_is_writable(bundle) {
        return Ok(None);
    }
    let staged = staged_path(bundle);
    remove_bundle(&staged);
    tools.copy_app(&app, &staged).map_err(|error| {
        InstallError::Failed(format!("the new app couldn’t be copied: {error}"))
    })?;
    let intact = tools.signature_is_valid(&staged)
        && tools.signing_identity(&staged).as_ref() == Some(running);
    if !intact {
        remove_bundle(&staged);
        return Err(Refusal::BrokenSignature.into());
    }
    Ok(Some(staged))
}

/// Deletes a downloaded disk image and the folder it was downloaded to.
pub fn remove_download(dmg: &Path) {
    let _ = std::fs::remove_file(dmg);
    if let Some(folder) = dmg.parent() {
        let _ = std::fs::remove_dir(folder);
    }
}

fn remove_bundle(path: &Path) {
    if path.exists() {
        let _ = std::fs::remove_dir_all(path);
    }
}

/// What the restart helper swaps, and the process it waits for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestartPlan {
    pub pid: u32,
    pub bundle: PathBuf,
    pub staged: PathBuf,
    pub aside: PathBuf,
}

impl RestartPlan {
    /// Swaps the version staged beside `bundle` in once this process quits.
    pub fn for_this_process(bundle: &Path) -> RestartPlan {
        RestartPlan {
            pid: std::process::id(),
            bundle: bundle.to_path_buf(),
            staged: staged_path(bundle),
            aside: aside_path(bundle),
        }
    }
}

/// The helper's shell script: it waits for the app to quit, moves the old
/// bundle aside and the staged one into its place, puts the old one back
/// if that fails, deletes the old one, then opens the bundle with
/// `relaunch`.
pub fn restart_script(plan: &RestartPlan, relaunch: &str) -> String {
    format!(
        r#"pid={pid}
bundle={bundle}
staged={staged}
aside={aside}
tries=0
while kill -0 "$pid" 2>/dev/null; do
  tries=$((tries + 1))
  [ "$tries" -gt {QUIT_WAIT_TENTHS} ] && exit 1
  sleep 0.1
done
[ -d "$staged" ] || exit 1
rm -rf "$aside"
mv "$bundle" "$aside" || exit 1
if mv "$staged" "$bundle"; then
  rm -rf "$aside"
else
  mv "$aside" "$bundle"
fi
{relaunch} "$bundle"
"#,
        pid = plan.pid,
        bundle = shell_quote(&plan.bundle.to_string_lossy()),
        staged = shell_quote(&plan.staged.to_string_lossy()),
        aside = shell_quote(&plan.aside.to_string_lossy()),
        relaunch = shell_quote(relaunch),
    )
}

/// `text` as one single-quoted shell word.
pub fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// Starts the restart helper on its own, so it outlives the app.
pub fn spawn_restart_helper(plan: &RestartPlan) -> std::io::Result<()> {
    use std::os::unix::process::CommandExt;
    Command::new("/usr/bin/nohup")
        .arg("/bin/sh")
        .arg("-c")
        .arg(restart_script(plan, "/usr/bin/open"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use super::*;

    const TEAM: &str = "K2MB68Z582";

    fn identity(team: &str) -> SigningIdentity {
        SigningIdentity {
            identifier: "com.borisnezlobin.gasp".into(),
            team_id: Some(team.into()),
        }
    }

    fn write_app(app: &Path, version: &str) {
        std::fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
        std::fs::write(app.join("Contents/MacOS/gasp"), version).unwrap();
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict><key>CFBundleShortVersionString</key><string>{version}</string></dict></plist>"#
        );
        std::fs::write(app.join("Contents/Info.plist"), plist).unwrap();
    }

    fn copy_dir(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_dir(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), target).unwrap();
            }
        }
    }

    /// Stands in for `hdiutil`, `codesign`, `spctl` and `ditto`: the
    /// "disk image" is a folder, and each check answers as told.
    struct FakeTools {
        mount: PathBuf,
        new_identity: SigningIdentity,
        running_identity: Option<SigningIdentity>,
        signature_valid: bool,
        notarized: bool,
        attached: Cell<bool>,
        detached: Cell<bool>,
        copied: RefCell<Vec<PathBuf>>,
    }

    impl FakeTools {
        fn new(mount: &Path) -> FakeTools {
            FakeTools {
                mount: mount.to_path_buf(),
                new_identity: identity(TEAM),
                running_identity: Some(identity(TEAM)),
                signature_valid: true,
                notarized: true,
                attached: Cell::new(false),
                detached: Cell::new(false),
                copied: RefCell::new(Vec::new()),
            }
        }

        fn is_new_app(&self, app: &Path) -> bool {
            app.starts_with(&self.mount) || self.copied.borrow().iter().any(|path| path == app)
        }
    }

    impl SystemTools for FakeTools {
        fn attach(&self, _: &Path) -> Result<PathBuf, String> {
            self.attached.set(true);
            Ok(self.mount.clone())
        }

        fn detach(&self, _: &Path) {
            self.detached.set(true);
        }

        fn signature_is_valid(&self, _: &Path) -> bool {
            self.signature_valid
        }

        fn signing_identity(&self, app: &Path) -> Option<SigningIdentity> {
            if self.is_new_app(app) {
                Some(self.new_identity.clone())
            } else {
                self.running_identity.clone()
            }
        }

        fn gatekeeper_accepts(&self, _: &Path) -> bool {
            self.notarized
        }

        fn copy_app(&self, from: &Path, to: &Path) -> Result<(), String> {
            copy_dir(from, to);
            self.copied.borrow_mut().push(to.to_path_buf());
            Ok(())
        }
    }

    struct Scene {
        _root: tempfile::TempDir,
        bundle: PathBuf,
        dmg: PathBuf,
        mount: PathBuf,
        release: Release,
    }

    fn scene(new_version: &str) -> Scene {
        let root = tempfile::tempdir().unwrap();
        let bundle = root.path().join("My Apps/Gasp.app");
        write_app(&bundle, "0.1.0");
        let mount = root.path().join("Volumes/Gasp");
        write_app(&mount.join("Gasp.app"), new_version);
        std::os::unix::fs::symlink("/Applications", mount.join("Applications")).unwrap();
        let dmg = root.path().join("download/Gasp-0.2.0.dmg");
        std::fs::create_dir_all(dmg.parent().unwrap()).unwrap();
        std::fs::write(&dmg, "abc").unwrap();
        let release = Release {
            version: Version::new(0, 2, 0),
            url: "https://example.com/Gasp-0.2.0.dmg".into(),
            notes: "https://example.com/v0.2.0".into(),
            published: "2026-10-01T09:00:00Z".into(),
            size: 3,
            sha256: Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".into()),
        };
        Scene {
            _root: root,
            bundle,
            dmg,
            mount,
            release,
        }
    }

    fn running() -> Version {
        Version::new(0, 1, 0)
    }

    #[test]
    fn a_verified_app_is_staged_beside_the_bundle() {
        let scene = scene("0.2.0");
        let tools = FakeTools::new(&scene.mount);
        let prepared = prepare(
            &scene.dmg,
            &scene.release,
            &scene.bundle,
            &running(),
            &tools,
        );
        let staged = scene.bundle.with_file_name(".Gasp-update.app");
        assert_eq!(prepared, Ok(Prepared::Staged(staged.clone())));
        assert_eq!(
            std::fs::read_to_string(staged.join("Contents/MacOS/gasp")).unwrap(),
            "0.2.0"
        );
        assert!(tools.detached.get());
        assert!(!scene.dmg.exists());
        assert_eq!(
            std::fs::read_to_string(scene.bundle.join("Contents/MacOS/gasp")).unwrap(),
            "0.1.0"
        );
    }

    #[test]
    fn a_stale_staged_copy_is_replaced() {
        let scene = scene("0.2.0");
        let staged = scene.bundle.with_file_name(".Gasp-update.app");
        write_app(&staged, "0.1.5");
        std::fs::write(staged.join("leftover"), "x").unwrap();
        let tools = FakeTools::new(&scene.mount);
        prepare(
            &scene.dmg,
            &scene.release,
            &scene.bundle,
            &running(),
            &tools,
        )
        .unwrap();
        assert!(!staged.join("leftover").exists());
    }

    fn refusal_for(scene: &Scene, tools: &FakeTools) -> Result<Prepared, InstallError> {
        let prepared = prepare(&scene.dmg, &scene.release, &scene.bundle, &running(), tools);
        assert!(!scene.dmg.exists(), "a refused download is deleted");
        assert!(!scene.bundle.with_file_name(".Gasp-update.app").exists());
        assert_eq!(tools.attached.get(), tools.detached.get());
        prepared
    }

    #[test]
    fn a_download_that_doesnt_match_its_checksum_is_never_opened() {
        let mut scene = scene("0.2.0");
        scene.release.sha256 = Some("0".repeat(64));
        let tools = FakeTools::new(&scene.mount);
        assert_eq!(
            refusal_for(&scene, &tools),
            Err(InstallError::Refused(Refusal::DigestMismatch))
        );
        assert!(!tools.attached.get());
    }

    #[test]
    fn another_developers_app_is_refused() {
        let other_team = identity("ATTACKER01");
        let other_app = SigningIdentity {
            identifier: "com.example.other".into(),
            team_id: Some(TEAM.into()),
        };
        for new_identity in [other_team, other_app] {
            let scene = scene("0.2.0");
            let tools = FakeTools {
                new_identity,
                ..FakeTools::new(&scene.mount)
            };
            assert_eq!(
                refusal_for(&scene, &tools),
                Err(Refusal::OtherDeveloper.into())
            );
        }
    }

    #[test]
    fn broken_unnotarized_or_older_apps_are_refused() {
        let scene = scene("0.2.0");
        let tools = FakeTools {
            signature_valid: false,
            ..FakeTools::new(&scene.mount)
        };
        assert_eq!(
            refusal_for(&scene, &tools),
            Err(Refusal::BrokenSignature.into())
        );
        let scene = self::scene("0.2.0");
        let tools = FakeTools {
            notarized: false,
            ..FakeTools::new(&scene.mount)
        };
        assert_eq!(
            refusal_for(&scene, &tools),
            Err(Refusal::NotNotarized.into())
        );
        let scene = self::scene("0.0.9");
        let tools = FakeTools::new(&scene.mount);
        assert_eq!(refusal_for(&scene, &tools), Err(Refusal::NotNewer.into()));
    }

    #[test]
    fn an_unsigned_running_app_cant_vouch_for_an_update() {
        let scene = scene("0.2.0");
        let tools = FakeTools {
            running_identity: None,
            ..FakeTools::new(&scene.mount)
        };
        assert_eq!(
            refusal_for(&scene, &tools),
            Err(Refusal::RunningUnsigned.into())
        );
        let scene = self::scene("0.2.0");
        let ad_hoc = SigningIdentity {
            identifier: "com.borisnezlobin.gasp".into(),
            team_id: None,
        };
        let tools = FakeTools {
            running_identity: Some(ad_hoc.clone()),
            new_identity: ad_hoc,
            ..FakeTools::new(&scene.mount)
        };
        assert_eq!(
            refusal_for(&scene, &tools),
            Err(Refusal::RunningUnsigned.into())
        );
    }

    #[test]
    fn an_image_with_no_app_is_refused() {
        let scene = scene("0.2.0");
        std::fs::remove_dir_all(scene.mount.join("Gasp.app")).unwrap();
        let tools = FakeTools::new(&scene.mount);
        assert_eq!(refusal_for(&scene, &tools), Err(Refusal::NoApp.into()));
    }

    #[test]
    fn a_folder_that_takes_no_files_keeps_the_image_for_dragging() {
        let scene = scene("0.2.0");
        let bundle = scene.bundle.parent().unwrap().join("missing/Gasp.app");
        let tools = FakeTools::new(&scene.mount);
        let prepared = prepare(&scene.dmg, &scene.release, &bundle, &running(), &tools);
        assert_eq!(prepared, Ok(Prepared::DragToFinish(scene.dmg.clone())));
        assert!(scene.dmg.exists());
        assert!(tools.detached.get());
    }

    #[test]
    fn progress_reports_each_whole_percent_once() {
        let source = vec![7u8; 1000];
        let mut sink = Vec::new();
        let mut reported = Vec::new();
        let reader = std::io::Cursor::new(source.clone()).chain(std::io::empty());
        let copied = copy_with_progress(SlowReader(reader), &mut sink, 1000, |percent| {
            reported.push(percent)
        })
        .unwrap();
        assert_eq!(copied, 1000);
        assert_eq!(sink, source);
        assert_eq!(reported.first(), Some(&25));
        assert_eq!(reported.last(), Some(&100));
        assert!(reported.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(percent_of(5, 0), 0);
        assert_eq!(percent_of(2000, 1000), 100);
    }

    /// Hands out a quarter of the data at a time, as a network would.
    struct SlowReader<R>(R);

    impl<R: Read> Read for SlowReader<R> {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            let limit = buffer.len().min(250);
            self.0.read(&mut buffer[..limit])
        }
    }

    #[test]
    fn shell_quoting_survives_spaces_and_quotes() {
        assert_eq!(
            shell_quote("/Applications/Gasp.app"),
            "'/Applications/Gasp.app'"
        );
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
        let tricky = r#"/tmp/a b/it's "here" $HOME `x`.app"#;
        let echoed = Command::new("/bin/sh")
            .arg("-c")
            .arg(format!("printf %s {}", shell_quote(tricky)))
            .output()
            .unwrap();
        assert_eq!(String::from_utf8(echoed.stdout).unwrap(), tricky);
    }

    fn exited_pid() -> u32 {
        let mut child = Command::new("/usr/bin/true").spawn().unwrap();
        let pid = child.id();
        child.wait().unwrap();
        pid
    }

    #[test]
    fn the_helper_swaps_bundles_with_awkward_names() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join(r#"Bob's "Apps" $dir"#);
        let bundle = folder.join("Gasp.app");
        write_app(&bundle, "0.1.0");
        let plan = RestartPlan {
            pid: exited_pid(),
            bundle: bundle.clone(),
            staged: staged_path(&bundle),
            aside: aside_path(&bundle),
        };
        write_app(&plan.staged, "0.2.0");
        let relaunched = root.path().join("relaunched");
        let relaunch = format!("{}/record", root.path().display());
        std::fs::write(
            &relaunch,
            format!(
                "#!/bin/sh\nprintf %s \"$1\" > {}\n",
                shell_quote(&relaunched.to_string_lossy())
            ),
        )
        .unwrap();
        Command::new("/bin/chmod")
            .arg("+x")
            .arg(&relaunch)
            .status()
            .unwrap();
        let status = Command::new("/bin/sh")
            .arg("-c")
            .arg(restart_script(&plan, &relaunch))
            .status()
            .unwrap();
        assert!(status.success());
        assert_eq!(
            std::fs::read_to_string(bundle.join("Contents/MacOS/gasp")).unwrap(),
            "0.2.0"
        );
        assert!(!plan.staged.exists());
        assert!(!plan.aside.exists());
        assert_eq!(
            std::fs::read_to_string(relaunched).unwrap(),
            bundle.to_string_lossy()
        );
    }

    #[test]
    fn the_helper_leaves_the_app_alone_without_a_staged_copy() {
        let root = tempfile::tempdir().unwrap();
        let bundle = root.path().join("Gasp.app");
        write_app(&bundle, "0.1.0");
        let plan = RestartPlan {
            pid: exited_pid(),
            ..RestartPlan::for_this_process(&bundle)
        };
        let status = Command::new("/bin/sh")
            .arg("-c")
            .arg(restart_script(&plan, "/usr/bin/false"))
            .status()
            .unwrap();
        assert!(!status.success());
        assert!(bundle.join("Contents/Info.plist").exists());
    }

    #[test]
    fn the_plan_for_this_process_names_its_pid() {
        let plan = RestartPlan::for_this_process(Path::new("/Applications/Gasp.app"));
        assert_eq!(plan.pid, std::process::id());
        assert_eq!(plan.staged, PathBuf::from("/Applications/.Gasp-update.app"));
        assert_eq!(plan.aside, PathBuf::from("/Applications/.Gasp-old.app"));
    }
}
