//! How a snapshot run keeps AppKit to itself: the app never becomes
//! active or takes a Dock icon, its windows draw at a fixed scale whatever
//! screen is attached, the clipboard is a private one, and light or dark
//! is chosen by the run rather than the system.
//!
//! Messages go through `Message::send_message` rather than `msg_send!`,
//! whose expansion tests a `cargo-clippy` feature this crate doesn't have.

#![allow(unsafe_code)]

use std::any::Any;
use std::sync::atomic::{AtomicPtr, Ordering};

use objc::runtime::{Class, Imp, Object, Sel, class_addMethod, object_getClass};
use objc::{Message, MessageArguments};

/// GPUI's application class, registered when the binary loads.
const APP_CLASS: &str = "GPUIApplication";
/// GPUI's window class, registered when the binary loads.
const WINDOW_CLASS: &str = "GPUIWindow";
/// `NSApplicationActivationPolicyProhibited`: no Dock icon, no menu bar,
/// never the active app.
const ACTIVATION_POLICY_PROHIBITED: isize = 2;

unsafe extern "C" {
    fn class_replaceMethod(
        class: *const Class,
        name: Sel,
        imp: Imp,
        types: *const std::ffi::c_char,
    ) -> Option<Imp>;
}

static PRIVATE_PASTEBOARD: AtomicPtr<Object> = AtomicPtr::new(std::ptr::null_mut());

/// Keeps the app out of the Dock and from ever becoming active, whatever
/// GPUI asks for when it finishes launching. Call before the app runs.
pub fn keep_app_in_background() {
    let Some(class) = Class::get(APP_CLASS) else {
        return;
    };
    // SAFETY: the method has the signature its type encoding (`v@:q`)
    // gives: `-(void)setActivationPolicy:(NSInteger)`. GPUI's class only
    // inherits it from NSApplication, so adding it overrides that.
    unsafe {
        let method: extern "C" fn(&Object, Sel, isize) = set_activation_policy;
        let imp = std::mem::transmute::<extern "C" fn(&Object, Sel, isize), Imp>(method);
        class_addMethod(
            class as *const Class as *mut Class,
            Sel::register("setActivationPolicy:"),
            imp,
            c"v@:q".as_ptr(),
        );
    }
}

extern "C" fn set_activation_policy(app: &Object, _: Sel, _asked: isize) {
    let Some(superclass) = Class::get("NSApplication") else {
        return;
    };
    // SAFETY: NSApplication answers `setActivationPolicy:` with a BOOL,
    // and `app` is the live application object AppKit called us with.
    let _: bool = unsafe {
        objc::__send_super_message(
            app,
            superclass,
            Sel::register("setActivationPolicy:"),
            (ACTIVATION_POLICY_PROHIBITED,),
        )
    }
    .unwrap_or(false);
}

/// Draws every GPUI window at twice its size in points, whatever screen
/// is attached. GPUI takes a window's scale from its screen and uses 2
/// when it has none, so its windows are made to answer that they're on
/// no screen, as a window placed off every screen would. Call before any
/// window opens.
pub fn draw_at_double_scale() {
    let Some(class) = Class::get(WINDOW_CLASS) else {
        return;
    };
    // SAFETY: `-(NSScreen *)screen` takes no arguments and answers an
    // object (`@@:`); GPUI's class inherits it from NSWindow, so adding it
    // overrides that, and nil is an answer NSWindow gives itself.
    unsafe {
        let method: extern "C" fn(&Object, Sel) -> *mut Object = no_screen;
        let imp = std::mem::transmute::<extern "C" fn(&Object, Sel) -> *mut Object, Imp>(method);
        class_addMethod(
            class as *const Class as *mut Class,
            Sel::register("screen"),
            imp,
            c"@@:".as_ptr(),
        );
    }
}

extern "C" fn no_screen(_: &Object, _: Sel) -> *mut Object {
    std::ptr::null_mut()
}

