//! `NSWorkspace`'s `accessibilityDisplayShouldReduceMotion`.
//!
//! Messages go through `Message::send_message` rather than `msg_send!`,
//! whose expansion tests a `cargo-clippy` feature this crate doesn't have.

#![allow(unsafe_code)]

use objc::Message;
use objc::runtime::{BOOL, Class, NO, Object, Sel};

pub fn is_on() -> bool {
    let Some(class) = Class::get("NSWorkspace") else {
        return false;
    };
    // SAFETY: both messages match their AppKit declarations: a class
    // method answering the shared workspace, then a BOOL property on it,
    // which may be read from any thread.
    unsafe {
        let workspace: Result<*mut Object, _> =
            class.send_message(Sel::register("sharedWorkspace"), ());
        let Some(workspace) = workspace.ok().filter(|workspace| !workspace.is_null()) else {
            return false;
        };
        let reduce: Result<BOOL, _> =
            (*workspace).send_message(Sel::register("accessibilityDisplayShouldReduceMotion"), ());
        reduce.is_ok_and(|reduce| reduce != NO)
    }
}
