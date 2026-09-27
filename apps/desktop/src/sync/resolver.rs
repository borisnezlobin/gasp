//! The conflict resolver: each place where this device and another one
//! changed the same lines, side by side, with a choice per place of which
//! version to keep. Finishing hands the choices to sync, which commits
//! the merge and carries on.
//!
//! It works from the keyboard alone: 1, 2 and 3 keep this device's
//! version, the other device's or both for the current place and move on
//! to the next one left to decide; Up and Down move between places (and
//! on into the next file); Enter finishes, or goes to the first place
//! still open.

use editor_config::keys::KeyChord;
use editor_sync::{ConflictHunk, ConflictedFile, Resolution, Segment};
use gpui::{
    AnyElement, App, ClickEvent, Context, DismissEvent, Entity, EventEmitter, FocusHandle,
    Focusable, KeyDownEvent, ScrollHandle, SharedString, Window, div, prelude::*, relative,
};

use super::service::SyncService;
use super::state::{count, file_label};
use crate::icons::{IconName, icon};
use crate::picker::shortcut::Shortcut;
use crate::settings_view::controls::{button, choice_button, inert_button};
use crate::settings_view::modal_size;
use crate::theme::{KeycapTheme, SettingsTheme};
use crate::ui::keycap;

/// Lines of unchanged text shown above each conflict, for orientation.
const CONTEXT_LINES: usize = 2;

/// Which version of one conflict to keep.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Mine,
    Theirs,
    Both,
}

impl Choice {
    const ALL: [Choice; 3] = [Choice::Mine, Choice::Theirs, Choice::Both];

    fn label(self) -> &'static str {
        match self {
            Choice::Mine => "Keep mine",
            Choice::Theirs => "Keep theirs",
            Choice::Both => "Keep both",
        }
    }

    /// The key that picks it, shown on its button.
    fn key(self) -> &'static str {
        match self {
            Choice::Mine => "1",
            Choice::Theirs => "2",
            Choice::Both => "3",
        }
    }

    fn from_key(key: &str) -> Option<Choice> {
        Choice::ALL.into_iter().find(|choice| choice.key() == key)
    }

    fn id(self) -> &'static str {
        match self {
            Choice::Mine => "mine",
            Choice::Theirs => "theirs",
            Choice::Both => "both",
        }
    }

    fn resolution(self) -> Resolution {
        match self {
            Choice::Mine => Resolution::ThisDevice,
            Choice::Theirs => Resolution::OtherDevice,
            Choice::Both => Resolution::Both,
        }
    }

    /// Whether this choice keeps this device's side, and the other's.
    fn keeps(self) -> (bool, bool) {
        match self {
            Choice::Mine => (true, false),
            Choice::Theirs => (false, true),
            Choice::Both => (true, true),
        }
    }
}

/// The resolver, shown as a modal.
pub struct ConflictResolver {
    service: Entity<SyncService>,
    files: Vec<ConflictedFile>,
    choices: Vec<Vec<Option<Choice>>>,
    selected: usize,
    /// The place in the selected file the keys act on.
    current: usize,
    focus_handle: FocusHandle,
    style: SettingsTheme,
    scroll: ScrollHandle,
}

impl EventEmitter<DismissEvent> for ConflictResolver {}

impl Focusable for ConflictResolver {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl ConflictResolver {
    pub fn new(service: Entity<SyncService>, cx: &mut Context<Self>) -> Self {
        let files = service.read(cx).conflicts().to_vec();
        let choices = files
            .iter()
            .map(|file| vec![None; file.hunk_count()])
            .collect();
        ConflictResolver {
            service,
            files,
            choices,
            selected: 0,
            current: 0,
            focus_handle: cx.focus_handle(),
            style: resolved_style(cx),
            scroll: ScrollHandle::new(),
        }
    }

    pub fn files(&self) -> &[ConflictedFile] {
        &self.files
    }

    /// Picks what to keep for hunk `hunk` of file `file`, which becomes
    /// the place the keys act on.
    pub fn choose(&mut self, file: usize, hunk: usize, choice: Choice, cx: &mut Context<Self>) {
        if let Some(slot) = self
            .choices
            .get_mut(file)
            .and_then(|file| file.get_mut(hunk))
        {
            *slot = Some(choice);
            if file == self.selected {
                self.current = hunk;
            }
            cx.notify();
        }
    }

