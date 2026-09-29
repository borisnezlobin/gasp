//! Drawing the Snippets page's rows. Each reads "type this, get this":
//! the keys in a chip, an arrow, and what they turn into as it will look,
//! with math rendered. Small marks at the right say where and when it
//! fires, with the words in their tooltips. A snippet row opens its
//! editor when clicked; the switch turns it on or off.
//!
//! The first row shows the idea with one worked example beside the button
//! that adds a snippet.

use gpui::{
    AnyElement, ClickEvent, Context, Div, MouseButton, MouseDownEvent, SharedString, Stateful,
    Window, div, img, prelude::*,
};

use super::controls::{control_note, icon_label_button, toggle_switch, two_column_row};
use super::model::PageSpec;
use super::snippet_look::{KeyPiece, Place, ResultPiece, SnippetLook, TriggerLook};
use super::snippets_page::{ReplacementRow, SnippetRow};
use super::view::{ControlRow, SettingsFocus, SettingsView};
use crate::icons::{IconName, icon};
use crate::preview::math::{MathKey, MathState};
use crate::ui::Selectable;
use crate::ui::Tooltip;
use crate::ui::keycap::{Glyph, keycap_glyphs};

/// The worked example above the list: the keys, and the LaTeX they give.
const EXAMPLE_KEYS: [&str; 2] = ["@", "a"];
const EXAMPLE_TEX: &str = "\\alpha";
/// What the example gives, shown while its math renders.
const EXAMPLE_TEXT: &str = "α";

/// A math result as drawn.
pub(super) enum Drawn {
    Image(AnyElement),
    Pending,
    /// It failed, or came out empty.
    Unrenderable,
}

/// The icon for where a snippet works.
fn place_icon(place: Place) -> IconName {
    match place {
        Place::Anywhere => IconName::GlobeSimple,
        Place::Text => IconName::TextT,
        Place::Math => IconName::Sigma,
        Place::Code => IconName::Code,
    }
}

