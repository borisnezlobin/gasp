//! AppKit's half of Look up. GPUI's view doesn't answer
//! `quickLookWithEvent:`, which AppKit sends for a force click or a
//! three-finger tap, so this adds the method to its class and passes the
//! gesture on; and it shows the dictionary popover with
//! `showDefinitionForAttributedString:atPoint:`.
//!
//! Messages go through `Message::send_message` rather than `msg_send!`,
//! whose expansion tests a `cargo-clippy` feature this crate doesn't have.

#![allow(unsafe_code)]

use std::any::Any;
use std::ffi::c_void;
use std::sync::OnceLock;

use futures::channel::mpsc::UnboundedSender;
use objc::runtime::{Class, Imp, NO, Object, Sel, class_addMethod};
use objc::{Message, MessageArguments};

/// GPUI's view class, registered when the app starts.
const VIEW_CLASS: &str = "GPUIView";

/// `NSUTF8StringEncoding`.
const UTF8: usize = 4;

/// AppKit's `NSPoint`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct NSPoint {
    x: f64,
    y: f64,
}

#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {
    static NSFontAttributeName: *mut Object;
}

/// A force click or three-finger tap: the view it landed on, and where in
/// the view's coordinates, whose y runs up from its bottom edge.
#[derive(Clone, Copy, Debug)]
pub struct Gesture {
    pub view: usize,
    pub x: f64,
    pub y: f64,
}

/// Text for the popover, drawn with its baseline's left end at
/// `baseline` in the view's coordinates.
pub struct Definition<'a> {
    pub text: &'a str,
    pub font_family: &'a str,
    pub font_size: f64,
    pub baseline: (f64, f64),
}

static GESTURES: OnceLock<UnboundedSender<Gesture>> = OnceLock::new();

/// Sends every force click and three-finger tap on a GPUI view to
/// `gestures`. Answers false when it was already listening or the class
/// isn't there.
pub fn listen(gestures: UnboundedSender<Gesture>) -> bool {
    let Some(class) = Class::get(VIEW_CLASS) else {
        return false;
    };
    if GESTURES.set(gestures).is_err() {
        return false;
    }
    let method: extern "C" fn(&Object, Sel, *mut Object) = quick_look_with_event;
    // SAFETY: the method has the signature its type encoding (`v@:@`)
    // gives, `-(void)quickLookWithEvent:(NSEvent *)event`, and adding it
    // leaves the class's other methods alone. NSView only inherits it, so
    // the class itself doesn't have it yet.
    let added = unsafe {
        let imp = std::mem::transmute::<extern "C" fn(&Object, Sel, *mut Object), Imp>(method);
        class_addMethod(
            class as *const Class as *mut Class,
            Sel::register("quickLookWithEvent:"),
            imp,
            c"v@:@".as_ptr(),
        )
    };
    added != NO
}

extern "C" fn quick_look_with_event(view: &Object, _: Sel, event: *mut Object) {
    let Some(gestures) = GESTURES.get() else {
        return;
    };
    let view = view as *const Object as *mut Object;
    let nil: *mut Object = std::ptr::null_mut();
    // SAFETY: AppKit calls this on the main thread with an NSEvent, whose
    // location is in the window's coordinates; nil converts from those.
    let at: NSPoint = unsafe {
        let in_window: NSPoint = send(event, "locationInWindow", ());
        send(view, "convertPoint:fromView:", (in_window, nil))
    };
    let gesture = Gesture {
        view: view as usize,
        x: at.x,
        y: at.y,
    };
    // The app's gone when nothing receives; there's nothing to show then.
    let _ = gestures.unbounded_send(gesture);
}

/// Shows the dictionary popover for `definition` over the NSView at
/// `view`.
pub fn show_definition(view: usize, definition: &Definition<'_>) {
    let view = view as *mut Object;
    let at = NSPoint {
        x: definition.baseline.0,
        y: definition.baseline.1,
    };
    // SAFETY: `view` is a window's live NSView and this runs on the main
    // thread. Each message matches its AppKit declaration, and the pool
    // releases the objects made here once the popover has what it needs.
    unsafe {
        let pool: *mut Object = send(class("NSAutoreleasePool"), "new", ());
        let text = ns_string(definition.text);
        let font = ns_font(definition.font_family, definition.font_size);
        let attributes: *mut Object = send(
            class("NSDictionary"),
            "dictionaryWithObject:forKey:",
            (font, NSFontAttributeName),
        );
        let string: *mut Object = send(class("NSAttributedString"), "alloc", ());
        let string: *mut Object = send(string, "initWithString:attributes:", (text, attributes));
        let _: *mut Object = send(string, "autorelease", ());
        let _: () = send(
            view,
            "showDefinitionForAttributedString:atPoint:",
            (string, at),
        );
        let _: () = send(pool, "drain", ());
    }
}

/// An autoreleased NSString copy of `text`.
///
/// # Safety
///
/// Call on the main thread inside an autorelease pool.
unsafe fn ns_string(text: &str) -> *mut Object {
    // SAFETY: the bytes are valid UTF-8 and live for the call, which
    // copies them.
    unsafe {
        let string: *mut Object = send(class("NSString"), "alloc", ());
        let string: *mut Object = send(
            string,
            "initWithBytes:length:encoding:",
            (text.as_ptr().cast::<c_void>(), text.len(), UTF8),
        );
        send(string, "autorelease", ())
    }
}

/// The font called `family` at `size` points, or the system font when
/// AppKit doesn't know it, such as one the app bundles.
///
/// # Safety
///
/// Call on the main thread inside an autorelease pool.
unsafe fn ns_font(family: &str, size: f64) -> *mut Object {
    // SAFETY: both are class methods taking an NSString and a CGFloat, or
    // a CGFloat, and returning an autoreleased NSFont or nil.
    unsafe {
        let named: *mut Object = if family.is_empty() {
            std::ptr::null_mut()
        } else {
            send(
                class("NSFont"),
                "fontWithName:size:",
                (ns_string(family), size),
            )
        };
        if named.is_null() {
            send(class("NSFont"), "systemFontOfSize:", (size,))
        } else {
            named
        }
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
