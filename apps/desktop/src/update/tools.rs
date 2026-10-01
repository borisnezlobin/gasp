//! The macOS tools an update runs: `hdiutil` to open the disk image,
//! `codesign` and `spctl` to check the app in it, and `ditto` to copy it
//! out. [`SystemTools`] is what the rest of the update sees, so tests can
//! stand in for all of them.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use super::verify::{SigningIdentity, mount_point, parse_signing_identity};

pub trait SystemTools {
    /// Attaches `dmg` read-only and out of Finder's sight, and answers
    /// where it's mounted.
    fn attach(&self, dmg: &Path) -> Result<PathBuf, String>;
    fn detach(&self, mount: &Path);
    /// Whether `codesign --verify --deep --strict` accepts `app`.
    fn signature_is_valid(&self, app: &Path) -> bool;
    /// Who signed `app`, from `codesign -dv`.
    fn signing_identity(&self, app: &Path) -> Option<SigningIdentity>;
    /// Whether Gatekeeper would open `app`, which needs it notarized.
    fn gatekeeper_accepts(&self, app: &Path) -> bool;
    /// Copies the bundle at `from` to `to`, keeping its signature intact.
    fn copy_app(&self, from: &Path, to: &Path) -> Result<(), String>;
}

/// The real tools, from `/usr/bin`.
pub struct MacTools;

const HDIUTIL: &str = "/usr/bin/hdiutil";
const CODESIGN: &str = "/usr/bin/codesign";
const SPCTL: &str = "/usr/sbin/spctl";
const DITTO: &str = "/usr/bin/ditto";

impl SystemTools for MacTools {
    fn attach(&self, dmg: &Path) -> Result<PathBuf, String> {
        let output = run(Command::new(HDIUTIL)
            .args(["attach", "-nobrowse", "-readonly", "-noautoopen", "-plist"])
            .arg(dmg))?;
        mount_point(&output.stdout).ok_or_else(|| "the disk image has no volume".to_string())
    }

    fn detach(&self, mount: &Path) {
        let _ = run(Command::new(HDIUTIL)
            .args(["detach", "-quiet", "-force"])
            .arg(mount));
    }

    fn signature_is_valid(&self, app: &Path) -> bool {
        run(Command::new(CODESIGN)
            .args(["--verify", "--deep", "--strict"])
            .arg(app))
        .is_ok()
    }

    fn signing_identity(&self, app: &Path) -> Option<SigningIdentity> {
        let output = Command::new(CODESIGN)
            .args(["-dv", "--verbose=2"])
            .arg(app)
            .output()
            .ok()?;
        parse_signing_identity(&String::from_utf8_lossy(&output.stderr))
    }

    fn gatekeeper_accepts(&self, app: &Path) -> bool {
        run(Command::new(SPCTL)
            .args(["--assess", "--type", "execute"])
            .arg(app))
        .is_ok()
    }

    fn copy_app(&self, from: &Path, to: &Path) -> Result<(), String> {
        run(Command::new(DITTO).arg(from).arg(to)).map(|_| ())
    }
}

fn run(command: &mut Command) -> Result<Output, String> {
    let output = command.output().map_err(|error| error.to_string())?;
    if output.status.success() {
        return Ok(output);
    }
    let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(if message.is_empty() {
        output.status.to_string()
    } else {
        message
    })
}