impl SettingsView {
    /// A row of the Snippets page's lists, which draws its own control.
    pub(super) fn render_typing_row(
        &mut self,
        index: usize,
        row: &ControlRow,
        focused: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // The columns are named as every row's are, for the layout tests.
        let page = self
            .current_section()
            .map_or("", |page| PageSpec::get(page).id);
        let columns = format!("{page}-{index}");
        let row_element = match row {
            ControlRow::SnippetsFile => self.render_snippets_intro(&columns, focused, window, cx),
            ControlRow::Snippet(snippet) => {
                self.render_snippet_row(&columns, snippet, focused, window, cx)
            }
            ControlRow::Replacement(replacement) => {
                self.render_replacement_row(&columns, replacement, focused, window, cx)
            }
            _ => div(),
        };
        let note = self
            .row_error(row)
            .map(|message| control_note(message, &self.style));
        row_element
            .relative()
            .children(note)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _: &MouseDownEvent, window, cx| {
                    if view.focus != SettingsFocus::Control(index) {
                        view.set_focus(SettingsFocus::Control(index), window, cx);
                    }
                }),
            )
            .into_any_element()
    }

    /// The example that shows what a snippet is, the file they're kept
    /// in, and the button that adds one.
    fn render_snippets_intro(
        &mut self,
        columns: &str,
        focused: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let style = self.style.clone();
        let keys: Vec<Stateful<Div>> = EXAMPLE_KEYS
            .iter()
            .map(|key| {
                self.key_chip(SharedString::from(format!("example-key-{key}")))
                    .child(*key)
            })
            .collect();
        let result = match self.math_image(EXAMPLE_TEX, style.example_math_size, window, cx) {
            Drawn::Image(image) => image,
            _ => div()
                .text_size(style.example_math_size)
                .child(EXAMPLE_TEXT)
                .into_any_element(),
        };
        let example = div()
            .selector(|| "snippets-example".to_string())
            .flex()
            .items_center()
            .gap(style.control_gap)
            .child(div().flex().gap(style.gap_sm).children(keys))
            .child(arrow(&style))
            .child(result);
        let text = div()
            .flex()
            .flex_col()
            .gap(style.control_gap)
            .child(example)
            .child(
                div()
                    .text_size(style.small_text_size)
                    .text_color(style.text_muted)
                    .child(self.snippets_file_description()),
            );
        let add = icon_label_button(
            "add-snippet",
            IconName::Plus,
            "New snippet",
            true,
            focused,
            &style,
        )
        .selector(|| "add-snippet".to_string())
        .on_click(cx.listener(|view, _: &ClickEvent, window, cx| {
            view.open_snippet_editor(None, window, cx)
        }));
        two_column_row(columns, text, Some(add.into_any_element()), &style)
    }

    /// A snippet: the whole row opens its editor, and its switch turns it
    /// on or off.
    fn render_snippet_row(
        &mut self,
        columns: &str,
        row: &SnippetRow,
        focused: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let style = self.style.clone();
        let line = row.line;
        let name = format!("snippet-{line}");
        let text = self.type_and_get(&row.look, &name, window, cx);
        let switch = toggle_switch(
            SharedString::from(format!("toggle-snippet-{line}")),
            row.on,
            false,
            &style,
        )
        .selector(move || format!("toggle-snippet-{line}"))
        .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
            cx.stop_propagation();
            view.toggle_snippet(line, cx)
        }));
        let control = self.typing_controls(&row.look, row.on, &name, switch);
        let hover = style.card_hover;
        let plate = div()
            .id(SharedString::from(format!("snippet-row-{line}")))
            .selector(move || format!("snippet-row-{line}"))
            .mx(-(style.card_padding_x - style.plate_inset))
            .my(-(style.list_row_padding_y - style.plate_inset))
            .px(style.card_padding_x - style.plate_inset)
            .py(style.list_row_padding_y - style.plate_inset)
            .rounded(style.radius)
            // The focus ring is a shadow, which shows through a clear fill.
            .bg(style.card_background)
            .cursor_pointer()
            .hover(move |plate| plate.bg(hover))
            .when(focused, |plate| plate.shadow(vec![style.focus()]))
            .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                view.open_snippet_editor(Some(line), window, cx)
            }))
            .child(two_column_row(
                columns,
                dimmed(text, row.on, &style),
                Some(control),
                &style,
            ));
        div().child(plate)
    }

    /// A replacement: the same "type, get" row, with its switch.
    fn render_replacement_row(
        &mut self,
        columns: &str,
        row: &ReplacementRow,
        focused: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let style = self.style.clone();
        let index = row.index;
        let name = format!("replacement-{index}");
        let text = self.type_and_get(&row.look, &name, window, cx);
        let switch = toggle_switch(
            SharedString::from(format!("toggle-replacement-{index}")),
            row.on,
            focused,
            &style,
        )
        .selector(move || format!("toggle-replacement-{index}"))
        .on_click(
            cx.listener(move |view, _: &ClickEvent, _, cx| view.toggle_replacement(index, cx)),
        );
        let control = self.typing_controls(&row.look, row.on, &name, switch);
        two_column_row(columns, dimmed(text, row.on, &style), Some(control), &style)
    }

    /// The marks for where and when, then the switch.
    fn typing_controls(
        &self,
        look: &SnippetLook,
        on: bool,
        name: &str,
        switch: Stateful<Div>,
    ) -> AnyElement {
        let style = &self.style;
        let tooltips = look.tooltips();
        let mut marks: Vec<AnyElement> = Vec::new();
        let mut tips = tooltips.into_iter();
        if look.on_tab {
            let tab = keycap_glyphs(
                vec![Glyph::Text(SharedString::new_static("Tab"))],
                &self.keycaps.clone().compact(),
            );
            marks.push(mark(format!("{name}-tab"), tab, tips.next(), style));
        }
        for place in &look.places {
            let glyph = icon(place_icon(*place))
                .size(style.small_icon_size)
                .text_color(style.text_muted);
            let id = format!("{name}-place-{place:?}");
            marks.push(mark(id, div().child(glyph), tips.next(), style));
        }
        if look.on_selection {
            let glyph = icon(IconName::Selection)
                .size(style.small_icon_size)
                .text_color(style.text_muted);
            let id = format!("{name}-selection");
            marks.push(mark(id, div().child(glyph), tips.next(), style));
        }
        div()
            .flex()
            .items_center()
            .gap(style.row_gap)
            .child(dimmed(
                div()
                    .flex()
                    .items_center()
                    .gap(style.gap_sm)
                    .children(marks),
                on,
                style,
            ))
            .child(switch)
            .into_any_element()
    }

    /// The keys, an arrow, and what they give.
    fn type_and_get(
        &mut self,
        look: &SnippetLook,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let style = self.style.clone();
        let result = self.result_element(look, name, window, cx);
        div()
            .flex()
            .items_center()
            .gap(style.control_gap)
            .child(
                div()
                    .flex_none()
                    .w(style.trigger_column_width)
                    .flex()
                    .child(self.trigger_chip(look, name)),
            )
            .child(arrow(&style))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .flex()
                    .items_center()
                    .child(result),
            )
    }

    /// What's typed, as one key chip. Parts that stand for any letter or
    /// digit are an example in the accent, and say so in a tooltip.
    fn trigger_chip(&self, look: &SnippetLook, name: &str) -> AnyElement {
        let style = &self.style;
        let selector = format!("{name}-keys");
        let chip = self
            .key_chip(SharedString::from(selector.clone()))
            .selector(move || selector.clone());
        let chip = match &look.trigger {
            TriggerLook::Keys(pieces) => {
                chip.children(pieces.iter().map(|piece| key_piece(piece, style)))
            }
            TriggerLook::Pattern(source) => chip
                .font_family(style.code_font_family.clone())
                .child(div().truncate().child(source.clone()))
                .tooltip(Tooltip::new(format!("Any text matching {source}"), None).builder()),
        };
        match &look.trigger_note {
            Some(note) => chip
                .tooltip(Tooltip::new(note.clone(), None).builder())
                .into_any_element(),
            None => chip.into_any_element(),
        }
    }

    /// A key chip, as the shortcuts are drawn, with its text at full
    /// strength: what's typed is the point of the row.
    fn key_chip(&self, id: SharedString) -> Stateful<Div> {
        let keys = &self.keycaps;
        div()
            .id(id)
            .flex_none()
            .max_w_full()
            .overflow_hidden()
            .flex()
            .items_center()
            .h(keys.height)
            .min_w(keys.height)
            .justify_center()
            .px(keys.padding_x)
            .rounded(keys.radius)
            .bg(keys.fill)
            .font_family(keys.font_family.clone())
            .text_size(self.style.small_text_size)
            .font_weight(keys.font_weight)
            .line_height(keys.height)
            .text_color(self.style.text)
            .whitespace_nowrap()
    }

    /// What a snippet gives: its math rendered, or else its pieces.
    fn result_element(
        &mut self,
        look: &SnippetLook,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let size = self.style.snippet_math_size;
        let drawn = look
            .math
            .as_ref()
            .map(|tex| self.math_image(tex, size, window, cx));
        match drawn {
            Some(Drawn::Image(image)) => {
                let name = name.to_string();
                div()
                    .selector(move || format!("{name}-math"))
                    .child(image)
                    .into_any_element()
            }
            // Rendering takes a moment; the row keeps its height meanwhile.
            Some(Drawn::Pending) => div().h(self.keycaps.height).into_any_element(),
            Some(Drawn::Unrenderable) | None => result_pieces(look, name, &self.style),
        }
    }

    /// The rendered equation at `size`, starting its render if it hasn't
    /// been.
    pub(super) fn math_image(
        &mut self,
        tex: &str,
        size: gpui::Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Drawn {
        let key = MathKey::new(tex, false, size, window.scale_factor(), self.style.text);
        let state = self.math.lookup(key);
        self.start_math_renders(cx);
        match state {
            MathState::Pending => Drawn::Pending,
            // An equation that came out empty, such as an environment
            // with nothing in it, shows its source instead.
            MathState::Ready(image) if image.width > gpui::px(0.) => Drawn::Image(
                img(image.image.clone())
                    .flex_none()
                    .w(image.width)
                    .h(image.height)
                    .into_any_element(),
            ),
            _ => Drawn::Unrenderable,
        }
    }

    fn start_math_renders(&mut self, cx: &mut Context<Self>) {
        for request in self.math.take_requests() {
            cx.spawn(async move |this, cx| {
                let key = request.key.clone();
                let state = cx
                    .background_executor()
                    .spawn(async move { request.run() })
                    .await;
                this.update(cx, |view, cx| {
                    view.math.finish(key, state);
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
    }

    /// Whether a row's math has been asked for, and whether it's drawn.
    pub fn snippet_math(&self, tex: &str) -> Option<bool> {
        self.math
            .find(tex)
            .map(|state| matches!(state, MathState::Ready(_)))
    }
}

fn arrow(style: &crate::theme::SettingsTheme) -> impl IntoElement {
    icon(IconName::ArrowRight)
        .flex_none()
        .size(style.small_icon_size)
        .text_color(style.text_faint)
}

/// A switched-off row's content, faded so it reads as off at a glance.
fn dimmed(element: Div, on: bool, style: &crate::theme::SettingsTheme) -> Div {
    element.when(!on, |element| element.opacity(style.inactive_opacity))
}

/// A small mark on the right of a row, with its words in a tooltip.
fn mark(
    id: String,
    content: Div,
    tooltip: Option<String>,
    style: &crate::theme::SettingsTheme,
) -> AnyElement {
    let hover = style.card_hover;
    let element = content
        .id(SharedString::from(id.clone()))
        .selector(move || id.clone())
        .flex_none()
        .min_w(style.indicator_size)
        .h(style.indicator_size)
        .flex()
        .items_center()
        .justify_center()
        .rounded(style.radius)
        .hover(move |mark| mark.bg(hover));
    match tooltip {
        Some(words) => element
            .tooltip(Tooltip::new(words, None).builder())
            .into_any_element(),
        None => element.into_any_element(),
    }
}

fn key_piece(piece: &KeyPiece, style: &crate::theme::SettingsTheme) -> AnyElement {
    match piece {
        KeyPiece::Text(text) => div().child(text.clone()).into_any_element(),
        KeyPiece::Example(text) => div()
            .text_color(style.accent)
            .child(text.clone())
            .into_any_element(),
        KeyPiece::Space => div()
            .text_color(style.text_faint)
            .child("␣")
            .into_any_element(),
    }
}

/// A result that isn't rendered math: its text, line by line, with each
/// place the cursor stops drawn as a mark in the accent.
fn result_pieces(
    look: &SnippetLook,
    name: &str,
    style: &crate::theme::SettingsTheme,
) -> AnyElement {
    let mut lines: Vec<Vec<&ResultPiece>> = vec![Vec::new()];
    for piece in &look.result {
        match piece {
            ResultPiece::Break => lines.push(Vec::new()),
            piece => lines.last_mut().expect("there's a line").push(piece),
        }
    }
    let (font, size) = if look.code {
        (style.code_font_family.clone(), style.small_text_size)
    } else {
        (style.font_family.clone(), style.text_size)
    };
    let name = name.to_string();
    div()
        .selector(move || format!("{name}-result"))
        .flex()
        .flex_col()
        .font_family(font)
        .text_size(size)
        .children(lines.into_iter().map(|line| {
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .min_h(style.slot_height)
                .children(line.into_iter().map(|piece| result_piece(piece, style)))
        }))
        .into_any_element()
}

fn result_piece(piece: &ResultPiece, style: &crate::theme::SettingsTheme) -> AnyElement {
    match piece {
        ResultPiece::Text(text) => div()
            .whitespace_nowrap()
            .child(text.clone())
            .into_any_element(),
        ResultPiece::Example(text) => div()
            .whitespace_nowrap()
            .text_color(style.accent)
            .child(text.clone())
            .into_any_element(),
        ResultPiece::Slot | ResultPiece::Break => div()
            .selector(|| "snippet-slot".to_string())
            .flex_none()
            .w(style.slot_width)
            .h(style.slot_height)
            .mx(style.gap_xs)
            .rounded(style.slot_radius)
            .border(style.slot_border)
            .border_color(style.accent)
            .bg(style.slot_fill)
            .into_any_element(),
    }
}
