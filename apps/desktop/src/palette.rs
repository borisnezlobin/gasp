//! The command palette (`palette.open`, Mod+P): every palette
//! command with its current shortcut, recently used ones first.
//!
//! Enter runs the command. Mod+Enter asks for a new shortcut: the next
//! chord pressed becomes [`PaletteEvent::Bind`], which the owner writes to
//! `rules.toml`.

use editor_config::{CommandRegistry, Platform, RuleSet};
use gpui::{
    AnyElement, App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable,
    Keystroke, ParentElement, Render, SharedString, Subscription, Window, div, prelude::*,
};

use crate::picker::fuzzy::{Candidate, Matcher, Query};
use crate::picker::shortcut::{Shortcut, capture_chord, is_lone_modifier};
use crate::picker::{Confirmed, Picker, PickerDelegate, highlighted_text};
use crate::theme::{InputTheme, PickerTheme, UiTheme};
use crate::ui::keycap;

/// Extra score for the most recently used command, falling by
/// [`RECENT_STEP`] per place in the recent list.
const RECENT_BOOST: i32 = 24;
const RECENT_STEP: i32 = 3;
/// How much a match that needs the category ranks below a title match.
const CATEGORY_MATCH_PENALTY: i32 = 30;

/// What the palette asks its owner to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaletteEvent {
    /// Run this command id.
    Run(String),
    /// Bind `chord` (portable, such as `Mod+Shift+K`) to `command`.
    Bind { command: String, chord: String },
}

/// One command as the palette lists it.
#[derive(Clone, Debug)]
pub struct PaletteCommand {
    pub id: String,
    pub title: String,
    pub category: String,
    /// The shortcuts that run it on this platform.
    pub shortcuts: Vec<Shortcut>,
    /// Its place in the recent list, most recent first.
    pub recent_rank: Option<usize>,
    title_candidate: Candidate,
    /// `category title`, for queries that name the category.
    full_candidate: Candidate,
}

#[derive(Clone, Debug)]
struct PaletteMatch {
    command: usize,
    score: i32,
    title_positions: Vec<usize>,
    category_positions: Vec<usize>,
}

/// A command the palette's delegate picked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaletteChoice {
    Run(String),
    Bind(String),
}

/// The palette's items and matching.
pub struct PaletteDelegate {
    commands: Vec<PaletteCommand>,
    matches: Vec<PaletteMatch>,
    matcher: Matcher,
}

impl PaletteDelegate {
    /// Every palette command, with shortcuts from `rules` on `platform`,
    /// ordered recent first and then by title.
    pub fn new(rules: &RuleSet, recent: &[String], platform: Platform) -> Self {
        let registry = CommandRegistry::<()>::with_builtins();
        let mut commands: Vec<PaletteCommand> = registry
            .commands()
            .filter(|info| info.palette)
            .map(|info| {
                let shortcuts = rules
                    .keys_for(&info.id, platform)
                    .into_iter()
                    .map(|chord| Shortcut::new(chord, platform))
                    .collect();
                PaletteCommand {
                    id: info.id.clone(),
                    title: info.title.clone(),
                    category: info.category.clone(),
                    shortcuts,
                    recent_rank: recent.iter().position(|id| *id == info.id),
                    title_candidate: Candidate::new(&info.title),
                    full_candidate: Candidate::new(&format!("{} {}", info.category, info.title)),
                }
            })
            .collect();
        commands.sort_by(|a, b| {
            let rank = |command: &PaletteCommand| command.recent_rank.unwrap_or(usize::MAX);
            rank(a).cmp(&rank(b)).then_with(|| a.title.cmp(&b.title))
        });
        Self {
            commands,
            matches: Vec::new(),
            matcher: Matcher::new(),
        }
    }

    pub fn commands(&self) -> &[PaletteCommand] {
        &self.commands
    }

    /// The command shown in row `index`.
    pub fn command_at(&self, index: usize) -> Option<&PaletteCommand> {
        let found = self.matches.get(index)?;
        self.commands.get(found.command)
    }