    pub fn select_file(&mut self, file: usize, cx: &mut Context<Self>) {
        if file < self.files.len() {
            self.selected = file;
            self.current = 0;
            self.scroll.scroll_to_item(0);
            cx.notify();
        }
    }

    /// The file and place the keys act on.
    pub fn current(&self) -> (usize, usize) {
        (self.selected, self.current)
    }

    /// Every place in order, as (file, place).
    fn places(&self) -> Vec<(usize, usize)> {
        self.choices
            .iter()
            .enumerate()
            .flat_map(|(file, hunks)| (0..hunks.len()).map(move |hunk| (file, hunk)))
            .collect()
    }

    /// Makes (file, place) the current place and scrolls to it.
    fn go_to(&mut self, (file, hunk): (usize, usize), cx: &mut Context<Self>) {
        self.selected = file;
        self.current = hunk;
        self.scroll.scroll_to_item(hunk);
        cx.notify();
    }

    /// Moves `step` places on (back for a negative step), into the next
    /// or previous file at either end of this one.
    fn move_current(&mut self, step: isize, cx: &mut Context<Self>) {
        let places = self.places();
        let Some(at) = places.iter().position(|place| *place == self.current()) else {
            return;
        };
        let next = at.saturating_add_signed(step).min(places.len() - 1);
        self.go_to(places[next], cx);
    }

    /// The first place after the current one (wrapping) still to decide.
    fn next_open_place(&self) -> Option<(usize, usize)> {
        let places = self.places();
        let at = places.iter().position(|place| *place == self.current())?;
        let open = |(file, hunk): &&(usize, usize)| self.choices[*file][*hunk].is_none();
        places[at + 1..]
            .iter()
            .chain(&places[..=at])
            .find(open)
            .copied()
    }

    /// Keeps `choice` for the current place and goes on to the next one
    /// left to decide.
    fn choose_current(&mut self, choice: Choice, cx: &mut Context<Self>) {
        let (file, hunk) = self.current();
        self.choose(file, hunk, choice, cx);
        if let Some(next) = self.next_open_place() {
            self.go_to(next, cx);
        }
    }

    /// Enter: finishes when every place is decided, and otherwise goes to
    /// the next place still open.
    fn finish_or_go_on(&mut self, cx: &mut Context<Self>) {
        if self.is_complete() {
            return self.finish(cx);
        }
        if let Some(next) = self.next_open_place() {
            self.go_to(next, cx);
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        if keystroke.modifiers.modified() || self.files.is_empty() {
            return;
        }
        let key = keystroke.key.as_str();
        match key {
            "up" => self.move_current(-1, cx),
            "down" => self.move_current(1, cx),
            "enter" => self.finish_or_go_on(cx),
            _ => match Choice::from_key(key) {
                Some(choice) => self.choose_current(choice, cx),
                None => return,
            },
        }
        // The keys act on the current place, so it shows its ring.
        crate::ui::focus_visible::set_keyboard_driving(true, cx);
        cx.stop_propagation();
    }

    fn decided(&self) -> usize {
        self.choices
            .iter()
            .flatten()
            .filter(|c| c.is_some())
            .count()
    }

    fn total(&self) -> usize {
        self.choices.iter().map(Vec::len).sum()
    }

    /// Whether every conflict in every file has a choice.
    pub fn is_complete(&self) -> bool {
        self.decided() == self.total()
    }

    /// Hands the choices to sync and closes.
    pub fn finish(&mut self, cx: &mut Context<Self>) {
        if !self.is_complete() {
            return;
        }
        let resolutions: Vec<Vec<Resolution>> = self
            .choices
            .iter()
            .map(|file| file.iter().flatten().map(|c| c.resolution()).collect())
            .collect();
        self.service
            .update(cx, |service, cx| service.resolve(resolutions, cx));
        cx.emit(DismissEvent);
    }

    // ---- Drawing ----

    fn render_header(&self) -> AnyElement {
        let style = &self.style;
        let file = &self.files[self.selected];
        let intro = format!(
            "{} changed in {} on this device and on another one. Pick what to keep in each place.",
            file_label(&file.path),
            count(file.hunk_count(), "place")
        );
        div()
            .flex()
            .flex_col()
            .gap(style.text_gap)
            .child(
                div()
                    .text_size(style.page_title_size)
                    .font_weight(style.strong_weight)
                    .child("Resolve sync conflicts"),
            )
            .child(
                div()
                    .text_size(style.small_text_size)
                    .text_color(style.text_muted)
                    .child(intro),
            )
            .into_any_element()
    }

    /// The list of files, when there's more than one.
    fn render_files(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.files.len() < 2 {
            return None;
        }
        let style = &self.style;
        let items = self.files.iter().enumerate().map(|(index, file)| {
            let done = self.choices[index].iter().all(Option::is_some);
            let selected = index == self.selected;
            let selector = format!("resolver-file-{index}");
            div()
                .id(("resolver-file", index))
                .debug_selector(|| selector)
                .flex()
                .items_center()
                .gap(style.control_gap)
                .h(style.nav_item_height)
                .px(style.control_gap)
                .rounded(style.radius)
                .cursor_pointer()
                .when(selected, |item| item.bg(style.selected))
                .when(!selected, |item| item.hover(|item| item.bg(style.hover)))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .child(file_label(&file.path)),
                )
                .when(done, |item| {
                    item.child(
                        icon(IconName::Check)
                            .size(style.small_icon_size)
                            .text_color(style.text_muted),
                    )
                })
                .on_click(
                    cx.listener(move |view, _: &ClickEvent, _, cx| view.select_file(index, cx)),
                )
        });
        Some(
            div()
                .flex_none()
                .w(style.nav_width)
                .h_full()
                .p(style.nav_padding)
                .bg(style.card_background)
                .flex()
                .flex_col()
                .gap(style.gap_xs)
                .children(items)
                .into_any_element(),
        )
    }

