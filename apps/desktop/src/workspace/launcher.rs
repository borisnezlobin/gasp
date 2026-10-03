//! What an empty tab shows: a field to find a note and a button for a new
//! one, over the notes opened most recently as cards. Each card is drawn
//! from its note's opening lines (see [`super::note_shape`]), so notes
//! tell apart by their layout. The arrows move between cards and Enter
//! opens one. Typing a letter starts a search, as if in the find field.
//!
//! The cards start with this session's notes and fill up with the
//! vault's most recently changed ones, which are found off the main
//! thread so a new tab opens at once however large the vault. Their
//! shapes are read off the main thread too, and draw in when they arrive
//! without moving anything.

use std::cell::Cell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    AnyElement, App, Context, EventEmitter, FocusHandle, Focusable, KeyDownEvent, MouseButton,
    Pixels, RenderImage, SharedString, Task, Window, canvas, div, prelude::*, px,
};

use super::files::{note_title, notes_by_recency};
use super::note_shape::{ShapeLine, read_shape};
use crate::icons::{IconName, icon};
use crate::keymap::RunCommand;
use crate::theme::UiTheme;
use crate::ui::Selectable;
use crate::ui::{keycap, truncated, ui_theme};

/// Notes the launcher shows at most.
pub const MAX_RECENT: usize = 8;

/// Folders the launcher reads at most when looking for recent notes.
pub(crate) const RECENT_SCAN_FOLDERS: usize = 200;

/// The most cards a row holds.
const MAX_COLUMNS: usize = 4;
/// A card's page, where its note's shape is drawn.
const CARD_WIDTH: f32 = 188.;
const PAGE_HEIGHT: f32 = 128.;
/// The lines of a note a card draws at most.
const SHAPE_LINES: usize = 8;

/// The launcher asks for a note to open in its tab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenRecent(pub PathBuf);

/// The launcher asks for the quick switcher, already holding what was
/// typed on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchFrom(pub String);

pub struct Launcher {
    focus_handle: FocusHandle,
    vault: PathBuf,
    recent: Vec<PathBuf>,
    selected: usize,
    shapes: HashMap<PathBuf, Vec<ShapeLine>>,
    /// How many cards fit on a row, as last laid out.
    columns: Rc<Cell<usize>>,
    /// The whale an empty vault shows, and whether it's the dark one.
    whale: Option<(bool, Arc<RenderImage>)>,
    _scan: Task<()>,
    _reading: Task<()>,
}

impl EventEmitter<OpenRecent> for Launcher {}
impl EventEmitter<SearchFrom> for Launcher {}

impl Focusable for Launcher {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Launcher {
    /// A launcher listing `recent` first, then the vault's most recently
    /// changed notes once they're found.
    pub fn new(vault: &Path, recent: Vec<PathBuf>, cx: &mut Context<Self>) -> Self {
        let root = vault.to_path_buf();
        let scanning =
            cx.background_spawn(async move { notes_by_recency(&root, RECENT_SCAN_FOLDERS) });
        let scan = cx.spawn(async move |launcher, cx| {
            let found = scanning.await;
            launcher
                .update(cx, |launcher, cx| launcher.add_found(found, cx))
                .ok();
        });
        let mut launcher = Self::empty(vault, recent, scan, cx);
        launcher.read_shapes(cx);
        launcher
    }

    /// A launcher listing `recent` first, then `found`, the vault's notes
    /// by recency as already read, as when the app starts.
    pub fn with_found(
        vault: &Path,
        recent: Vec<PathBuf>,
        found: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut launcher = Self::empty(vault, recent, Task::ready(()), cx);
        launcher.add_found(found, cx);
        launcher
    }

    fn empty(vault: &Path, recent: Vec<PathBuf>, scan: Task<()>, cx: &mut Context<Self>) -> Self {
        Launcher {
            focus_handle: cx.focus_handle(),
            vault: vault.to_path_buf(),
            recent: recent.into_iter().take(MAX_RECENT).collect(),
            selected: 0,
            shapes: HashMap::new(),
            columns: Rc::new(Cell::new(MAX_COLUMNS)),
            whale: None,
            _scan: scan,
            _reading: Task::ready(()),
        }
    }

