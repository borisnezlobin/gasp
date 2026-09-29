//! Reading back what GPUI drew into a window that's never shown.
//!
//! GPUI draws each frame into a drawable from the view's `CAMetalLayer`
//! and presents it. The layer is switched to a subclass whose
//! `nextDrawable` keeps the last drawable it handed out, and its textures
//! are made readable, so once the frames settle the last one is copied
//! into memory. Frames are asked for by sending the view `displayLayer:`,
//! the message AppKit sends when a layer needs drawing, so no display
//! link and no visible window are needed. The app never takes a Dock icon
//! or focus: its activation policy is held at "prohibited".
//!
//! Messages go through `Message::send_message` rather than `msg_send!`,
//! whose expansion tests a `cargo-clippy` feature this crate doesn't have.

#![allow(unsafe_code)]

use std::any::Any;
use std::cell::{Cell, RefCell};

use image::RgbaImage;
use metal::foreign_types::ForeignTypeRef;
use metal::{MTLBlitOption, MTLOrigin, MTLResourceOptions, MTLSize, MetalDrawable};
use objc::runtime::{Class, Imp, NO, Object, Sel, class_addMethod, object_getClass};
use objc::{Message, MessageArguments};

/// GPUI's application class, registered when the binary loads.
const APP_CLASS: &str = "GPUIApplication";
/// The layer class the snapshot switches GPUI's layer to.
const CAPTURING_LAYER_CLASS: &str = "GaspSnapshotLayer";
/// `NSApplicationActivationPolicyProhibited`: no Dock icon, no menu bar,
/// never the active app.
const ACTIVATION_POLICY_PROHIBITED: isize = 2;
const BYTES_PER_PIXEL: u64 = 4;

unsafe extern "C" {
    fn object_setClass(object: *mut Object, class: *const Class) -> *const Class;
    fn objc_allocateClassPair(
        superclass: *const Class,
        name: *const std::ffi::c_char,
        extra_bytes: usize,
    ) -> *mut Class;
    fn objc_registerClassPair(class: *mut Class);
}

thread_local! {
    static LAST_DRAWABLE: RefCell<Option<MetalDrawable>> = const { RefCell::new(None) };
    static FRAMES_DRAWN: Cell<u64> = const { Cell::new(0) };
}

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

/// The view a window draws into, set up to keep what it draws.
pub struct LayerCapture {
    view: *mut Object,
    layer: *mut Object,
}

impl LayerCapture {
    /// Hooks the layer of the NSView at `view`, which must be GPUI's
    /// view of a window that's open. Call on the main thread before the
    /// window's first frame.
    pub fn attach(view: usize) -> Result<Self, String> {
        let view = view as *mut Object;
        // SAFETY: `view` is a live NSView; `layer` answers its backing
        // layer, which GPUI makes a CAMetalLayer.
        let layer: *mut Object = unsafe { send(view, "layer", ()) };
        if layer.is_null() {
            return Err("the window's view has no layer".to_owned());
        }
        let class = capturing_layer_class(layer)?;
        // SAFETY: the subclass adds one method to the layer's own class
        // and no storage, so the layer is a valid instance of it.
        unsafe {
            let _: () = send(layer, "setFramebufferOnly:", (false,));
            object_setClass(layer, class);
        }
        Ok(Self { view, layer })
    }

    /// Asks GPUI for a frame now. It draws only when something changed,
    /// which [`LayerCapture::frames_drawn`] shows.
    pub fn request_frame(&self) {
        // SAFETY: GPUI's view answers `displayLayer:` by running its
        // frame callback, on this (the main) thread, outside any borrow
        // of the app.
        unsafe {
            let _: () = send(self.view, "displayLayer:", (self.layer,));
        }
    }

    /// How many frames GPUI has drawn so far.
    pub fn frames_drawn(&self) -> u64 {
        FRAMES_DRAWN.with(Cell::get)
    }

    /// The last frame drawn, as RGBA pixels at the window's backing
    /// scale.
    pub fn last_frame(&self) -> Result<RgbaImage, String> {
        let drawable = LAST_DRAWABLE
            .with(|last| last.borrow().clone())
            .ok_or("nothing was drawn")?;
        read_texture(drawable.texture())
    }
}