    fn render_hunks(&self, cx: &mut Context<Self>) -> AnyElement {
        let file = self.selected;
        let mut previous_clean: Option<&str> = None;
        let mut hunk_index = 0;
        let mut cards = Vec::new();
        for segment in &self.files[file].segments {
            match segment {
                Segment::Clean(text) => previous_clean = Some(text),
                Segment::Conflict(hunk) => {
                    let context = previous_clean.take().map(last_lines);
                    cards.push(self.render_hunk(file, hunk_index, hunk, context, cx));
                    hunk_index += 1;
                }
            }
        }
        div()
            .id("resolver-hunks")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .flex()
            .flex_col()
            .gap(self.style.card_gap)
            // Room for the current place's ring, which the scrolling clips.
            .p(self.style.gap_xs)
            .children(cards)
            .into_any_element()
    }

    fn render_hunk(
        &self,
        file: usize,
        index: usize,
        hunk: &ConflictHunk,
        context: Option<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let style = &self.style;
        let keys = crate::ui::ui_theme(cx).keycap.compact();
        let choice = self.choices[file][index];
        let (keeps_mine, keeps_theirs) = choice.map_or((true, true), Choice::keeps);
        let ringed = crate::ui::focus_visible::ring(index == self.current, cx);
        let decided = choice.is_some();
        let sides = div()
            .flex()
            .gap(style.control_gap)
            .child(self.render_side(
                "This device",
                &hunk.this_device,
                !hunk.base.is_empty(),
                decided && keeps_mine,
                !keeps_mine,
            ))
            .child(self.render_side(
                "Other device",
                &hunk.other_device,
                !hunk.base.is_empty(),
                decided && keeps_theirs,
                !keeps_theirs,
            ));
        let buttons =
            Choice::ALL.map(|option| {
                let selector = format!("resolve-{index}-{}", option.id());
                let chosen = choice == Some(option);
                choice_button(
                    SharedString::from(selector.clone()),
                    option.label(),
                    chosen,
                    style,
                )
                .child(key_chip(option.key(), &keys))
                .debug_selector(|| selector)
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    view.choose(file, index, option, cx)
                }))
            });
        div()
            .flex()
            .flex_col()
            .gap(style.control_gap)
            .p(style.gap_sm)
            .rounded(style.radius)
            // Opaque under the ring, which would otherwise fill the card in.
            .when(ringed, |card| {
                card.bg(style.background).shadow(vec![style.focus()])
            })
            .children(context.map(|context| {
                // Context orients; one line each is enough.
                let lines = context
                    .lines()
                    .map(|line| div().truncate().child(line.to_owned()))
                    .collect::<Vec<_>>();
                div()
                    .flex()
                    .flex_col()
                    .text_size(style.small_text_size)
                    .text_color(style.text_faint)
                    .children(lines)
            }))
            .child(sides)
            .child(
                div()
                    .flex()
                    .gap(style.control_gap)
                    .text_size(style.small_text_size)
                    .children(buttons),
            )
            .into_any_element()
    }

    /// One version of a hunk, ringed when it's kept and faded when it isn't.
    fn render_side(
        &self,
        title: &'static str,
        text: &str,
        had_base: bool,
        kept: bool,
        dropped: bool,
    ) -> AnyElement {
        let style = &self.style;
        let body = if text.is_empty() {
            let note = if had_base {
                "Removed here"
            } else {
                "Nothing here"
            };
            div().text_color(style.text_faint).child(note)
        } else {
            div().child(text.trim_end_matches('\n').to_owned())
        };
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(style.gap_sm)
            .child(
                div()
                    .text_size(style.small_text_size)
                    .text_color(style.text_muted)
                    .child(title),
            )
            .child(
                div()
                    .p(style.card_padding_x * 0.75)
                    .rounded(style.radius)
                    .bg(style.card_background)
                    .when(kept, |side| side.shadow(vec![style.ring(style.text_faint)]))
                    .when(dropped, |side| side.text_color(style.text_faint))
                    .child(body),
            )
            .into_any_element()
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let style = &self.style;
        let progress = format!("{} of {} decided", self.decided(), self.total());
        let keys = crate::ui::ui_theme(cx).keycap.compact();
        let finish = if self.is_complete() {
            button("resolver-finish", "Finish merge", true, false, style)
                .gap(style.gap_sm)
                .child(key_chip("Enter", &keys.on_text(style.on_accent)))
                .debug_selector(|| "resolver-finish".to_owned())
                .on_click(cx.listener(|view, _: &ClickEvent, _, cx| view.finish(cx)))
        } else {
            inert_button("resolver-finish", "Finish merge", style)
                .debug_selector(|| "resolver-finish".to_owned())
        };
        let cancel = button("resolver-cancel", "Not now", false, false, style)
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(DismissEvent)));
        div()
            .flex()
            .items_center()
            .gap(style.control_gap)
            .text_size(style.small_text_size)
            .child(div().flex_1().text_color(style.text_muted).child(progress))
            .child(cancel)
            .child(finish)
            .into_any_element()
    }
}

