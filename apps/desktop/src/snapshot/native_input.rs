//! Pointer events delivered the way AppKit delivers them: as `NSEvent`s
//! sent to GPUI's view, which turns them into GPUI events exactly as it
//! does for a person's mouse or trackpad. GPUI keeps the rest of that
//! path to itself, so nothing here goes around it.
//!
//! Each event starts as a Core Graphics event, the kind the system makes
//! for the mouse, so it carries its button, click count and modifiers as
//! a real one would. Core Graphics places events on the screen, and an
//! event for a window that's on no screen reads its position as a screen
//! position, so each is placed by measuring where a first try lands.

#![allow(unsafe_code)]

use std::ffi::c_void;

use gpui::{MouseButton, Pixels, PlatformInput, Point, ScrollDelta};
use objc::runtime::{Class, Object};
use objc::{Encode, Encoding};

use super::appkit::send;

/// `CGEventType` values, the same numbers as `NSEventType`'s.
const LEFT_MOUSE_DOWN: u32 = 1;
const LEFT_MOUSE_UP: u32 = 2;
const RIGHT_MOUSE_DOWN: u32 = 3;
const RIGHT_MOUSE_UP: u32 = 4;
const MOUSE_MOVED: u32 = 5;
const LEFT_MOUSE_DRAGGED: u32 = 6;
/// `CGMouseButton` values.
const LEFT_BUTTON: u32 = 0;
const RIGHT_BUTTON: u32 = 1;
/// `kCGMouseEventClickState`: how many clicks this press is.
const CLICK_STATE_FIELD: u32 = 1;
/// `kCGScrollEventUnitPixel`.
const SCROLL_IN_PIXELS: u32 = 0;
/// How far an event may land from where it was asked for, in points.
const PLACEMENT_TOLERANCE: f64 = 0.5;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct NativePoint {
    x: f64,
    y: f64,
}

// SAFETY: the layout is CGPoint's: two doubles.
unsafe impl Encode for NativePoint {
    fn encode() -> Encoding {
        // SAFETY: a valid encoding for a struct of two doubles.
        unsafe { Encoding::from_str("{CGPoint=dd}") }
    }
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventCreateMouseEvent(
        source: *const c_void,
        kind: u32,
        location: NativePoint,
        button: u32,
    ) -> *mut c_void;
    fn CGEventCreateScrollWheelEvent2(
        source: *const c_void,
        units: u32,
        wheel_count: u32,
        wheel1: i32,
        wheel2: i32,
        wheel3: i32,
    ) -> *mut c_void;
    fn CGEventSetLocation(event: *mut c_void, location: NativePoint);
    fn CGEventSetIntegerValueField(event: *mut c_void, field: u32, value: i64);
    fn CGEventSetFlags(event: *mut c_void, flags: u64);
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(object: *const c_void);
}

/// What one pointer event is to AppKit.
struct NativeEvent {
    /// The view's method that takes it.
    selector: &'static str,
    kind: u32,
    button: u32,
    position: Point<Pixels>,
    modifiers: gpui::Modifiers,
    clicks: usize,
}

/// GPUI's view in a window, taking events.
pub struct NativePointer {
    view: *mut Object,
}

impl NativePointer {
    /// `view` is GPUI's NSView for a window that's open.
    pub fn new(view: usize) -> Self {
        Self {
            view: view as *mut Object,
        }
    }

    /// Sends a pointer event to the view, at a position in window points
    /// from the top-left of a view `height` points tall.
    pub fn send(&self, event: &PlatformInput, height: Pixels) -> Result<(), String> {
        let height = f64::from(f32::from(height));
        objc::rc::autoreleasepool(|| {
            let (selector, native) = match event {
                PlatformInput::ScrollWheel(scroll) => (
                    "scrollWheel:",
                    scroll_event(scroll.position, scroll.delta, height)?,
                ),
                event => {
                    let described = describe(event)?;
                    (described.selector, mouse_event(&described, height)?)
                }
            };
            // SAFETY: GPUI's view answers each of these selectors with
            // `-(void)…:(NSEvent *)`.
            unsafe {
                let _: () = send(self.view, selector, (native,));
            }
            Ok(())
        })
    }
}

fn describe(event: &PlatformInput) -> Result<NativeEvent, String> {
    let (selector, kind, button, position, modifiers, clicks) = match event {
        PlatformInput::MouseMove(moved) => {
            let (selector, kind) = match moved.pressed_button {
                None => ("mouseMoved:", MOUSE_MOVED),
                Some(MouseButton::Left) => ("mouseDragged:", LEFT_MOUSE_DRAGGED),
                Some(_) => return Err("only the left button drags".to_owned()),
            };
            (
                selector,
                kind,
                LEFT_BUTTON,
                moved.position,
                moved.modifiers,
                0,
            )
        }
        PlatformInput::MouseDown(down) => {
            let (selector, kind, button) = button_event(down.button, true)?;
            (
                selector,
                kind,
                button,
                down.position,
                down.modifiers,
                down.click_count,
            )
        }
        PlatformInput::MouseUp(up) => {
            let (selector, kind, button) = button_event(up.button, false)?;
            (
                selector,
                kind,
                button,
                up.position,
                up.modifiers,
                up.click_count,
            )
        }
        _ => return Err("that isn't a pointer event".to_owned()),
    };
    Ok(NativeEvent {
        selector,
        kind,
        button,
        position,
        modifiers,
        clicks,
    })
}

