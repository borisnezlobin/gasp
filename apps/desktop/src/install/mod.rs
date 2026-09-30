//! Moving the app into Applications from the app itself. Opened from a
//! disk image, a translocated copy or Downloads, Gasp shows a small
//! window instead of a vault: the whale breaches out of the page, and
//! one button moves Gasp to Applications, opens the moved copy and ejects
//! the disk image. "Not now" opens Gasp from where it is, and isn't asked
//! again for that copy.
//!
//! Where the app runs from ([`location`]), the animation ([`scene`] and
//! [`paint`]) and the window ([`view`]) work on every platform; only the
//! copying, which needs macOS's `ditto`, `open` and `hdiutil`, is
//! macOS's, in [`macos`].

pub mod location;
mod paint;
pub mod scene;
pub mod view;

#[cfg(target_os = "macos")]
mod macos;

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

pub use location::{Declined, Placement, Source};
pub use paint::{WHALE_BODY, WHALE_LIGHT};
pub use view::{InstallHooks, InstallView, Step};

use crate::workspace::state::AppState;

/// Passed to the moved copy when it's opened, so it doesn't ask again.
pub const JUST_INSTALLED_FLAG: &str = "--installed";
/// Opens the install window wherever the app is, for testing and
/// screenshots.
pub const SHOW_INSTALL_FLAG: &str = "--show-install";
/// Holds the install window's page at one moment, such as `breach:0.4`.
pub const FREEZE_VARIABLE: &str = "GASP_INSTALL_FREEZE";

/// The version a "Not now" is remembered for.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Where a copy goes, and whether a Gasp already there is open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prepared {
    pub target: PathBuf,
    pub running: bool,
}

/// The steps of moving the app, each run off the main thread.
pub trait Installer: Send + Sync {
    /// Picks where the copy goes: `/Applications`, or `~/Applications`
    /// when that isn't writable.
    fn prepare(&self, placement: &Placement) -> Result<Prepared, InstallError>;
    /// Asks the Gasp open from `target` to quit, and waits until it has.
    fn quit_running(&self, target: &Path) -> Result<(), InstallError>;
    /// Copies the running bundle to `target`, replacing what's there,
    /// and reports how far it's got in `progress`.
    fn copy(
        &self,
        placement: &Placement,
        target: &Path,
        progress: &Progress,
    ) -> Result<(), InstallError>;
    /// Opens the copy at `target`, then ejects the disk image or puts the
    /// downloaded copy in the Trash once this app has quit.
    fn open_and_tidy(&self, placement: &Placement, target: &Path) -> Result<(), InstallError>;
}

/// How much of the copy is done, from 0 to 1, shared with the thread
/// doing it.
#[derive(Debug, Default)]
pub struct Progress(AtomicU32);

impl Progress {
    pub fn set(&self, done: f32) {
        self.0
            .store(done.clamp(0., 1.).to_bits(), Ordering::Relaxed);
    }

    pub fn get(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }
}

/// What went wrong, worded to say what happened and what to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstallError {
    /// Not on a Mac.
    Unsupported,
    /// Not running from an app bundle, such as a development build.
    NotABundle,
    /// No Applications folder could be written to.
    NoDestination(String),
    Copy(String),
    /// The Gasp already in Applications is still open.
    StillOpen,
    /// The old copy couldn't go to the Trash.
    Replace(String),
    /// The copy is in place but didn't open.
    Open {
        target: PathBuf,
        reason: String,
    },
}

impl InstallError {
    /// Whether trying again could work.
    pub fn can_retry(&self) -> bool {
        !matches!(self, InstallError::Unsupported | InstallError::NotABundle)
    }
}

impl fmt::Display for InstallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InstallError::Unsupported => {
                write!(f, "Moving to Applications only works on a Mac.")
            }
            InstallError::NotABundle => write!(
                f,
                "This is a development build, not an app bundle, so there's nothing to move."
            ),
            InstallError::NoDestination(reason) => write!(
                f,
                "Neither Applications folder could be written to ({reason}). Drag Gasp there in Finder instead."
            ),
            InstallError::Copy(reason) => write!(
                f,
                "Gasp couldn't be copied to Applications ({reason}). Try again, or drag it there in Finder."
            ),
            InstallError::StillOpen => write!(
                f,
                "The Gasp in Applications is still open. Quit it, then try again."
            ),
            InstallError::Replace(reason) => write!(
                f,
                "The old Gasp couldn't go to the Trash ({reason}). Delete it from Applications, then try again."
            ),
            InstallError::Open { target, reason } => write!(
                f,
                "Gasp is in {} now but didn't open ({reason}). Open it from there.",
                target
                    .parent()
                    .map_or(target.as_path(), Path::new)
                    .display()
            ),
        }
    }
}

