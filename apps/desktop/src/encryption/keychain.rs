//! Touch ID unlock for encrypted notes, on macOS.
//!
//! After a password unlocks a note, the key it made (the vault's, or the
//! note password's) is kept in the login keychain under the service
//! "Gasp encryption", named by the key's id. Reading it back first asks
//! for Touch ID through LocalAuthentication.
//!
//! A keychain item that the system itself guards with Touch ID needs the
//! data protection keychain, which only apps signed with a provisioning
//! profile's keychain-access-groups entitlement can use; a Developer ID
//! build has none. So the item is an ordinary login keychain item, which
//! only this app reads without a system prompt, and Gasp asks for Touch ID
//! itself before reading it.
//!
//! Elsewhere there's no biometric unlock, and every function here says so.

use gasp_crypt::Key;
use gpui::{App, AppContext, Task};

/// The keychain service the kept keys live under.
#[cfg(target_os = "macos")]
const SERVICE: &str = "Gasp encryption";

/// Why Touch ID didn't give a key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TouchIdError {
    /// The person cancelled, or the finger didn't match.
    Declined,
    /// No key is kept for this note yet: the password is needed once.
    NothingKept,
    Failed(String),
}

impl std::fmt::Display for TouchIdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TouchIdError::Declined => write!(
                f,
                "Touch ID didn’t unlock it. Try again or use the password."
            ),
            TouchIdError::NothingKept => {
                write!(
                    f,
                    "Unlock with the password once, then Touch ID can unlock it."
                )
            }
            TouchIdError::Failed(reason) => write!(f, "Touch ID didn’t work: {reason}"),
        }
    }
}

/// Whether keys may go to the keychain at all: never from a snapshot run
/// or a test, which keep away from the person's keychain.
pub(super) fn keeps_keys() -> bool {
    crate::sandbox::keeps_credentials() && crate::sandbox::kept_app_folder("").is_none()
}

/// Whether this Mac can unlock with Touch ID now.
pub fn is_available() -> bool {
    static AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    keeps_keys() && *AVAILABLE.get_or_init(platform::biometrics_available)
}

/// Keeps `key` under `id` for Touch ID to unlock with later.
pub fn keep(id: &str, key: &Key, cx: &App) {
    if !keeps_keys() {
        return;
    }
    let id = id.to_owned();
    let bytes = *key.bytes();
    cx.background_spawn(async move {
        if let Err(error) = platform::store(&id, &bytes) {
            eprintln!("could not keep a key for Touch ID: {error}");
        }
    })
    .detach();
}

/// Forgets the key kept under `id`, as when its password changed.
pub fn forget(id: &str, cx: &App) {
    if !keeps_keys() {
        return;
    }
    let id = id.to_owned();
    cx.background_spawn(async move { platform::delete(&id) })
        .detach();
}

/// Asks for Touch ID, giving `reason`, then reads the key kept under `id`.
pub fn read_with_touch_id(id: String, reason: String, cx: &App) -> Task<Result<Key, TouchIdError>> {
    if !keeps_keys() {
        return Task::ready(Err(TouchIdError::NothingKept));
    }
    let approval = platform::ask_for_touch_id(&reason);
    cx.background_spawn(async move {
        approval.await.map_err(|_| TouchIdError::Declined)??;
        platform::load(&id)
    })
}

#[cfg(target_os = "macos")]
mod platform {
    //! Messages go through `Message::send_message` rather than `msg_send!`,
    //! whose expansion tests a `cargo-clippy` feature this crate doesn't have.

    #![allow(unsafe_code)]

    use std::any::Any;
    use std::ffi::c_void;
    use std::sync::{Arc, Mutex};

    use block::{Block, ConcreteBlock};
    use futures::channel::oneshot;
    use gasp_crypt::Key;
    use objc::runtime::{BOOL, Class, NO, Object, Sel};
    use objc::{Message, MessageArguments};

    use super::{SERVICE, TouchIdError};

    /// `LAPolicyDeviceOwnerAuthenticationWithBiometrics`.
    const BIOMETRICS_POLICY: isize = 1;
    /// `NSUTF8StringEncoding`.
    const UTF8: usize = 4;

    #[link(name = "LocalAuthentication", kind = "framework")]
    unsafe extern "C" {}

    type Reply = Block<(BOOL, *mut Object), ()>;

