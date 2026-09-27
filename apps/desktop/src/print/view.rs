//! How the print dialog looks: the pages in a scrolling well on the left,
//! the settings on the right, and the page indicator and buttons along
//! the bottom.

use gpui::{
    AnyElement, Context, Div, Hsla, ImageSource, IntoElement, ObjectFit, Pixels, Render,
    SharedString, Window, div, img, prelude::*, px,
};

use super::preview::{self, LayoutFailure, Preview};
use super::settings::Control;
use super::{
    Cancel, ChangeNext, ChangePrevious, NextSetting, PRINT_CONTEXT, Pending, PreviousSetting,
    Print, PrintDialog, SaveAsPdf, Status, ToggleSetting, saved_message,
};
use crate::icons::{IconName, icon};
use crate::settings_view::controls::{dropdown_button, toggle_switch};
use crate::ui::button::Button;

impl PrintDialog {
    /// Keys go to an open menu rather than the dialog.
    fn keys_to_menu(&self, cx: &mut Context<Self>) -> bool {
        if self.menu.is_open() {
            cx.propagate();
        }
        self.menu.is_open()
    }

    fn preview_height(&self, window: &Window) -> Pixels {
        let print = &self.theme.print;
        let room = window.viewport_size().height - print.window_allowance;
        px(preview::preview_height(
            f32::from(room),
            f32::from(print.preview_min_height),
            f32::from(print.preview_max_height),
        ))
    }

    fn render_page(&self, index: usize, preview: &Preview) -> AnyElement {
        let print = &self.theme.print;
        let page = &preview.pages[index];
        div()
            .id(("print-page", index))
            .debug_selector(move || format!("print-page-{}", index + 1))
            .flex_none()
            .w(print.page_width)
            .h(print.page_width * page.aspect)
            .bg(print.paper)
            .shadow(print.page_shadows())
            .child(
                img(ImageSource::Render(page.image.clone()))
                    .size_full()
                    .object_fit(ObjectFit::Fill),
            )
            .into_any_element()
    }

    /// A blank page the shape of the paper, until the first is drawn.
    fn render_placeholder(&self) -> AnyElement {
        let print = &self.theme.print;
        div()
            .debug_selector(|| "print-placeholder".to_owned())
            .flex_none()
            .w(print.page_width)
            .h(print.page_width * self.settings.paper.aspect())
            .bg(print.placeholder)
            .shadow(print.page_shadows())
            .into_any_element()
    }

    /// Why the note couldn't be laid out, where the pages would be.
    fn render_failure(&self, failure: &LayoutFailure) -> AnyElement {
        let ui = &self.theme;
        let messages = failure.messages.iter().map(|message| {
            div()
                .text_size(ui.small_font_size)
                .text_color(ui.text_detail)
                .child(SharedString::from(message.clone()))
        });
        div()
            .debug_selector(|| "print-failure".to_owned())
            .flex()
            .flex_col()
            .items_center()
            .gap(ui.space_md)
            .max_w(ui.print.message_width)
            .pt(ui.space_xl * 2.)
            .child(
                icon(IconName::Warning)
                    .size(ui.icon_size)
                    .text_color(ui.error),
            )
            .child(
                div()
                    .text_color(ui.text)
                    .child("This note couldn’t be laid out"),
            )
            .children(messages)
            .into_any_element()
    }

