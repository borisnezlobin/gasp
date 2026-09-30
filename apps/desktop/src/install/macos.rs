//! Moving the app on macOS: `ditto` copies the bundle beside its new
//! place and it's renamed in once the old copy is in the Trash, `open`
//! starts the new copy, and `hdiutil` ejects the disk image once this app
//! has quit. A translocated copy's original is asked of the Security
//! framework, which has no public header for it.

#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_int, c_void};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::location::{Placement, Source};
use super::{InstallError, Installer, JUST_INSTALLED_FLAG, Prepared, Progress};

const SYSTEM_APPLICATIONS: &str = "/Applications";
/// Where a copy goes while it's being made, beside its final place.
const PARTIAL_SUFFIX: &str = ".moving";
/// How often a running copy and the copy's size are looked at.
const POLL: Duration = Duration::from_millis(100);
/// How long the Gasp in Applications gets to quit.
const QUIT_TIMEOUT: Duration = Duration::from_secs(15);
/// How long after this app quits the disk image is ejected.
const EJECT_DELAY_SECONDS: &str = "2";
/// The progress shown while `ditto` has written everything but hasn't
/// exited yet.
const NEARLY_DONE: f32 = 0.99;
const PATH_MAX: usize = 1024;
/// `RTLD_LAZY`.
const LAZY: c_int = 1;

pub struct MacInstaller;

impl Installer for MacInstaller {
    fn prepare(&self, placement: &Placement) -> Result<Prepared, InstallError> {
        if placement.source == Source::Development {
            return Err(InstallError::NotABundle);
        }
        let name = placement
            .bundle
            .file_name()
            .ok_or(InstallError::NotABundle)?;
        let folder = applications_folder()?;
        let target = folder.join(name);
        let running = is_running(&target);
        Ok(Prepared { target, running })
    }

    fn quit_running(&self, target: &Path) -> Result<(), InstallError> {
        let script = format!(
            "tell application \"{}\" to quit",
            applescript_quoted(&target.to_string_lossy())
        );
        // A refusal shows as the copy still running, below.
        let _ = Command::new("/usr/bin/osascript")
            .args(["-e", &script])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let started = Instant::now();
        while is_running(target) {
            if started.elapsed() > QUIT_TIMEOUT {
                return Err(InstallError::StillOpen);
            }
            std::thread::sleep(POLL);
        }
        Ok(())
    }

    fn copy(
        &self,
        placement: &Placement,
        target: &Path,
        progress: &Progress,
    ) -> Result<(), InstallError> {
        if is_running(target) {
            return Err(InstallError::StillOpen);
        }
        let partial = partial_path(target);
        remove_partial(&partial)?;
        ditto(&placement.bundle, &partial, progress)?;
        if target.exists() {
            to_trash(target).map_err(|error| InstallError::Replace(error.to_string()))?;
        }
        std::fs::rename(&partial, target).map_err(|error| InstallError::Copy(error.to_string()))?;
        // The copy was checked when it opened; quarantine would have
        // Gatekeeper translocate the new copy too.
        let _ = Command::new("/usr/bin/xattr")
            .args(["-dr", "com.apple.quarantine"])
            .arg(target)
            .stderr(Stdio::null())
            .status();
        progress.set(1.);
        Ok(())
    }

    fn open_and_tidy(&self, placement: &Placement, target: &Path) -> Result<(), InstallError> {
        let status = Command::new("/usr/bin/open")
            .arg("-n")
            .arg(target)
            .args(["--args", JUST_INSTALLED_FLAG])
            .status();
        let opened = match status {
            Ok(status) if status.success() => Ok(()),
            Ok(status) => Err(status.to_string()),
            Err(error) => Err(error.to_string()),
        };
        opened.map_err(|reason| InstallError::Open {
            target: target.to_path_buf(),
            reason,
        })?;
        if let Some(mount) = placement.mount() {
            eject_later(mount);
        }
        if let Some(leftover) = placement.leftover()
            && let Err(error) = to_trash(leftover)
        {
            eprintln!("could not put {} in the Trash: {error}", leftover.display());
        }
        Ok(())
    }
}

/// Puts `path` in the Trash through the file manager, which doesn't ask
/// to control Finder as the trash crate's default does.
fn to_trash(path: &Path) -> Result<(), trash::Error> {
    use trash::macos::{DeleteMethod, TrashContextExtMacos};
    let mut trash = trash::TrashContext::default();
    trash.set_delete_method(DeleteMethod::NsFileManager);
    trash.delete(path)
}

/// `/Applications` when this user can write to it, otherwise
/// `~/Applications`, made if need be.
fn applications_folder() -> Result<PathBuf, InstallError> {
    let system = PathBuf::from(SYSTEM_APPLICATIONS);
    if is_writable(&system) {
        return Ok(system);
    }
    let home =
        dirs::home_dir().ok_or_else(|| InstallError::NoDestination("no home folder".into()))?;
    let personal = home.join("Applications");
    std::fs::create_dir_all(&personal)
        .map_err(|error| InstallError::NoDestination(error.to_string()))?;
    Ok(personal)
}

fn is_writable(folder: &Path) -> bool {
    let probe = folder.join(format!(".gasp-write-test-{}", std::process::id()));
    let written = std::fs::write(&probe, b"").is_ok();
    if written {
        let _ = std::fs::remove_file(&probe);
    }
    written
}

