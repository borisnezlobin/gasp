//! Putting HTML and its plain text on the macOS pasteboard together, which
//! GPUI's clipboard (plain text and images only) can't: Mail, Pages and
//! Google Docs paste the HTML, and plain-text fields the text.
//!
//! Messages go through `objc`'s `Message::send_message` rather than its
//! macros, whose expansion trips `unexpected_cfgs`, as in
//! `crate::print::macos`.
#![allow(unsafe_code)]

use std::any::Any;
use std::ffi::c_void;

use anyhow::{Context as _, anyhow, bail};
use objc::runtime::{BOOL, Class, NO, Object, Sel};
use objc::{Message, MessageArguments};

type Id = *mut Object;

/// `NSUTF8StringEncoding`.
const UTF8_ENCODING: usize = 4;
/// `NSPasteboardTypeHTML` and `NSPasteboardTypeString`, by value.
const HTML_TYPE: &str = "public.html";
const TEXT_TYPE: &str = "public.utf8-plain-text";

/// Sends `selector` with `args` to `receiver`.
///
/// # Safety
///
/// `receiver` must respond to `selector`, and `args` and `R` must be the
/// types of its declaration.
unsafe fn send<M: Message, A: MessageArguments, R: Any>(
    receiver: &M,
    selector: &str,
    args: A,
) -> anyhow::Result<R> {
    // SAFETY: as the caller promises.
    unsafe { receiver.send_message(Sel::register(selector), args) }
        .map_err(|error| anyhow!("{selector} failed: {error:?}"))
}

/// A retained `NSString` copy of `text`; the caller releases it.
///
/// # Safety
///
/// Main thread only.
unsafe fn ns_string<'a>(text: &str) -> anyhow::Result<&'a Object> {
    let class = Class::get("NSString").context("macOS has no NSString")?;
    // SAFETY: `alloc` and `initWithBytes:length:encoding:` are
    // `NSString`'s own; the bytes live for the call, which copies them.
    unsafe {
        let string: Id = send(class, "alloc", ())?;
        let string = string.as_ref().context("macOS couldn’t make a string")?;
        let string: Id = send(
            string,
            "initWithBytes:length:encoding:",
            (text.as_ptr().cast::<c_void>(), text.len(), UTF8_ENCODING),
        )?;
        string.as_ref().context("the text isn’t valid UTF-8")
    }
}

/// Replaces what's on the general pasteboard with `html`, and `plain` for
/// places that take only text.
pub fn copy_html_and_text(html: &str, plain: &str) -> anyhow::Result<()> {
    let class = Class::get("NSPasteboard").context("macOS has no NSPasteboard")?;
    // SAFETY: runs on the main thread (GPUI's foreground executor); each
    // message is `NSPasteboard`'s own with its declared types, and the
    // strings made here are released after the pasteboard copies them.
    unsafe {
        let pasteboard: Id = send(class, "generalPasteboard", ())?;
        let pasteboard = pasteboard.as_ref().context("there’s no pasteboard")?;
        let _: isize = send(pasteboard, "clearContents", ())?;
        for (text, kind) in [(html, HTML_TYPE), (plain, TEXT_TYPE)] {
            let string = ns_string(text)?;
            let kind_string = ns_string(kind)?;
            let written: BOOL = send(
                pasteboard,
                "setString:forType:",
                (string as *const Object, kind_string as *const Object),
            )?;
            let _: () = send(string, "release", ())?;
            let _: () = send(kind_string, "release", ())?;
            if written == NO {
                bail!("the pasteboard didn’t take the {kind}");
            }
        }
    }
    Ok(())
}