    fn render_preview(&self, height: Pixels, cx: &mut Context<Self>) -> impl IntoElement {
        let print = &self.theme.print;
        let content: Vec<AnyElement> = match (&self.failure, &self.preview) {
            (Some(failure), _) => vec![self.render_failure(failure)],
            (None, Some(preview)) => (0..preview.pages.len())
                .map(|index| self.render_page(index, preview))
                .collect(),
            (None, None) => vec![self.render_placeholder()],
        };
        div()
            .id("print-preview")
            .flex_none()
            .w(print.well_width())
            .h(height)
            .rounded(print.well_radius)
            .bg(print.well)
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .on_scroll_wheel(cx.listener(|_, _, _, cx| cx.notify()))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(print.page_gap)
                    .p(print.well_padding)
                    .children(content),
            )
    }

    fn render_control(&self, index: usize, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let control = Control::ALL[index];
        let focused = index == self.focused && self.focus_handle.is_focused(window);
        let ring = crate::ui::focus_visible::ring(focused, cx);
        let style = &self.style;
        if control.is_switch() {
            return toggle_switch(
                ("print-switch", index),
                self.settings.is_on(control),
                ring,
                style,
            )
            .debug_selector(move || format!("print-{}", control.key()))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.focused = index;
                this.change(control, true, window, cx);
            }))
            .into_any_element();
        }
        let button = dropdown_button(
            ("print-choice", index),
            self.settings.choice_label(control),
            ring || self.menu.is_open_at(control.key()),
            style,
        )
        .debug_selector(move || format!("print-{}", control.key()))
        .on_click(cx.listener(move |this, _, window, cx| this.open_menu(control, window, cx)));
        div()
            .relative()
            .child(button)
            .children(self.menu.render_attached(control.key(), style.menu_offset))
            .into_any_element()
    }

    fn render_settings(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = &self.theme;
        let print = &ui.print;
        let rows: Vec<AnyElement> = (0..Control::ALL.len())
            .map(|index| {
                div()
                    .h(print.setting_row_height)
                    .flex()
                    .items_center()
                    .gap(ui.space_md)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(crate::ui::truncated(Control::ALL[index].label())),
                    )
                    .child(self.render_control(index, window, cx))
                    .into_any_element()
            })
            .collect();
        let title = format!(
            "Print {}",
            self.file_name().trim_end_matches(".pdf").to_owned()
        );
        div()
            .flex()
            .flex_col()
            .flex_none()
            .w(print.settings_width)
            .px(print.settings_padding)
            .child(
                div()
                    .pt(ui.space_md)
                    .pb(ui.space_md)
                    .text_size(ui.font_size + px(2.))
                    .child(crate::ui::truncated(title)),
            )
            .children(rows)
    }

    /// The line beside the buttons and its colour: what just happened,
    /// what's waited for, or which page is in view.
    fn footer_text(&self, height: Pixels) -> (String, Hsla) {
        let ui = &self.theme;
        let detail = ui.text_detail;
        match (&self.status, self.pending) {
            (Some(Status::Saved(path)), _) => return (saved_message(path), detail),
            (Some(Status::Failed(message)), _) => return (message.clone(), ui.error),
            (None, Some(Pending::Print)) => {
                return ("Prints once the pages are ready.".to_owned(), detail);
            }
            (None, Some(Pending::Save)) => {
                return ("Saves once the pages are ready.".to_owned(), detail);
            }
            (None, None) => {}
        }
        match (&self.preview, self.laying_out, &self.failure) {
            (_, _, Some(failure)) if !self.laying_out => (failure.summary(), ui.error),
            (Some(_), true, _) => ("Updating the preview…".to_owned(), detail),
            (None, true, _) => ("Laying out pages…".to_owned(), detail),
            (Some(preview), false, _) => (self.page_indicator(preview, height), detail),
            _ => (String::new(), detail),
        }
    }

    fn page_indicator(&self, preview: &Preview, height: Pixels) -> String {
        let print = &self.theme.print;
        let aspect = preview.pages.first().map_or(1., |page| page.aspect);
        let stride = print.page_width * aspect + print.page_gap;
        let scrolled = -self.scroll.offset().y - print.well_padding;
        let current = preview::current_page(
            f32::from(scrolled),
            f32::from(height),
            f32::from(stride),
            preview.pages.len(),
        );
        preview::page_indicator(current, preview.pages.len())
    }

    fn render_footer(&self, height: Pixels, cx: &mut Context<Self>) -> Div {
        let ui = &self.theme;
        let (text, color) = self.footer_text(height);
        let nothing_to_print = self.preview.is_none() && !self.laying_out;
        let saved = matches!(self.status, Some(Status::Saved(_)));
        div()
            .flex()
            .items_center()
            .gap(ui.space_md)
            .px(ui.row_padding_x)
            .pt(ui.space_md)
            .pb(ui.space_sm)
            .child(
                div()
                    .debug_selector(|| "print-footer".to_owned())
                    .flex_1()
                    .min_w_0()
                    .text_size(ui.small_font_size)
                    .text_color(color)
                    .child(crate::ui::truncated(text)),
            )
            .child(
                Button::new("print-cancel", if saved { "Close" } else { "Cancel" })
                    .on_click(cx.listener(|this, _, _, cx| this.cancel(cx))),
            )
            .child(
                Button::new("print-save", "Save as PDF…")
                    .disabled(nothing_to_print)
                    .on_click(cx.listener(|this, _, _, cx| this.save_as_pdf(cx))),
            )
            .child(
                Button::new("print-print", "Print…")
                    .primary()
                    .disabled(nothing_to_print)
                    .on_click(cx.listener(|this, _, window, cx| this.print(window, cx))),
            )
    }
}

impl Render for PrintDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The theme can change while the dialog is open.
        self.theme = crate::ui::ui_theme(cx);
        self.style = crate::ui::settings_theme(cx);
        self.scale_factor = window.scale_factor();
        let ui = self.theme.clone();
        let height = self.preview_height(window);
        crate::ui::dialog(&ui)
            .key_context(PRINT_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &NextSetting, _, cx| {
                if !this.keys_to_menu(cx) {
                    this.move_focus(true, cx)
                }
            }))
            .on_action(cx.listener(|this, _: &PreviousSetting, _, cx| {
                if !this.keys_to_menu(cx) {
                    this.move_focus(false, cx)
                }
            }))
            .on_action(cx.listener(|this, _: &ChangeNext, window, cx| {
                if !this.keys_to_menu(cx) {
                    this.change(this.focused_control(), true, window, cx)
                }
            }))
            .on_action(cx.listener(|this, _: &ChangePrevious, window, cx| {
                if !this.keys_to_menu(cx) {
                    this.change(this.focused_control(), false, window, cx)
                }
            }))
            .on_action(cx.listener(|this, _: &ToggleSetting, window, cx| {
                if !this.keys_to_menu(cx) {
                    this.toggle_focused(window, cx)
                }
            }))
            .on_action(cx.listener(|this, _: &Print, window, cx| {
                if !this.keys_to_menu(cx) {
                    this.print(window, cx)
                }
            }))
            .on_action(cx.listener(|this, _: &SaveAsPdf, _, cx| {
                if !this.keys_to_menu(cx) {
                    this.save_as_pdf(cx)
                }
            }))
            .on_action(cx.listener(|this, _: &Cancel, _, cx| {
                if !this.keys_to_menu(cx) {
                    this.cancel(cx)
                }
            }))
            .p(ui.dialog_padding)
            .child(
                div()
                    .flex()
                    .child(self.render_preview(height, cx))
                    .child(self.render_settings(window, cx)),
            )
            .child(self.render_footer(height, cx))
            .children(self.menu.render_overlay(window, cx))
    }
}
