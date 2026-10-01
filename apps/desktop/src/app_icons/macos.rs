//! AppKit's half of reading an app's icon: `NSWorkspace`'s
//! `iconForFile:`, drawn at a size and written out as a PNG.
//!
//! Messages go through `Message::send_message` rather than `msg_send!`,
//! whose expansion tests a `cargo-clippy` feature this crate doesn't have.

#![allow(unsafe_code)]

use std::any::Any;
use std::ffi::c_void;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use objc::runtime::{Class, Object, Sel};
use objc::{Message, MessageArguments};

/// `NSUTF8StringEncoding`.
const UTF8: usize = 4;

/// `NSBitmapImageFileTypePNG`.
const PNG_FILE_TYPE: usize = 4;

/// AppKit's `NSRect`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct NSRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

/// The icon Finder shows for the file or bundle at `path`, `pixels`
/// square or the nearest size the icon has, as PNG bytes.
pub fn icon_png(path: &Path, pixels: u32) -> Option<Vec<u8>> {
    let path = path.as_os_str().as_bytes();
    // SAFETY: each message matches its AppKit declaration; NSWorkspace,
    // NSImage and NSBitmapImageRep may be used off the main thread, and
    // the pool releases everything made here once the bytes are copied.
    unsafe {
        let pool: *mut Object = send(class("NSAutoreleasePool"), "new", ());
        let png = draw_png(path, pixels);
        let _: () = send(pool, "drain", ());
        png
    }
}

/// # Safety
///
/// Call inside an autorelease pool.
unsafe fn draw_png(path: &[u8], pixels: u32) -> Option<Vec<u8>> {
    // SAFETY: as the caller promises; every object is autoreleased.
    unsafe {
        let workspace: *mut Object = send(class("NSWorkspace"), "sharedWorkspace", ());
        let image: *mut Object = send(workspace, "iconForFile:", (ns_string(path)?,));
        if image.is_null() {
            return None;
        }
        let side = f64::from(pixels);
        let mut rect = NSRect {
            x: 0.,
            y: 0.,
            width: side,
            height: side,
        };
        let nil: *mut Object = std::ptr::null_mut();
        let picture: *const c_void = send(
            image,
            "CGImageForProposedRect:context:hints:",
            (&mut rect as *mut NSRect, nil, nil),
        );
        if picture.is_null() {
            return None;
        }
        let bitmap: *mut Object = send(class("NSBitmapImageRep"), "alloc", ());
        let bitmap: *mut Object = send(bitmap, "initWithCGImage:", (picture,));
        if bitmap.is_null() {
            return None;
        }
        let _: *mut Object = send(bitmap, "autorelease", ());
        let properties: *mut Object = send(class("NSDictionary"), "dictionary", ());
        let data: *mut Object = send(
            bitmap,
            "representationUsingType:properties:",
            (PNG_FILE_TYPE, properties),
        );
        bytes_of(data)
    }
}

/// A copy of an `NSData`'s bytes.
///
/// # Safety
///
/// `data` is nil or a live NSData.
unsafe fn bytes_of(data: *mut Object) -> Option<Vec<u8>> {
    if data.is_null() {
        return None;
    }
    // SAFETY: NSData answers `bytes` with a pointer to `length` bytes that
    // live as long as it does.
    unsafe {
        let length: usize = send(data, "length", ());
        let bytes: *const u8 = send(data, "bytes", ());
        if bytes.is_null() || length == 0 {
            return None;
        }
        Some(std::slice::from_raw_parts(bytes, length).to_vec())
    }
}

/// An autoreleased NSString of the UTF-8 `text`, or `None` when it isn't
/// UTF-8.
///
/// # Safety
///
/// Call inside an autorelease pool.
unsafe fn ns_string(text: &[u8]) -> Option<*mut Object> {
    std::str::from_utf8(text).ok()?;
    // SAFETY: the bytes are valid UTF-8 and live for the call, which
    // copies them.
    unsafe {
        let string: *mut Object = send(class("NSString"), "alloc", ());
        let string: *mut Object = send(
            string,
            "initWithBytes:length:encoding:",
            (text.as_ptr().cast::<c_void>(), text.len(), UTF8),
        );
        (!string.is_null()).then(|| send(string, "autorelease", ()))
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
