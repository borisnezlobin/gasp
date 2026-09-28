//! AppKit's half of moving the window: GPUI's view stops saying a press
//! on it can move the window, and a press on the tab bar's empty space
//! hands the current event to `performWindowDragWithEvent:`.
//!
//! Messages go through `Message::send_message` rather than `msg_send!`,
//! whose expansion tests a `cargo-clippy` feature this crate doesn't have.

#![allow(unsafe_code)]

use std::any::Any;

use objc::runtime::{BOOL, Class, Imp, NO, Object, Sel, class_addMethod};
use objc::{Message, MessageArguments};

/// GPUI's view class, registered when the app starts.
const VIEW_CLASS: &str = "GPUIView";

/// Gives GPUI's view `-(BOOL)mouseDownCanMoveWindow` answering NO.
/// Answers whether it could: false when the class is missing or already
/// has the method.
pub fn keep_presses_in_the_view() -> bool {
    let Some(class) = Class::get(VIEW_CLASS) else {
        return false;
    };
    let method: extern "C" fn(&Object, Sel) -> BOOL = cannot_move_window;
    // SAFETY: the method has the signature its type encoding (`c@:`, a
    // BOOL from `self` and `_cmd`) gives; NSView only inherits it, so the
    // class doesn't have its own yet, and adding it leaves the rest alone.
    let added = unsafe {
        let imp = std::mem::transmute::<extern "C" fn(&Object, Sel) -> BOOL, Imp>(method);
        let types = if cfg!(target_arch = "aarch64") {
            c"B@:"
        } else {
            c"c@:"
        };
        class_addMethod(
            class as *const Class as *mut Class,
            Sel::register("mouseDownCanMoveWindow"),
            imp,
            types.as_ptr(),
        )
    };
    added != NO
}

extern "C" fn cannot_move_window(_: &Object, _: Sel) -> BOOL {
    NO
}

/// Moves the window of the NSView at `view` with the pointer, from the
/// mouse-down AppKit is handling now.
pub fn drag_window(view: usize) {
    let view = view as *mut Object;
    // SAFETY: this runs on the main thread inside AppKit's handling of a
    // mouse-down, so `currentEvent` is that event; `view` is a window's
    // live NSView. Each message matches its AppKit declaration.
    unsafe {
        let app: *mut Object = send(class("NSApplication"), "sharedApplication", ());
        let event: *mut Object = send(app, "currentEvent", ());
        let window: *mut Object = send(view, "window", ());
        if event.is_null() || window.is_null() {
            return;
        }
        let _: () = send(window, "performWindowDragWithEvent:", (event,));
    }
}

/// The class called `name`, as an object to send messages to.
fn class(name: &str) -> *mut Object {
    let class = Class::get(name).unwrap_or_else(|| panic!("AppKit has no {name}"));
    class as *const Class as *mut Object
}

/// Sends `selector` to `receiver` with `args`, answering what it returns.
///
/// # Safety
///
/// `receiver` is a live object or class, and `args` and `R` match the
/// method's declaration.
unsafe fn send<A: MessageArguments, R: Any>(receiver: *mut Object, selector: &str, args: A) -> R {
    // SAFETY: as the caller promises.
    let sent = unsafe { (*receiver).send_message(Sel::register(selector), args) };
    sent.unwrap_or_else(|error| panic!("{selector} failed: {error:?}"))
}
