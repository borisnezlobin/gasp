//! Moving files to the trash while keeping track of where they went, so a
//! deleted note can come back after the app restarts
//! (`workspace::deleted`), and putting them back.
//!
//! On macOS the system trash goes through `NSFileManager`'s
//! `trashItemAtURL:resultingItemURL:error:`, which says where the file
//! landed. Linux and Windows search their trash by the file's original
//! place instead. The vault's own `.trash` is a folder like any other.

use std::io;
use std::path::{Path, PathBuf};

use gasp_config::settings::TrashMode;

/// Moves `relative` (in the vault at `root`) to the trash the way `mode`
/// says, and returns where it went when that's known: always for the
/// vault's trash and on macOS, never for a file deleted outright.
pub fn move_to_trash(root: &Path, relative: &Path, mode: TrashMode) -> io::Result<Option<PathBuf>> {
    match crate::sandbox::trash_mode(mode) {
        TrashMode::System => system_trash(&root.join(relative)),
        TrashMode::Vault => gasp_vault::ops::move_to_vault_trash(root, relative).map(Some),
        TrashMode::Delete => {
            gasp_vault::ops::trash(root, relative, TrashMode::Delete).map(|()| None)
        }
    }
}

/// Moves a file that went to the trash to `destination`.
pub fn put_back(trashed: &Path, destination: &Path) -> io::Result<()> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match std::fs::rename(trashed, destination) {
        Ok(()) => Ok(()),
        // The trash can be on another volume than the vault.
        Err(_) if trashed.is_file() => {
            std::fs::copy(trashed, destination)?;
            std::fs::remove_file(trashed)
        }
        Err(error) => Err(error),
    }
}

#[cfg(target_os = "macos")]
fn system_trash(path: &Path) -> io::Result<Option<PathBuf>> {
    macos::trash_item(path).map(Some)
}

#[cfg(not(target_os = "macos"))]
fn system_trash(path: &Path) -> io::Result<Option<PathBuf>> {
    trash::delete(path).map_err(io::Error::other)?;
    Ok(None)
}

/// Puts back the newest item in the system's trash that came from
/// `original`, where the trash can be searched by it. Returns whether
/// there was one.
#[cfg(any(
    target_os = "windows",
    all(
        unix,
        not(target_os = "macos"),
        not(target_os = "ios"),
        not(target_os = "android")
    )
))]
pub fn restore_from_system_trash(original: &Path) -> io::Result<bool> {
    let items = trash::os_limited::list().map_err(io::Error::other)?;
    let newest = items
        .into_iter()
        .filter(|item| item.original_path() == original)
        .max_by_key(|item| item.time_deleted);
    let Some(item) = newest else {
        return Ok(false);
    };
    trash::os_limited::restore_all([item]).map_err(io::Error::other)?;
    Ok(true)
}

/// Where the system's trash can't be searched, only a known place helps.
#[cfg(not(any(
    target_os = "windows",
    all(
        unix,
        not(target_os = "macos"),
        not(target_os = "ios"),
        not(target_os = "android")
    )
)))]
pub fn restore_from_system_trash(_original: &Path) -> io::Result<bool> {
    Ok(false)
}

#[cfg(target_os = "macos")]
mod macos {
    //! Messages go through `Message::send_message` rather than `msg_send!`,
    //! whose expansion tests a `cargo-clippy` feature this crate doesn't have.

    #![allow(unsafe_code)]

    use std::any::Any;
    use std::ffi::{CStr, c_char, c_void};
    use std::io;
    use std::os::unix::ffi::OsStrExt;
    use std::path::{Path, PathBuf};

    use objc::runtime::{BOOL, Class, NO, Object, Sel};
    use objc::{Message, MessageArguments};

    /// `NSUTF8StringEncoding`.
    const UTF8: usize = 4;

    /// Moves `path` to the trash and returns where it landed.
    pub fn trash_item(path: &Path) -> io::Result<PathBuf> {
        let bytes = path.as_os_str().as_bytes();
        // SAFETY: every message goes to a live object with arguments of the
        // types its method declares, inside an autorelease pool drained at
        // the end; the strings read back are copied before the pool goes.
        unsafe {
            let pool: *mut Object = send(class("NSAutoreleasePool"), "new", ());
            let result = trash_in_pool(bytes);
            let _: () = send(pool, "drain", ());
            result
        }
    }

