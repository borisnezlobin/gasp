//! The modal slot: one dialog at a time, centred near the top of the
//! window. It closes on the dialog's `DismissEvent`, on Escape or on a
//! click outside, and gives focus back to where it was.
//!
//! The slot only places the dialog. Each dialog draws its own surface
//! (`crate::ui::dialog`) at its own width, so the shadow always hugs it.

use std::any::TypeId;

use gpui::{
    AnyView, App, Context, DismissEvent, Entity, FocusHandle, Focusable, KeyDownEvent, ManagedView,
    MouseButton, Subscription, Window, div, prelude::*,
};

use crate::theme::UiTheme;

struct ActiveModal {
    view: AnyView,
    type_id: TypeId,
    focus: FocusHandle,
    previous_focus: Option<FocusHandle>,
    _dismiss: Subscription,
}

/// Holds the open modal, if any.
#[derive(Default)]
pub struct ModalLayer {
    active: Option<ActiveModal>,
}

impl ModalLayer {
    pub fn is_open(&self) -> bool {
        self.active.is_some()
    }

    /// The open modal, if it's a `V`.
    pub fn active<V: 'static>(&self) -> Option<Entity<V>> {
        self.active.as_ref()?.view.clone().downcast::<V>().ok()
    }

    /// Opens a `V`, or closes it when a `V` is already open. Another kind of
    /// modal is replaced.
    pub fn toggle<V, T>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<T>,
        build: impl FnOnce(&mut Window, &mut Context<V>) -> V,
    ) where
        V: ManagedView,
        T: ModalHost,
    {
        let was_same = self
            .active
            .as_ref()
            .is_some_and(|modal| modal.type_id == TypeId::of::<V>());
        let previous_focus = self.close(window).or_else(|| window.focused(cx));
        if was_same {
            return;
        }
        let view = cx.new(|cx| build(window, cx));
        let dismiss = cx.subscribe_in(&view, window, |host, _, _: &DismissEvent, window, cx| {
            host.close_modal(window, cx);
        });
        let focus = view.focus_handle(cx);
        window.focus(&focus);
        // A screen opens without a ring until the keyboard moves in it.
        crate::ui::focus_visible::set_keyboard_driving(false, cx);
        self.active = Some(ActiveModal {
            view: view.into(),
            type_id: TypeId::of::<V>(),
            focus,
            previous_focus,
            _dismiss: dismiss,
        });
        cx.notify();
    }

    /// Closes the modal and gives focus back. Returns the focus it was
    /// going to restore, for a modal that replaces it.
    pub fn close(&mut self, window: &mut Window) -> Option<FocusHandle> {
        let modal = self.active.take()?;
        if let Some(previous) = &modal.previous_focus {
            window.focus(previous);
        }
        modal.previous_focus
    }

    /// Every modal sizes itself, so this does nothing. It stays for the
    /// callers that ask for it.
    pub fn set_self_sized(&mut self) {}

    /// Whether the modal holds keyboard focus.
    pub fn has_focus(&self, window: &Window, cx: &App) -> bool {
        self.active
            .as_ref()
            .is_some_and(|modal| modal.focus.contains_focused(window, cx))
    }

    /// Draws the modal over everything, with a backdrop that closes it.
    pub fn render<T: ModalHost>(
        &self,
        theme: &UiTheme,
        cx: &mut Context<T>,
    ) -> Option<impl IntoElement> {
        let modal = self.active.as_ref()?;
        Some(
            div()
                .id("modal-layer")
                .absolute()
                .inset_0()
                .flex()
                .flex_col()
                .items_center()
                .px(theme.surface_gap)
                .pt(theme.dialog_top_offset)
                .bg(theme.backdrop)
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|host, _, window, cx| host.close_modal(window, cx)),
                )
                .on_key_down(cx.listener(|host, event: &KeyDownEvent, window, cx| {
                    if event.keystroke.key == "escape" {
                        host.close_modal(window, cx);
                        cx.stop_propagation();
                    }
                }))
                .child(
                    div()
                        .id("modal")
                        .flex()
                        .justify_center()
                        .max_w_full()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(modal.view.clone()),
                ),
        )
    }
}

/// A view that owns a [`ModalLayer`].
pub trait ModalHost: 'static + Sized {
    fn close_modal(&mut self, window: &mut Window, cx: &mut Context<Self>);
}