/// Gives the app a clipboard of its own, so copying in a run never
/// changes the system's and pasting only sees what the run copied. GPUI
/// takes the general pasteboard as it starts, so call this before it does.
pub fn use_private_clipboard() {
    let Some(class) = Class::get("NSPasteboard") else {
        return;
    };
    // SAFETY: `+pasteboardWithUniqueName` answers a new autoreleased
    // pasteboard, retained here for the life of the process. The
    // replacement for `+generalPasteboard` has its signature (`@@:`) and
    // is set on the metaclass, where class methods live.
    unsafe {
        let pasteboard: *mut Object = send(
            class as *const Class as *mut Object,
            "pasteboardWithUniqueName",
            (),
        );
        let _: *mut Object = send(pasteboard, "retain", ());
        PRIVATE_PASTEBOARD.store(pasteboard, Ordering::Release);
        let metaclass = object_getClass(class as *const Class as *const Object);
        let method: extern "C" fn(&Object, Sel) -> *mut Object = private_pasteboard;
        let imp = std::mem::transmute::<extern "C" fn(&Object, Sel) -> *mut Object, Imp>(method);
        class_replaceMethod(
            metaclass,
            Sel::register("generalPasteboard"),
            imp,
            c"@@:".as_ptr(),
        );
    }
}

extern "C" fn private_pasteboard(_: &Object, _: Sel) -> *mut Object {
    PRIVATE_PASTEBOARD.load(Ordering::Acquire)
}

/// Removes the private clipboard from the system's pasteboard server.
pub fn release_private_clipboard() {
    let pasteboard = PRIVATE_PASTEBOARD.swap(std::ptr::null_mut(), Ordering::AcqRel);
    if pasteboard.is_null() {
        return;
    }
    // SAFETY: the pasteboard was retained when it was made.
    unsafe {
        let _: () = send(pasteboard, "releaseGlobally", ());
    }
}

/// Draws the app light or dark, as the system appearance would. A vault
/// whose settings choose a theme keeps it. Call once the app is running.
pub fn set_appearance(dark: bool) {
    let (Some(app_class), Some(appearance_class)) =
        (Class::get("NSApplication"), Class::get("NSAppearance"))
    else {
        return;
    };
    let name = match dark {
        true => "NSAppearanceNameDarkAqua",
        false => "NSAppearanceNameAqua",
    };
    // SAFETY: `+sharedApplication` answers the app; `+appearanceNamed:`
    // takes an NSString and answers an appearance or nil, which
    // `setAppearance:` takes (nil follows the system).
    unsafe {
        let app: *mut Object = send(
            app_class as *const Class as *mut Object,
            "sharedApplication",
            (),
        );
        let name = ns_string(name);
        let appearance: *mut Object = send(
            appearance_class as *const Class as *mut Object,
            "appearanceNamed:",
            (name,),
        );
        let _: () = send(app, "setAppearance:", (appearance,));
    }
}

/// An autoreleased NSString with `text`.
///
/// # Safety
///
/// Call on a thread with an autorelease pool, such as the main thread
/// while the app runs.
unsafe fn ns_string(text: &str) -> *mut Object {
    let Some(class) = Class::get("NSString") else {
        return std::ptr::null_mut();
    };
    let text = std::ffi::CString::new(text).unwrap_or_default();
    // SAFETY: `+stringWithUTF8String:` takes a NUL-terminated C string.
    unsafe {
        send(
            class as *const Class as *mut Object,
            "stringWithUTF8String:",
            (text.as_ptr(),),
        )
    }
}

/// Sends `selector` to `receiver` with `args`, answering what it returns.
///
/// # Safety
///
/// `receiver` is a live object, and `args` and `R` match the method's
/// declaration.
pub(super) unsafe fn send<A: MessageArguments, R: Any>(
    receiver: *mut Object,
    selector: &str,
    args: A,
) -> R {
    // SAFETY: as the caller promises.
    let sent = unsafe { (*receiver).send_message(Sel::register(selector), args) };
    sent.unwrap_or_else(|error| panic!("{selector} failed: {error:?}"))
}