/// Stands in where the app can't move itself.
pub struct Unsupported;

impl Installer for Unsupported {
    fn prepare(&self, _: &Placement) -> Result<Prepared, InstallError> {
        Err(InstallError::Unsupported)
    }

    fn quit_running(&self, _: &Path) -> Result<(), InstallError> {
        Err(InstallError::Unsupported)
    }

    fn copy(&self, _: &Placement, _: &Path, _: &Progress) -> Result<(), InstallError> {
        Err(InstallError::Unsupported)
    }

    fn open_and_tidy(&self, _: &Placement, _: &Path) -> Result<(), InstallError> {
        Err(InstallError::Unsupported)
    }
}

/// The installer for this platform.
#[cfg(target_os = "macos")]
pub fn platform_installer() -> Arc<dyn Installer> {
    Arc::new(macos::MacInstaller)
}

/// The installer for this platform.
#[cfg(not(target_os = "macos"))]
pub fn platform_installer() -> Arc<dyn Installer> {
    Arc::new(Unsupported)
}

/// The running bundle and the one the user opened, when the system can
/// say (a translocated copy's original).
fn running_bundle() -> Option<(PathBuf, Option<PathBuf>)> {
    let exe = std::env::current_exe().ok()?;
    let bundle = location::bundle_of(&exe)?;
    let original = original_of(&bundle);
    Some((bundle, original))
}

#[cfg(target_os = "macos")]
fn original_of(bundle: &Path) -> Option<PathBuf> {
    location::is_translocated(bundle)
        .then(|| macos::original_path(bundle))
        .flatten()
}

#[cfg(not(target_os = "macos"))]
fn original_of(_: &Path) -> Option<PathBuf> {
    None
}

/// Where the app runs from, when it should offer to move and hasn't been
/// told "Not now" for this copy and version.
pub fn offer_at_launch() -> Option<Placement> {
    let (bundle, original) = running_bundle()?;
    let home = dirs::home_dir();
    let placement = location::placement(&bundle, original.as_deref(), home.as_deref())?;
    let declined = AppState::default_path().and_then(|path| AppState::load(&path).install_declined);
    location::should_offer(&placement, declined.as_ref(), VERSION).then_some(placement)
}

/// Where the app runs from, for `--show-install`, which asks anywhere.
pub fn preview_placement() -> Placement {
    let Some((bundle, original)) = running_bundle() else {
        return Placement {
            bundle: std::env::current_exe().unwrap_or_default(),
            original: None,
            source: Source::Development,
        };
    };
    let home = dirs::home_dir();
    location::placement_for_preview(&bundle, original.as_deref(), home.as_deref())
}

/// Remembers "Not now" for `placement`, so the next launch opens straight away.
pub fn remember_not_now(placement: &Placement) {
    let Some(path) = AppState::default_path() else {
        return;
    };
    let mut state = AppState::load(&path);
    state.install_declined = Some(location::declined(placement, VERSION));
    if let Err(error) = state.save(&path) {
        eprintln!("could not remember Not now: {error}");
    }
}

/// The moment `GASP_INSTALL_FREEZE` holds the page at, if it's set.
pub fn frozen_moment() -> Option<scene::Moment> {
    std::env::var(FREEZE_VARIABLE)
        .ok()
        .and_then(|text| scene::Moment::parse(&text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_is_shared_and_kept_in_range() {
        let progress = Progress::default();
        assert_eq!(progress.get(), 0.);
        progress.set(0.25);
        assert_eq!(progress.get(), 0.25);
        progress.set(3.);
        assert_eq!(progress.get(), 1.);
    }

    #[test]
    fn errors_say_what_to_do() {
        let open = InstallError::Open {
            target: PathBuf::from("/Applications/Gasp.app"),
            reason: "exit 1".into(),
        };
        assert!(open.to_string().contains("Gasp is in /Applications now"));
        assert!(
            InstallError::StillOpen
                .to_string()
                .contains("Quit it, then try again")
        );
        assert!(!InstallError::Unsupported.can_retry());
        assert!(InstallError::Copy("disk full".into()).can_retry());
    }
}