    fn match_command(&mut self, query: &Query, index: usize) -> Option<PaletteMatch> {
        let command = &self.commands[index];
        let recency = command
            .recent_rank
            .map_or(0, |rank| (RECENT_BOOST - RECENT_STEP * rank as i32).max(0));
        if let Some(found) = self.matcher.score(query, &command.title_candidate) {
            return Some(PaletteMatch {
                command: index,
                score: found.score + recency,
                title_positions: found.positions,
                category_positions: Vec::new(),
            });
        }
        let found = self.matcher.score(query, &command.full_candidate)?;
        let title_start = command.category.len() + 1;
        let (category_positions, title_positions): (Vec<usize>, Vec<usize>) = found
            .positions
            .iter()
            .partition(|position| **position < title_start);
        Some(PaletteMatch {
            command: index,
            score: found.score + recency - CATEGORY_MATCH_PENALTY,
            title_positions: title_positions.iter().map(|p| p - title_start).collect(),
            category_positions,
        })
    }
}

impl PickerDelegate for PaletteDelegate {
    type Event = PaletteChoice;

    fn placeholder(&self) -> SharedString {
        "Run a command".into()
    }

    fn match_count(&self) -> usize {
        self.matches.len()
    }

    fn update_matches(&mut self, query: &str) {
        let query = Query::new(query);
        let mut matches: Vec<PaletteMatch> = (0..self.commands.len())
            .filter_map(|index| self.match_command(&query, index))
            .collect();
        if !query.is_empty() {
            matches.sort_by(|a, b| b.score.cmp(&a.score).then(a.command.cmp(&b.command)));
        }
        self.matches = matches;
    }

    fn render_match(&self, index: usize, _selected: bool, theme: &PickerTheme) -> AnyElement {
        let (Some(found), Some(command)) = (self.matches.get(index), self.command_at(index)) else {
            return div().into_any_element();
        };
        let title = highlighted_text(command.title.clone(), &found.title_positions, theme);
        let category = highlighted_text(command.category.clone(), &found.category_positions, theme);
        div()
            .flex()
            .items_center()
            .gap(theme.row_gap)
            .w_full()
            .text_size(theme.row_font_size)
            .child(div().flex_none().child(title))
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_size(theme.detail_font_size)
                    .text_color(theme.detail_text)
                    .child(category),
            )
            .children(
                command
                    .shortcuts
                    .iter()
                    .map(|shortcut| keycap(*shortcut, &theme.keycap)),
            )
            .into_any_element()
    }

    fn confirm(&mut self, index: usize) -> Option<PaletteChoice> {
        let command = self.command_at(index)?;
        Some(PaletteChoice::Run(command.id.clone()))
    }

    fn secondary_confirm(&mut self, index: usize) -> Option<PaletteChoice> {
        let command = self.command_at(index)?;
        Some(PaletteChoice::Bind(command.id.clone()))
    }

    fn empty_message(&self, query: &str) -> SharedString {
        format!("No commands match “{}”.", query.trim()).into()
    }
}

/// Waiting for the chord to bind to a command.
struct Capture {
    command: String,
    title: String,
    current: Vec<Shortcut>,
    rejection: Option<SharedString>,
    _interceptor: Subscription,
}

/// The command palette. Emits [`PaletteEvent`], then [`DismissEvent`] when
/// it's done.
pub struct CommandPalette {
    picker: Entity<Picker<PaletteDelegate>>,
    capture: Option<Capture>,
    capture_focus: FocusHandle,
    platform: Platform,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<PaletteEvent> for CommandPalette {}
impl EventEmitter<DismissEvent> for CommandPalette {}

impl Focusable for CommandPalette {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        if self.capture.is_some() {
            return self.capture_focus.clone();
        }
        self.picker.focus_handle(cx)
    }
}