/// The subclass of `layer`'s class that keeps its drawables, made once.
fn capturing_layer_class(layer: *mut Object) -> Result<*const Class, String> {
    if let Some(class) = Class::get(CAPTURING_LAYER_CLASS) {
        return Ok(class);
    }
    // SAFETY: `layer` is a live object; the new class is its class plus
    // one method with the signature its encoding (`@@:`) gives, and it's
    // registered before any instance takes it.
    unsafe {
        let superclass = object_getClass(layer);
        let name = std::ffi::CString::new(CAPTURING_LAYER_CLASS).unwrap_or_default();
        let class = objc_allocateClassPair(superclass, name.as_ptr(), 0);
        if class.is_null() {
            return Err("couldn't subclass the window's layer".to_owned());
        }
        let method: extern "C" fn(&Object, Sel) -> *mut Object = next_drawable;
        let imp = std::mem::transmute::<extern "C" fn(&Object, Sel) -> *mut Object, Imp>(method);
        if class_addMethod(class, Sel::register("nextDrawable"), imp, c"@@:".as_ptr()) == NO {
            return Err("couldn't hook the window's layer".to_owned());
        }
        objc_registerClassPair(class);
        Ok(class)
    }
}

extern "C" fn next_drawable(layer: &Object, _: Sel) -> *mut Object {
    // SAFETY: the layer's class is our subclass, so its superclass is the
    // CAMetalLayer class that answers `nextDrawable` with a drawable or nil.
    let drawable: *mut Object = unsafe {
        let class = &*object_getClass(layer);
        let Some(superclass) = class.superclass() else {
            return std::ptr::null_mut();
        };
        objc::__send_super_message(layer, superclass, Sel::register("nextDrawable"), ())
            .unwrap_or(std::ptr::null_mut())
    };
    if drawable.is_null() {
        return drawable;
    }
    // SAFETY: a non-nil answer is a CAMetalDrawable; the copy kept here
    // retains it, and the one returned stays GPUI's.
    let kept = unsafe { metal::MetalDrawableRef::from_ptr(drawable.cast()) }.to_owned();
    LAST_DRAWABLE.with(|last| *last.borrow_mut() = Some(kept));
    FRAMES_DRAWN.with(|frames| frames.set(frames.get() + 1));
    drawable
}

/// Copies a BGRA texture into memory, as RGBA.
fn read_texture(texture: &metal::TextureRef) -> Result<RgbaImage, String> {
    let (width, height) = (texture.width(), texture.height());
    let row_bytes = width * BYTES_PER_PIXEL;
    let device = texture.device();
    let buffer = device.new_buffer(row_bytes * height, MTLResourceOptions::StorageModeShared);
    let queue = device.new_command_queue();
    let commands = queue.new_command_buffer();
    let blit = commands.new_blit_command_encoder();
    blit.copy_from_texture_to_buffer(
        texture,
        0,
        0,
        MTLOrigin { x: 0, y: 0, z: 0 },
        MTLSize {
            width,
            height,
            depth: 1,
        },
        &buffer,
        0,
        row_bytes,
        row_bytes * height,
        MTLBlitOption::empty(),
    );
    blit.end_encoding();
    commands.commit();
    commands.wait_until_completed();
    let length = usize::try_from(row_bytes * height).map_err(|error| error.to_string())?;
    // SAFETY: the buffer is shared memory `length` bytes long, and the
    // blit that filled it has completed.
    let bgra = unsafe { std::slice::from_raw_parts(buffer.contents().cast::<u8>(), length) };
    let rgba = bgra
        .chunks_exact(4)
        .flat_map(|pixel| [pixel[2], pixel[1], pixel[0], pixel[3]])
        .collect();
    let size = |pixels: u64| u32::try_from(pixels).map_err(|error| error.to_string());
    RgbaImage::from_raw(size(width)?, size(height)?, rgba)
        .ok_or_else(|| "the frame's size didn't match its pixels".to_owned())
}

/// Sends `selector` to `receiver` with `args`, answering what it returns.
///
/// # Safety
///
/// `receiver` is a live object, and `args` and `R` match the method's
/// declaration.
unsafe fn send<A: MessageArguments, R: Any>(receiver: *mut Object, selector: &str, args: A) -> R {
    // SAFETY: as the caller promises.
    let sent = unsafe { (*receiver).send_message(Sel::register(selector), args) };
    sent.unwrap_or_else(|error| panic!("{selector} failed: {error:?}"))
}