    pub fn recent(&self) -> &[PathBuf] {
        &self.recent
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    /// The shape read for `note`, once it has been.
    pub fn shape(&self, note: &Path) -> Option<&[ShapeLine]> {
        self.shapes.get(note).map(Vec::as_slice)
    }

    /// Appends notes the scan found, after the ones already listed, so the
    /// card the keyboard is on stays put.
    fn add_found(&mut self, found: Vec<PathBuf>, cx: &mut Context<Self>) {
        for path in found {
            if self.recent.len() >= MAX_RECENT {
                break;
            }
            if !self.recent.contains(&path) {
                self.recent.push(path);
            }
        }
        self.read_shapes(cx);
        cx.notify();
    }

    /// Reads the shapes of the notes listed that haven't been read yet.
    fn read_shapes(&mut self, cx: &mut Context<Self>) {
        let unread: Vec<PathBuf> = self
            .recent
            .iter()
            .filter(|note| !self.shapes.contains_key(*note))
            .cloned()
            .collect();
        if unread.is_empty() {
            return;
        }
        let reading = cx.background_spawn(async move {
            unread
                .into_iter()
                .map(|note| {
                    let shape = read_shape(&note, SHAPE_LINES);
                    (note, shape)
                })
                .collect::<Vec<_>>()
        });
        self._reading = cx.spawn(async move |launcher, cx| {
            let read = reading.await;
            launcher
                .update(cx, |launcher, cx| {
                    launcher.shapes.extend(read);
                    cx.notify();
                })
                .ok();
        });
    }

    /// Moves the keyboard `delta` cards along, stopping at the first and
    /// last, so Down on the last row lands on the last card.
    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.recent.is_empty() {
            return;
        }
        let last = self.recent.len() as isize - 1;
        self.selected = (self.selected as isize + delta).clamp(0, last) as usize;
        cx.notify();
    }