/// The key that does something, as a small chip on the button that does
/// the same.
fn key_chip(key: &str, theme: &KeycapTheme) -> impl IntoElement {
    let chord = KeyChord::parse(key).expect("resolver keys parse");
    keycap(
        Shortcut::new(chord, editor_config::Platform::current()),
        theme,
    )
}

/// The settings look in the theme in effect, with the interface font
/// this system has.
pub(super) fn resolved_style(cx: &mut gpui::App) -> SettingsTheme {
    crate::ui::settings_theme(cx)
}

/// The last few lines of `text`, for context above a conflict.
fn last_lines(text: &str) -> String {
    let lines: Vec<&str> = text.trim_end_matches('\n').lines().collect();
    let start = lines.len().saturating_sub(CONTEXT_LINES);
    lines[start..].join("\n")
}

impl Render for ConflictResolver {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The theme can change while the resolver is open.
        self.style = resolved_style(cx);
        let style = self.style.clone();
        let size = modal_size(window.viewport_size(), &style);
        let root = div()
            .id("conflict-resolver")
            .key_context("ConflictResolver")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .w(size.width)
            .h(size.height)
            .flex()
            .overflow_hidden()
            .rounded(style.modal_radius)
            .bg(style.background)
            .shadow(vec![style.outline(), style.popover_shadow()])
            .font_family(style.font_family.clone())
            .text_size(style.text_size)
            .line_height(relative(style.line_height_factor))
            .text_color(style.text);
        if self.files.is_empty() {
            return root.child(
                div()
                    .p(style.content_padding_x)
                    .text_color(style.text_muted)
                    .child("There's nothing left to resolve."),
            );
        }
        root.children(self.render_files(cx)).child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(style.card_gap)
                .px(style.content_padding_x)
                .py(style.content_padding_y)
                .child(self.render_header())
                .child(self.render_hunks(cx))
                .child(self.render_footer(cx)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_is_the_last_two_lines() {
        assert_eq!(last_lines("a\nb\nc\n"), "b\nc");
        assert_eq!(last_lines("only\n"), "only");
        assert_eq!(last_lines(""), "");
    }

    #[test]
    fn choices_map_to_resolutions() {
        assert_eq!(Choice::Mine.resolution(), Resolution::ThisDevice);
        assert_eq!(Choice::Theirs.resolution(), Resolution::OtherDevice);
        assert_eq!(Choice::Both.resolution(), Resolution::Both);
        assert_eq!(Choice::Both.keeps(), (true, true));
    }
}