impl CommandPalette {
    /// A palette showing shortcuts from `rules`, with the command ids in
    /// `recent` (most recent first) at the top.
    pub fn new(
        rules: &RuleSet,
        recent: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::for_platform(rules, recent, Platform::current(), window, cx)
    }

    /// Like [`CommandPalette::new`], showing shortcuts as `platform` would.
    pub fn for_platform(
        rules: &RuleSet,
        recent: Vec<String>,
        platform: Platform,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let delegate = PaletteDelegate::new(rules, &recent, platform);
        let picker = cx.new(|cx| Picker::new(delegate, window, cx));
        let subscriptions = vec![
            cx.subscribe_in(&picker, window, Self::on_choice),
            cx.subscribe(&picker, |_, _, _: &DismissEvent, cx| cx.emit(DismissEvent)),
        ];
        Self {
            picker,
            capture: None,
            capture_focus: cx.focus_handle(),
            platform,
            _subscriptions: subscriptions,
        }
    }

    pub fn picker(&self) -> &Entity<Picker<PaletteDelegate>> {
        &self.picker
    }

    /// The command waiting for a new shortcut, if any.
    pub fn capturing(&self) -> Option<&str> {
        self.capture
            .as_ref()
            .map(|capture| capture.command.as_str())
    }

    /// Why the last chord pressed while capturing was refused.
    pub fn capture_rejection(&self) -> Option<SharedString> {
        self.capture.as_ref()?.rejection.clone()
    }

    fn on_choice(
        &mut self,
        _: &Entity<Picker<PaletteDelegate>>,
        choice: &Confirmed<PaletteChoice>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match &choice.0 {
            PaletteChoice::Run(id) => {
                cx.emit(PaletteEvent::Run(id.clone()));
                cx.emit(DismissEvent);
            }
            PaletteChoice::Bind(id) => self.start_capture(id, window, cx),
        }
    }

    /// Waits for the next chord and binds it to `command`.
    pub fn start_capture(&mut self, command: &str, window: &mut Window, cx: &mut Context<Self>) {
        let delegate = self.picker.read(cx).delegate();
        let Some(found) = delegate.commands().iter().find(|c| c.id == command) else {
            return;
        };
        let (title, current) = (found.title.clone(), found.shortcuts.clone());
        let palette = cx.entity().downgrade();
        let interceptor = cx.intercept_keystrokes(move |event, window, cx| {
            if let Some(palette) = palette.upgrade() {
                palette.update(cx, |palette, cx| {
                    palette.on_captured_keystroke(&event.keystroke, window, cx);
                });
            }
        });
        self.capture = Some(Capture {
            command: command.to_string(),
            title,
            current,
            rejection: None,
            _interceptor: interceptor,
        });
        window.focus(&self.capture_focus);
        cx.notify();
    }

    /// Leaves capture and goes back to the list.
    pub fn cancel_capture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.capture = None;
        window.focus(&self.picker.focus_handle(cx));
        cx.notify();
    }

    fn on_captured_keystroke(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.capture.is_none() || !self.capture_focus.is_focused(window) {
            return;
        }
        if is_lone_modifier(keystroke) {
            return;
        }
        cx.stop_propagation();
        if keystroke.key == "escape" && !keystroke.modifiers.modified() {
            self.cancel_capture(window, cx);
            return;
        }
        match capture_chord(keystroke, self.platform) {
            Ok(chord) => self.finish_capture(chord, cx),
            Err(reason) => {
                if let Some(capture) = self.capture.as_mut() {
                    capture.rejection = Some(reason.into());
                }
                cx.notify();
            }
        }
    }

    fn finish_capture(&mut self, chord: String, cx: &mut Context<Self>) {
        let Some(capture) = self.capture.take() else {
            return;
        };
        cx.emit(PaletteEvent::Bind {
            command: capture.command,
            chord,
        });
        cx.emit(DismissEvent);
        cx.notify();
    }

    fn render_capture(&self, capture: &Capture, theme: &PickerTheme, ui: &UiTheme) -> AnyElement {
        let hint = capture.rejection.clone().map_or_else(
            || current_keys(&capture.current, theme).text_color(theme.detail_text),
            |reason| div().text_color(theme.warning_text).child(reason),
        );
        crate::ui::dialog(ui)
            .key_context("PaletteCapture")
            .track_focus(&self.capture_focus)
            .w(theme.width)
            .gap(theme.capture_gap)
            .px(theme.input_padding_x)
            .py(theme.input_padding_y)
            .child(
                div()
                    .text_size(InputTheme::default().query_font_size)
                    .child(format!("Press the new shortcut for “{}”", capture.title)),
            )
            .child(div().text_size(theme.detail_font_size).child(hint))
            .child(
                div()
                    .text_size(theme.detail_font_size)
                    .text_color(theme.detail_text)
                    .child("Escape goes back to the list."),
            )
            .into_any_element()
    }
}