    fn open(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(path) = self.recent.get(index) {
            cx.emit(OpenRecent(path.clone()));
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        if let Some(typed) = typed_text(keystroke) {
            cx.emit(SearchFrom(typed.to_owned()));
            cx.stop_propagation();
            return;
        }
        if keystroke.modifiers.modified() {
            return;
        }
        let row = self.columns.get().max(1) as isize;
        match keystroke.key.as_str() {
            "left" => self.move_selection(-1, cx),
            "right" => self.move_selection(1, cx),
            "up" => self.move_selection(-row, cx),
            "down" => self.move_selection(row, cx),
            "enter" => self.open(self.selected, cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    fn folder_label(&self, path: &Path) -> Option<SharedString> {
        let relative = path.strip_prefix(&self.vault).unwrap_or(path);
        let folder = relative.parent()?.to_string_lossy().replace('\\', "/");
        (!folder.is_empty()).then(|| folder.into())
    }

    fn render_card(
        &self,
        index: usize,
        path: &Path,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ui = ui_theme(cx);
        let selected = focused && index == self.selected;
        let shape = self.shapes.get(path).map(Vec::as_slice).unwrap_or_default();
        let folder = self.folder_label(path);
        let group = SharedString::from(format!("launcher-card-{index}"));
        div()
            .id(("recent", index))
            .selector(move || format!("launcher-card-{index}"))
            .group(group.clone())
            .flex()
            .flex_col()
            .flex_none()
            .w(px(CARD_WIDTH))
            .gap(ui.space_md)
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |launcher, _, _, cx| launcher.open(index, cx)),
            )
            .child(
                page(shape, &ui)
                    .when(selected, |page| page.shadow(vec![ui.focus()]))
                    .group_hover(group, |style| style.bg(ui.row_hover)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .px(ui.space_xs)
                    .child(
                        div()
                            .flex()
                            .text_color(if selected { ui.text_strong } else { ui.text })
                            .child(truncated(SharedString::from(note_title(path))).grow()),
                    )
                    .children(folder.map(|folder| {
                        div()
                            .flex()
                            .text_size(ui.small_font_size)
                            .text_color(ui.text_detail)
                            .child(truncated(folder).grow())
                    })),
            )
            .into_any_element()
    }

    /// The whale an empty vault shows, decoded once for the theme in effect.
    fn whale(&mut self, cx: &mut App) -> Option<Arc<RenderImage>> {
        let dark = crate::ui::is_dark(cx);
        if self
            .whale
            .as_ref()
            .is_none_or(|(was_dark, _)| *was_dark != dark)
        {
            self.whale = crate::tour::WhaleArt::still(dark).map(|still| (dark, still));
        }
        self.whale.as_ref().map(|(_, still)| still.clone())
    }

    fn render_empty(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let ui = ui_theme(cx);
        let whale = self
            .whale(cx)
            .map(|still| crate::tour::drawn_still(&still, px(220.)));
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(ui.space_xl)
            .pt(ui.space_xl * 3.)
            .children(whale)
            .child(
                div()
                    .text_color(ui.text_detail)
                    .child("This vault has no notes yet."),
            )
            .into_any_element()
    }
}

/// A card's page: its note's opening lines as bars.
fn page(shape: &[ShapeLine], ui: &UiTheme) -> gpui::Div {
    let inner = px(CARD_WIDTH) - ui.space_lg * 2.;
    div()
        .flex()
        .flex_col()
        .gap(ui.space_sm + ui.space_xs)
        .h(px(PAGE_HEIGHT))
        .p(ui.space_lg)
        .overflow_hidden()
        .rounded(ui.dialog_radius)
        .bg(ui.app_background)
        .children(shape.iter().map(|line| shape_line(*line, inner, ui)))
}

/// One line of a page.
fn shape_line(line: ShapeLine, width: Pixels, ui: &UiTheme) -> AnyElement {
    let text = |share: f32| bar(width * share, px(4.), ui.fill_strong);
    let marked = |mark: gpui::Div, share: f32| {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(ui.space_sm)
            .child(mark)
            .child(text(share * 0.85))
            .into_any_element()
    };
    match line {
        ShapeLine::Heading(share) => {
            bar(width * share * 0.7, px(7.), ui.text_faint).into_any_element()
        }
        ShapeLine::Text(share) => text(share).into_any_element(),
        ShapeLine::Item(share) => marked(bar(px(4.), px(4.), ui.text_faint), share),
        ShapeLine::Task { done, share } => {
            let fill = if done { ui.text_faint } else { ui.fill_strong };
            marked(div().size(px(7.)).rounded(px(2.)).bg(fill), share)
        }
        ShapeLine::Quote(share) => marked(bar(px(2.), px(8.), ui.text_faint), share),
        ShapeLine::Code(share) => div()
            .w(width)
            .px(ui.space_sm)
            .py(px(2.))
            .rounded(px(2.))
            .bg(ui.fill_strong)
            .child(bar(width * share * 0.8, px(3.), ui.text_faint))
            .into_any_element(),
        ShapeLine::Picture => div()
            .w(width * 0.62)
            .h(px(28.))
            .rounded(ui.menu_row_radius)
            .bg(ui.fill_strong)
            .into_any_element(),
    }
}

fn bar(width: Pixels, height: Pixels, color: gpui::Hsla) -> gpui::Div {
    div()
        .flex_none()
        .w(width.max(px(2.)))
        .h(height)
        .rounded_full()
        .bg(color)
}

/// The text a keystroke types, when it types any: a letter, digit or mark,
/// with Shift at most. Space and Enter stay keys.
fn typed_text(keystroke: &gpui::Keystroke) -> Option<&str> {
    let modifiers = &keystroke.modifiers;
    if modifiers.control || modifiers.alt || modifiers.platform || modifiers.function {
        return None;
    }
    keystroke.key_char.as_deref().filter(|text| {
        !text.is_empty()
            && text
                .chars()
                .all(|ch| !ch.is_whitespace() && !ch.is_control())
    })
}

/// The field that opens the quick switcher, drawn as the search field it
/// opens, with the caret waiting in it.
fn find_field(cx: &mut App) -> impl IntoElement {
    let ui = ui_theme(cx);
    let id = "switcher.open";
    div()
        .id(id)
        .selector(move || format!("launcher-{id}"))
        .flex()
        .flex_row()
        .flex_1()
        .min_w_0()
        .items_center()
        .gap(ui.space_md)
        .h(ui.row_height * 1.3)
        .px(ui.space_lg)
        .rounded(ui.dialog_radius)
        .bg(ui.app_background)
        .text_color(ui.text_detail)
        .cursor_text()
        .hover(|style| style.bg(ui.row_hover))
        .on_click(move |_, window, cx| {
            window.dispatch_action(Box::new(RunCommand { id: id.into() }), cx)
        })
        .child(
            icon(IconName::MagnifyingGlass)
                .size(ui.icon_size)
                .text_color(ui.icon),
        )
        .child(
            div()
                .w(ui.tour.caret_width)
                .h(ui.font_size * 1.2)
                .rounded_full()
                .bg(ui.caret_mark),
        )
        .child(div().flex_1().child("Find a note"))
        .children(crate::ui::hints::shortcut(id, cx).map(|shortcut| keycap(shortcut, &ui.keycap)))
}

fn new_note_button(cx: &mut App) -> impl IntoElement {
    let ui = ui_theme(cx);
    let id = "note.new";
    div()
        .id(id)
        .selector(move || format!("launcher-{id}"))
        .flex()
        .flex_row()
        .flex_none()
        .items_center()
        .gap(ui.space_md)
        .h(ui.row_height * 1.3)
        .px(ui.space_lg)
        .rounded(ui.dialog_radius)
        .bg(ui.accent)
        .text_color(ui.on_accent)
        .hover(|style| style.bg(ui.accent.opacity(0.85)))
        .on_click(move |_, window, cx| {
            window.dispatch_action(Box::new(RunCommand { id: id.into() }), cx)
        })
        .child(
            icon(IconName::NotePencil)
                .size(ui.icon_size)
                .text_color(ui.on_accent),
        )
        .child("New note")
}

/// An empty element that records how many cards fit on a row.
fn column_probe(columns: Rc<Cell<usize>>, gap: Pixels) -> impl IntoElement {
    canvas(
        move |bounds, _, _| {
            let fit = ((bounds.size.width + gap) / (px(CARD_WIDTH) + gap)).floor() as usize;
            columns.set(fit.clamp(1, MAX_COLUMNS));
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_full()
}

impl Render for Launcher {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = ui_theme(cx);
        let focused = self.focus_handle.contains_focused(window, cx);
        let recent = self.recent.clone();
        let cards: Vec<AnyElement> = recent
            .iter()
            .enumerate()
            .map(|(index, path)| self.render_card(index, path, focused, cx))
            .collect();
        let gap = ui.space_xl * 1.5;
        let row_width = px(CARD_WIDTH) * MAX_COLUMNS as f32 + gap * (MAX_COLUMNS - 1) as f32;
        let body = if cards.is_empty() {
            self.render_empty(cx)
        } else {
            div()
                .relative()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap(gap)
                .child(column_probe(self.columns.clone(), gap))
                .children(cards)
                .into_any_element()
        };
        div()
            .id("launcher")
            .key_context("Launcher")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .size_full()
            .flex()
            .justify_center()
            .px(ui.space_xl * 2.)
            .pt(ui.dialog_top_offset)
            .font_family(ui.font_family.clone())
            .text_size(ui.font_size)
            .text_color(ui.text)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w(row_width)
                    .max_w_full()
                    .gap(ui.space_xl * 2.)
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap(ui.space_md)
                            .child(find_field(cx))
                            .child(new_note_button(cx)),
                    )
                    .child(body),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cards_fit_as_many_to_a_row_as_there_is_room_for() {
        let gap = px(24.);
        let fit = |width: f32| ((px(width) + gap) / (px(CARD_WIDTH) + gap)).floor() as usize;
        assert_eq!(fit(CARD_WIDTH * 4. + 24. * 3.).clamp(1, MAX_COLUMNS), 4);
        assert_eq!(fit(CARD_WIDTH * 2. + 30.).clamp(1, MAX_COLUMNS), 2);
        assert_eq!(fit(100.).clamp(1, MAX_COLUMNS), 1);
    }
}
