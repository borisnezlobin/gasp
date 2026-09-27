//! The macOS print panel for a PDF, through PDFKit: the PDF is opened as a
//! `PDFDocument`, which makes its own print operation, and that runs as a
//! sheet on the note's window with the standard print panel.
//!
//! Kept minimal on purpose: a handful of messages, no delegates and no
//! callbacks. Messages go through `objc`'s `Message::send_message`
//! rather than its macros, whose expansion trips `unexpected_cfgs`.
#![allow(unsafe_code)]

use std::any::Any;
use std::ffi::c_void;

use anyhow::{Context as _, anyhow, bail};
use objc::runtime::{Class, Object, Sel, YES};
use objc::{Message, MessageArguments};

// `PDFDocument` lives in PDFKit, which GPUI doesn't link.
#[link(name = "PDFKit", kind = "framework")]
unsafe extern "C" {}

type Id = *mut Object;

/// `NSUTF8StringEncoding`.
const UTF8_ENCODING: usize = 4;

/// `kPDFPrintPageScaleDownToFit`: pages larger than the printer's paper
/// shrink to fit; others print at their size.
const SCALE_DOWN_TO_FIT: isize = 2;

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

fn class(name: &str) -> anyhow::Result<&'static Class> {
    Class::get(name).with_context(|| format!("macOS has no {name}"))
}

/// An object from a pointer that must not be null.
///
/// # Safety
///
/// `object` must be null or point to a live Objective-C object.
unsafe fn object<'a>(object: Id, what: &str) -> anyhow::Result<&'a Object> {
    // SAFETY: as the caller promises.
    unsafe { object.as_ref() }.with_context(|| format!("macOS couldn’t make {what}"))
}

/// Shows the print panel for `pdf` as a sheet on the window of the
/// `NSView` `view`, titled `title`. Returns once the sheet is up; the
/// panel prints or cancels by itself.
pub fn show_print_panel(view: *mut c_void, pdf: &[u8], title: &str) -> anyhow::Result<()> {
    // SAFETY: this runs on the main thread (GPUI's foreground executor).
    // Every message goes to an object of the class that declares it, with
    // the argument and return types of its declaration; `view` is the
    // window's live `NSView`.
    unsafe {
        let view = object(view.cast::<Object>(), "the window’s view")?;
        let window: Id = send(view, "window", ())?;
        let window = object(window, "the note’s window")?;
        let document = pdf_document(pdf)?;
        let operation = print_operation(document, title)?;
        run_as_sheet(operation, window)
    }
}

/// `pdf` as a `PDFDocument`. It stays retained: the sheet prints from it
/// after [`show_print_panel`] returns, and one PDF per print is a small
/// price for never freeing it early.
///
/// # Safety
///
/// Main thread only.
unsafe fn pdf_document<'a>(pdf: &[u8]) -> anyhow::Result<&'a Object> {
    let document_class = class("PDFDocument").context("PDFKit isn’t available")?;
    // SAFETY: `dataWithBytes:length:` copies the bytes; `alloc` and
    // `initWithData:` are `PDFDocument`'s own.
    unsafe {
        let data: Id = send(
            class("NSData")?,
            "dataWithBytes:length:",
            (pdf.as_ptr().cast::<c_void>(), pdf.len()),
        )?;
        let document: Id = send(document_class, "alloc", ())?;
        let document: Id = send(
            object(document, "a PDF document")?,
            "initWithData:",
            (data,),
        )?;
        object(document, "a PDF document from the export")
    }
}

/// The document's print job, with the print panel and progress shown.
///
/// # Safety
///
/// `document` must be a `PDFDocument`; main thread only.
unsafe fn print_operation<'a>(document: &Object, title: &str) -> anyhow::Result<&'a Object> {
    // SAFETY: as the caller promises; the messages are `PDFDocument`'s
    // and `NSPrintOperation`'s own.
    unsafe {
        let info: Id = send(class("NSPrintInfo")?, "sharedPrintInfo", ())?;
        let operation: Id = send(
            document,
            "printOperationForPrintInfo:scalingMode:autoRotate:",
            (info, SCALE_DOWN_TO_FIT, YES),
        )?;
        let operation = object(operation, "a print job")?;
        set_job_title(operation, title)?;
        let _: () = send(operation, "setShowsPrintPanel:", (YES,))?;
        let _: () = send(operation, "setShowsProgressPanel:", (YES,))?;
        Ok(operation)
    }
}

/// Runs the job as a sheet on `window`: no delegate, no callback.
///
/// # Safety
///
/// `operation` must be an `NSPrintOperation` and `window` an `NSWindow`;
/// main thread only.
unsafe fn run_as_sheet(operation: &Object, window: &Object) -> anyhow::Result<()> {
    let window: *const Object = window;
    let no_delegate: Id = std::ptr::null_mut();
    let no_context: *mut c_void = std::ptr::null_mut();
    // SAFETY: as the caller promises; a null selector means no callback.
    unsafe {
        let no_selector = Sel::from_ptr(std::ptr::null());
        send(
            operation,
            "runOperationModalForWindow:delegate:didRunSelector:contextInfo:",
            (window, no_delegate, no_selector, no_context),
        )
    }
}

/// Names the print job after the note, as the queue and the PDF option
/// in the panel show it.
///
/// # Safety
///
/// `operation` must be an `NSPrintOperation`.
unsafe fn set_job_title(operation: &Object, title: &str) -> anyhow::Result<()> {
    // SAFETY: as the caller promises; the string is made and released
    // here, and the operation copies it.
    unsafe {
        let string: Id = send(class("NSString")?, "alloc", ())?;
        let string: Id = send(
            object(string, "a string")?,
            "initWithBytes:length:encoding:",
            (title.as_ptr().cast::<c_void>(), title.len(), UTF8_ENCODING),
        )?;
        let Some(string) = string.as_ref() else {
            bail!("The note’s name isn’t valid text");
        };
        let _: () = send(operation, "setJobTitle:", (string as *const Object,))?;
        let _: () = send(string, "release", ())?;
    }
    Ok(())
}