impl Render for CommandPalette {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match &self.capture {
            Some(capture) => {
                let theme = self.picker.read(cx).theme().clone();
                self.render_capture(capture, &theme, &crate::ui::ui_theme(cx))
            }
            None => self.picker.clone().into_any_element(),
        }
    }
}

/// "Now Ctrl+B or Ctrl+Shift+B.", with each shortcut drawn as keys.
fn current_keys(current: &[Shortcut], theme: &PickerTheme) -> gpui::Div {
    let line = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(theme.capture_gap);
    if current.is_empty() {
        return line.child("It has no shortcut yet.");
    }
    let mut line = line.child("Now");
    for (index, shortcut) in current.iter().enumerate() {
        if index > 0 {
            line = line.child("or");
        }
        line = line.child(keycap(*shortcut, &theme.keycap));
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    fn delegate(recent: &[&str]) -> PaletteDelegate {
        let recent: Vec<String> = recent.iter().map(|id| id.to_string()).collect();
        PaletteDelegate::new(&RuleSet::defaults(), &recent, Platform::Linux)
    }

    fn ids(delegate: &PaletteDelegate) -> Vec<String> {
        (0..delegate.match_count())
            .map(|index| delegate.command_at(index).unwrap().id.clone())
            .collect()
    }

    #[test]
    fn lists_only_palette_commands() {
        let mut palette = delegate(&[]);
        palette.update_matches("");
        let ids = ids(&palette);
        assert!(ids.contains(&"format.bold".to_string()));
        assert!(!ids.contains(&"palette.open".to_string()));
        assert!(!ids.contains(&"cursor.left".to_string()));
    }

    #[test]
    fn recent_commands_come_first() {
        let mut palette = delegate(&["tab.new", "format.bold", "no.such-command"]);
        palette.update_matches("");
        assert_eq!(ids(&palette)[..2], ["tab.new", "format.bold"]);
    }

    #[test]
    fn recency_breaks_ties_between_matches() {
        let mut palette = delegate(&["tab.go-5"]);
        palette.update_matches("go to tab");
        assert_eq!(ids(&palette)[0], "tab.go-5");
    }

    #[test]
    fn category_words_find_commands() {
        let mut palette = delegate(&[]);
        palette.update_matches("formatting bold");
        assert_eq!(ids(&palette)[0], "format.bold");
        let found = &palette.matches[0];
        assert!(!found.category_positions.is_empty());
    }

    #[test]
    fn shortcuts_show_per_platform() {
        let rules = RuleSet::defaults();
        let find = |palette: &PaletteDelegate, id: &str| {
            palette
                .commands()
                .iter()
                .find(|c| c.id == id)
                .unwrap()
                .shortcuts
                .clone()
        };
        let mac = PaletteDelegate::new(&rules, &[], Platform::Macos);
        let linux = PaletteDelegate::new(&rules, &[], Platform::Linux);
        assert_eq!(find(&mac, "search.open"), ["⇧⌘F"]);
        assert_eq!(find(&linux, "search.open"), ["Ctrl+Shift+F"]);
        assert_eq!(find(&linux, "format.bold"), ["Ctrl+B"]);
    }
}