/// Whether a process runs from inside the bundle at `target`.
fn is_running(target: &Path) -> bool {
    let pattern = format!(
        "^{}/Contents/MacOS/",
        regex_quoted(&target.to_string_lossy())
    );
    Command::new("/usr/bin/pgrep")
        .args(["-f", &pattern])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn partial_path(target: &Path) -> PathBuf {
    let mut name = target.file_name().unwrap_or_default().to_os_string();
    name.push(PARTIAL_SUFFIX);
    target.with_file_name(name)
}

fn remove_partial(partial: &Path) -> Result<(), InstallError> {
    match std::fs::remove_dir_all(partial) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            Err(InstallError::Copy(error.to_string()))
        }
        _ => Ok(()),
    }
}

/// Copies `source` to `destination` with `ditto`, which keeps the
/// bundle's signature, links and extended attributes, and reports how
/// many of its bytes have arrived.
fn ditto(source: &Path, destination: &Path, progress: &Progress) -> Result<(), InstallError> {
    let total = folder_size(source).max(1);
    let mut child = Command::new("/usr/bin/ditto")
        .arg(source)
        .arg(destination)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| InstallError::Copy(error.to_string()))?;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                let done = folder_size(destination) as f32 / total as f32;
                progress.set(done.min(NEARLY_DONE));
                std::thread::sleep(POLL);
            }
            Err(error) => return Err(InstallError::Copy(error.to_string())),
        }
    }
    let output = child
        .wait_with_output()
        .map_err(|error| InstallError::Copy(error.to_string()))?;
    if output.status.success() {
        return Ok(());
    }
    let reason = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    let _ = std::fs::remove_dir_all(destination);
    Err(InstallError::Copy(if reason.is_empty() {
        output.status.to_string()
    } else {
        reason
    }))
}

/// The bytes of every file under `path`, not following links.
fn folder_size(path: &Path) -> u64 {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if !metadata.is_dir() {
        return metadata.len();
    }
    std::fs::read_dir(path)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| folder_size(&entry.path()))
                .sum()
        })
        .unwrap_or(0)
}

/// Ejects the disk image at `mount` a moment after this app quits, from
/// a shell that outlives it. The app runs from the image, so it can't
/// be ejected before then.
fn eject_later(mount: &Path) {
    let script = format!(
        "sleep {EJECT_DELAY_SECONDS}; /usr/bin/hdiutil detach -quiet \"$0\" >/dev/null 2>&1"
    );
    let spawned = Command::new("/bin/sh")
        .args(["-c", &script])
        .arg(mount)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    if let Err(error) = spawned {
        eprintln!("could not eject {}: {error}", mount.display());
    }
}

fn applescript_quoted(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

fn regex_quoted(text: &str) -> String {
    text.chars()
        .flat_map(|ch| {
            let special = "\\^$.|?*+()[]{}".contains(ch);
            special.then_some('\\').into_iter().chain([ch])
        })
        .collect()
}

// Translocation ------------------------------------------------------------

type CFTypeRef = *const c_void;
type TranslocatedOriginal = unsafe extern "C" fn(CFTypeRef, *mut CFTypeRef) -> CFTypeRef;

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFURLCreateFromFileSystemRepresentation(
        allocator: CFTypeRef,
        buffer: *const u8,
        length: isize,
        is_directory: u8,
    ) -> CFTypeRef;
    fn CFURLGetFileSystemRepresentation(
        url: CFTypeRef,
        resolve_against_base: u8,
        buffer: *mut u8,
        max_length: isize,
    ) -> u8;
    fn CFRelease(object: CFTypeRef);
}

unsafe extern "C" {
    fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

const SECURITY: &CStr = c"/System/Library/Frameworks/Security.framework/Security";
const ORIGINAL_PATH: &CStr = c"SecTranslocateCreateOriginalPathForURL";

/// The bundle a translocated `bundle` was copied from, as the Security
/// framework reports it.
pub fn original_path(bundle: &Path) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStrExt;
    let bytes = bundle.as_os_str().as_bytes();
    // SAFETY: the symbol is looked up by name and called with the
    // signature Security.framework has exported since macOS 10.12; each
    // URL created here is released once, and the buffer is sized for
    // the call.
    unsafe {
        let handle = dlopen(SECURITY.as_ptr(), LAZY);
        if handle.is_null() {
            return None;
        }
        let symbol = dlsym(handle, ORIGINAL_PATH.as_ptr());
        if symbol.is_null() {
            return None;
        }
        let original_for: TranslocatedOriginal = std::mem::transmute(symbol);
        let url = CFURLCreateFromFileSystemRepresentation(
            std::ptr::null(),
            bytes.as_ptr(),
            bytes.len() as isize,
            1,
        );
        if url.is_null() {
            return None;
        }
        let original = original_for(url, std::ptr::null_mut());
        CFRelease(url);
        if original.is_null() {
            return None;
        }
        let mut buffer = vec![0u8; PATH_MAX];
        let ok =
            CFURLGetFileSystemRepresentation(original, 1, buffer.as_mut_ptr(), PATH_MAX as isize);
        CFRelease(original);
        if ok == 0 {
            return None;
        }
        let path = CStr::from_bytes_until_nul(&buffer).ok()?;
        Some(PathBuf::from(std::ffi::OsStr::from_bytes(path.to_bytes())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_partial_copy_sits_beside_the_target() {
        assert_eq!(
            partial_path(Path::new("/Applications/Gasp.app")),
            PathBuf::from("/Applications/Gasp.app.moving")
        );
    }

    #[test]
    fn paths_are_quoted_for_pgrep_and_applescript() {
        assert_eq!(regex_quoted("/A (1)/Gasp.app"), "/A \\(1\\)/Gasp\\.app");
        assert_eq!(applescript_quoted("a\"b"), "a\\\"b");
    }
}