/// The view's selector, the event type and the button for a button going
/// down or up.
fn button_event(button: MouseButton, down: bool) -> Result<(&'static str, u32, u32), String> {
    match (button, down) {
        (MouseButton::Left, true) => Ok(("mouseDown:", LEFT_MOUSE_DOWN, LEFT_BUTTON)),
        (MouseButton::Left, false) => Ok(("mouseUp:", LEFT_MOUSE_UP, LEFT_BUTTON)),
        (MouseButton::Right, true) => Ok(("rightMouseDown:", RIGHT_MOUSE_DOWN, RIGHT_BUTTON)),
        (MouseButton::Right, false) => Ok(("rightMouseUp:", RIGHT_MOUSE_UP, RIGHT_BUTTON)),
        _ => Err("only the left and right buttons click".to_owned()),
    }
}

fn mouse_event(event: &NativeEvent, height: f64) -> Result<*mut Object, String> {
    // SAFETY: the Core Graphics event is live until `placed` releases it.
    unsafe {
        let cg_event = CGEventCreateMouseEvent(
            std::ptr::null(),
            event.kind,
            NativePoint::default(),
            event.button,
        );
        if cg_event.is_null() {
            return Err("Core Graphics made no mouse event".to_owned());
        }
        CGEventSetIntegerValueField(cg_event, CLICK_STATE_FIELD, event.clicks as i64);
        CGEventSetFlags(cg_event, modifier_flags(event.modifiers));
        placed(cg_event, in_view(event.position, height))
    }
}

/// A wheel scroll in pixels; GPUI reads a positive `delta` as toward the
/// top, as the system reports it.
fn scroll_event(
    position: Point<Pixels>,
    delta: ScrollDelta,
    height: f64,
) -> Result<*mut Object, String> {
    let ScrollDelta::Pixels(pixels) = delta else {
        return Err("scrolls are in pixels".to_owned());
    };
    let dy = f32::from(pixels.y).round() as i32;
    // SAFETY: the Core Graphics event is live until `placed` releases it.
    unsafe {
        let cg_event =
            CGEventCreateScrollWheelEvent2(std::ptr::null(), SCROLL_IN_PIXELS, 1, dy, 0, 0);
        if cg_event.is_null() {
            return Err("Core Graphics made no scroll event".to_owned());
        }
        placed(cg_event, in_view(position, height))
    }
}

/// AppKit's event for `cg_event`, moved so it lands at `wanted` in the
/// view, then releases `cg_event`.
///
/// # Safety
///
/// `cg_event` is a live `CGEventRef` this call may release.
unsafe fn placed(cg_event: *mut c_void, wanted: NativePoint) -> Result<*mut Object, String> {
    // SAFETY: as the caller promises; AppKit's events retain what they're
    // made from, so releasing it after is safe.
    let native = unsafe {
        CGEventSetLocation(cg_event, NativePoint::default());
        let origin = ns_event_from(cg_event).map(location_in_window);
        let native = origin.and_then(|origin| {
            CGEventSetLocation(
                cg_event,
                NativePoint {
                    x: wanted.x - origin.x,
                    y: origin.y - wanted.y,
                },
            );
            ns_event_from(cg_event)
        });
        CFRelease(cg_event);
        native?
    };
    let landed = location_in_window(native);
    let off = (landed.x - wanted.x).abs().max((landed.y - wanted.y).abs());
    match off > PLACEMENT_TOLERANCE {
        true => Err(format!("the event landed at {landed:?}, not {wanted:?}")),
        false => Ok(native),
    }
}

/// A point in window points from the top-left, as AppKit counts it in a
/// view: from the bottom-left.
fn in_view(position: Point<Pixels>, height: f64) -> NativePoint {
    NativePoint {
        x: f64::from(f32::from(position.x)),
        y: height - f64::from(f32::from(position.y)),
    }
}

/// `CGEventFlags` for held modifiers.
fn modifier_flags(modifiers: gpui::Modifiers) -> u64 {
    const SHIFT: u64 = 1 << 17;
    const CONTROL: u64 = 1 << 18;
    const OPTION: u64 = 1 << 19;
    const COMMAND: u64 = 1 << 20;
    const FUNCTION: u64 = 1 << 23;
    [
        (modifiers.shift, SHIFT),
        (modifiers.control, CONTROL),
        (modifiers.alt, OPTION),
        (modifiers.platform, COMMAND),
        (modifiers.function, FUNCTION),
    ]
    .into_iter()
    .filter(|(held, _)| *held)
    .fold(0, |flags, (_, flag)| flags | flag)
}

/// AppKit's event for a Core Graphics one, autoreleased.
///
/// # Safety
///
/// `cg_event` is a live `CGEventRef`.
unsafe fn ns_event_from(cg_event: *mut c_void) -> Result<*mut Object, String> {
    let class = Class::get("NSEvent").ok_or("AppKit has no NSEvent")?;
    // SAFETY: `+eventWithCGEvent:` takes a CGEventRef and answers an
    // autoreleased event or nil.
    let event: *mut Object = unsafe {
        send(
            class as *const Class as *mut Object,
            "eventWithCGEvent:",
            (cg_event,),
        )
    };
    match event.is_null() {
        true => Err("AppKit made no event from Core Graphics'".to_owned()),
        false => Ok(event),
    }
}

fn location_in_window(event: *mut Object) -> NativePoint {
    // SAFETY: an NSEvent answers `-locationInWindow` with an NSPoint.
    unsafe { send(event, "locationInWindow", ()) }
}
