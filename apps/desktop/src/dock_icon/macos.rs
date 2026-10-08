//! AppKit's half of the Dock icon: `NSApplication`'s
//! `setApplicationIconImage:`, with an `NSImage` made from PNG bytes, or
//! nil for the bundle's own icon.
//!
//! Messages go through `Message::send_message` rather than `msg_send!`,
//! whose expansion tests a `cargo-clippy` feature this crate doesn't have.

#![allow(unsafe_code)]

use std::any::Any;
use std::ffi::c_void;

use objc::runtime::{BOOL, Class, NO, Object, Sel};
use objc::{Message, MessageArguments};

/// Shows the PNG `png` in the Dock, or the bundle's icon for `None`.
/// AppKit only takes this on the main thread; anywhere else, such as a
/// test's thread, it does nothing.
pub fn set_dock_png(png: Option<&[u8]>) {
    // SAFETY: each message matches its AppKit declaration, and they're
    // only sent on the main thread. The pool releases the image and data
    // once the application has kept its own reference.
    unsafe {
        let on_main_thread: BOOL = send(class("NSThread"), "isMainThread", ());
        if on_main_thread == NO {
            return;
        }
        let pool: *mut Object = send(class("NSAutoreleasePool"), "new", ());
        let image = png.map_or(std::ptr::null_mut(), |png| image_of(png));
        let app: *mut Object = send(class("NSApplication"), "sharedApplication", ());
        let _: () = send(app, "setApplicationIconImage:", (image,));
        let _: () = send(pool, "drain", ());
    }
}

/// An autoreleased NSImage of the PNG `png`, or nil when it doesn't
/// decode.
///
/// # Safety
///
/// Call inside an autorelease pool.
unsafe fn image_of(png: &[u8]) -> *mut Object {
    // SAFETY: as the caller promises; `dataWithBytes:length:` copies the
    // bytes, and every object is autoreleased.
    unsafe {
        let data: *mut Object = send(
            class("NSData"),
            "dataWithBytes:length:",
            (png.as_ptr().cast::<c_void>(), png.len()),
        );
        let image: *mut Object = send(class("NSImage"), "alloc", ());
        let image: *mut Object = send(image, "initWithData:", (data,));
        if image.is_null() {
            return image;
        }
        send(image, "autorelease", ())
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