    pub fn biometrics_available() -> bool {
        let Some(context_class) = Class::get("LAContext") else {
            return false;
        };
        // SAFETY: a new LAContext answers canEvaluatePolicy:error: with a
        // null error pointer, and is released once asked.
        unsafe {
            let context: *mut Object = send(class_object(context_class), "new", ());
            let error: *mut *mut Object = std::ptr::null_mut();
            let available: BOOL = send(
                context,
                "canEvaluatePolicy:error:",
                (BIOMETRICS_POLICY, error),
            );
            let _: () = send(context, "release", ());
            available != NO
        }
    }

    /// Shows the system's Touch ID sheet; the answer arrives once the
    /// person has answered it.
    pub fn ask_for_touch_id(reason: &str) -> oneshot::Receiver<Result<(), TouchIdError>> {
        let (sender, receiver) = oneshot::channel();
        let sender = Arc::new(Mutex::new(Some(sender)));
        let Some(context_class) = Class::get("LAContext") else {
            answer(
                &sender,
                Err(TouchIdError::Failed("no LocalAuthentication".into())),
            );
            return receiver;
        };
        let replied = sender.clone();
        let reply = ConcreteBlock::new(move |success: BOOL, _error: *mut Object| {
            let answered = match success != NO {
                true => Ok(()),
                false => Err(TouchIdError::Declined),
            };
            answer(&replied, answered);
        })
        .copy();
        let reason = reason.as_bytes();
        // SAFETY: the context and string are live for the call; the reply
        // block is copied to the heap and LocalAuthentication retains it
        // until it has run. The context is released after the call; the
        // framework keeps it alive while its sheet shows.
        unsafe {
            let context: *mut Object = send(class_object(context_class), "new", ());
            let string: *mut Object = send(class("NSString"), "alloc", ());
            let string: *mut Object = send(
                string,
                "initWithBytes:length:encoding:",
                (reason.as_ptr().cast::<c_void>(), reason.len(), UTF8),
            );
            let reply: &Reply = &reply;
            let _: () = send(
                context,
                "evaluatePolicy:localizedReason:reply:",
                (BIOMETRICS_POLICY, string, reply as *const Reply),
            );
            let _: () = send(string, "release", ());
            let _: () = send(context, "release", ());
        }
        receiver
    }

    type Answer = Arc<Mutex<Option<oneshot::Sender<Result<(), TouchIdError>>>>>;

    fn answer(sender: &Answer, result: Result<(), TouchIdError>) {
        let taken = sender.lock().ok().and_then(|mut sender| sender.take());
        if let Some(sender) = taken {
            sender.send(result).ok();
        }
    }

    fn entry(id: &str) -> Result<keyring::Entry, TouchIdError> {
        keyring::Entry::new(SERVICE, id).map_err(|error| TouchIdError::Failed(error.to_string()))
    }

    pub fn store(id: &str, bytes: &[u8; 32]) -> Result<(), TouchIdError> {
        entry(id)?
            .set_secret(bytes)
            .map_err(|error| TouchIdError::Failed(error.to_string()))
    }

    pub fn load(id: &str) -> Result<Key, TouchIdError> {
        let secret = match entry(id)?.get_secret() {
            Ok(secret) => secret,
            Err(keyring::Error::NoEntry) => return Err(TouchIdError::NothingKept),
            Err(error) => return Err(TouchIdError::Failed(error.to_string())),
        };
        let bytes: [u8; 32] = secret.try_into().map_err(|_| TouchIdError::NothingKept)?;
        Ok(Key::from_bytes(bytes))
    }

    pub fn delete(id: &str) {
        if let Ok(entry) = entry(id) {
            entry.delete_credential().ok();
        }
    }

    fn class(name: &str) -> *mut Object {
        let class = Class::get(name).unwrap_or_else(|| panic!("Foundation has no {name}"));
        class_object(class)
    }

    fn class_object(class: &Class) -> *mut Object {
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

#[cfg(not(target_os = "macos"))]
mod platform {
    use futures::channel::oneshot;
    use gasp_crypt::Key;

    use super::TouchIdError;

    pub fn biometrics_available() -> bool {
        false
    }

    pub fn ask_for_touch_id(_reason: &str) -> oneshot::Receiver<Result<(), TouchIdError>> {
        let (sender, receiver) = oneshot::channel();
        sender.send(Err(TouchIdError::NothingKept)).ok();
        receiver
    }

    pub fn store(_id: &str, _bytes: &[u8; 32]) -> Result<(), TouchIdError> {
        Ok(())
    }

    pub fn load(_id: &str) -> Result<Key, TouchIdError> {
        Err(TouchIdError::NothingKept)
    }

    pub fn delete(_id: &str) {}
}