    /// # Safety
    ///
    /// Call inside an autorelease pool.
    unsafe fn trash_in_pool(path: &[u8]) -> io::Result<PathBuf> {
        // SAFETY: as the caller promises.
        unsafe {
            let string: *mut Object = send(class("NSString"), "alloc", ());
            let string: *mut Object = send(
                string,
                "initWithBytes:length:encoding:",
                (path.as_ptr().cast::<c_void>(), path.len(), UTF8),
            );
            if string.is_null() {
                return Err(io::Error::other("the path isn’t valid UTF-8"));
            }
            let _: *mut Object = send(string, "autorelease", ());
            let url: *mut Object = send(class("NSURL"), "fileURLWithPath:", (string,));
            let manager: *mut Object = send(class("NSFileManager"), "defaultManager", ());
            let mut landed: *mut Object = std::ptr::null_mut();
            let mut error: *mut Object = std::ptr::null_mut();
            let trashed: BOOL = send(
                manager,
                "trashItemAtURL:resultingItemURL:error:",
                (
                    url,
                    &mut landed as *mut *mut Object,
                    &mut error as *mut *mut Object,
                ),
            );
            if trashed == NO || landed.is_null() {
                return Err(io::Error::other(describe(error)));
            }
            let place: *const c_char = send(landed, "fileSystemRepresentation", ());
            let place = CStr::from_ptr(place).to_bytes();
            Ok(PathBuf::from(std::ffi::OsStr::from_bytes(place)))
        }
    }

    /// What an `NSError` says, or a stand-in when there's none.
    ///
    /// # Safety
    ///
    /// `error` is null or a live `NSError`.
    unsafe fn describe(error: *mut Object) -> String {
        if error.is_null() {
            return "the system didn’t move it to the trash".to_owned();
        }
        // SAFETY: as the caller promises.
        unsafe {
            let text: *mut Object = send(error, "localizedDescription", ());
            let text: *const c_char = send(text, "UTF8String", ());
            CStr::from_ptr(text).to_string_lossy().into_owned()
        }
    }

    fn class(name: &str) -> *mut Object {
        let class = Class::get(name).unwrap_or_else(|| panic!("Foundation has no {name}"));
        class as *const Class as *mut Object
    }

    /// Sends `selector` to `receiver` with `args`, answering what it returns.
    ///
    /// # Safety
    ///
    /// `receiver` is a live object or class, and `args` and `R` match the
    /// method's declaration.
    unsafe fn send<A: MessageArguments, R: Any>(
        receiver: *mut Object,
        selector: &str,
        args: A,
    ) -> R {
        // SAFETY: as the caller promises.
        let sent = unsafe { (*receiver).send_message(Sel::register(selector), args) };
        sent.unwrap_or_else(|error| panic!("{selector} failed: {error:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_vault_trash_says_where_a_note_went_and_gives_it_back() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("Plan.md"), "plan").unwrap();
        let landed = move_to_trash(root.path(), Path::new("Plan.md"), TrashMode::Vault)
            .unwrap()
            .expect("the vault's trash is a known place");
        assert_eq!(landed, root.path().join(".trash/Plan.md"));
        put_back(&landed, &root.path().join("Plan.md")).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.path().join("Plan.md")).unwrap(),
            "plan"
        );
        assert!(!landed.exists());
    }

    /// Uses the real system trash, so it only runs when asked for:
    /// `cargo test -p gasp-desktop --lib trash -- --ignored`. The file
    /// comes back out, leaving the trash as it was.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "moves a temporary file through the system trash"]
    fn the_system_trash_says_where_a_file_went() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("Trash check.md"), "check").unwrap();
        let landed = move_to_trash(root.path(), Path::new("Trash check.md"), TrashMode::System)
            .unwrap()
            .expect("macOS says where it went");
        assert!(landed.exists(), "{}", landed.display());
        assert!(!root.path().join("Trash check.md").exists());
        put_back(&landed, &root.path().join("Trash check.md")).unwrap();
        assert!(!landed.exists());
        assert_eq!(
            std::fs::read_to_string(root.path().join("Trash check.md")).unwrap(),
            "check"
        );
    }

    #[test]
    fn deleting_outright_leaves_no_place() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("Gone.md"), "").unwrap();
        let landed = move_to_trash(root.path(), Path::new("Gone.md"), TrashMode::Delete).unwrap();
        assert_eq!(landed, None);
        assert!(!root.path().join("Gone.md").exists());
    }
}
