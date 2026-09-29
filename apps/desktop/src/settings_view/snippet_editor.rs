//! The editor a snippet opens into on the Snippets page: what you type
//! and what it becomes as text fields, where and when it fires as real
//! controls, and a box to try it in that shows what typing gives as you
//! type. Tab walks the fields and controls, arrows change a choice, Space
//! flips a switch, Enter saves and Escape closes.

use gasp_snippets::{
    FileLine, Fire, InputContext, Options, Preview, STOP_GLYPH, Scope, SnippetEngine,
    format_expansion, format_options, format_trigger, parse_snippet, preview,
};
use gpui::{
    AnyElement, AppContext, ClickEvent, Context, Entity, FocusHandle, Focusable, Keystroke,
    ListOffset, SharedString, Stateful, Subscription, Window, div, prelude::*, px,
};

use super::controls::{button, control_note, field_box, segment, segmented, toggle_switch};
use super::snippets_page::{SNIPPETS_KEY, add_snippet, capitalised, test_context};
use super::view::{ControlRow, SettingsFocus, SettingsView};
use crate::text_input::{TextInput, TextInputEvent, TextInputStyle};

/// A text field of the snippet editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorField {
    Trigger,
    Expansion,
    Test,
}

impl EditorField {
    const ALL: [EditorField; 3] = [
        EditorField::Trigger,
        EditorField::Expansion,
        EditorField::Test,
    ];
}

/// A control of the snippet editor for one of the snippet's options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionControl {
    Place,
    Fire,
    WholeWord,
    AfterSpace,
}

/// Where the keyboard can be in the editor, in the order Tab walks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorStop {
    Field(EditorField),
    Control(OptionControl),
}

const STOPS: [EditorStop; 7] = [
    EditorStop::Field(EditorField::Trigger),
    EditorStop::Field(EditorField::Expansion),
    EditorStop::Control(OptionControl::Place),
    EditorStop::Control(OptionControl::Fire),
    EditorStop::Control(OptionControl::WholeWord),
    EditorStop::Control(OptionControl::AfterSpace),
    EditorStop::Field(EditorField::Test),
];

/// Where a snippet works, as the editor offers it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Anywhere,
    Text,
    Math,
}

impl Place {
    const ALL: [(Place, &'static str); 3] = [
        (Place::Anywhere, "Anywhere"),
        (Place::Text, "In text"),
        (Place::Math, "In math"),
    ];

    /// The place `scopes` read as, or `None` for a mix the editor has no
    /// choice for, such as code only.
    pub fn of(scopes: &[Scope]) -> Option<Place> {
        let math = |scope: &Scope| {
            matches!(
                scope,
                Scope::Context(InputContext::Math) | Scope::InlineMath | Scope::BlockMath
            )
        };
        match scopes {
            [] => Some(Place::Anywhere),
            [Scope::Context(InputContext::Text)] => Some(Place::Text),
            scopes if scopes.iter().all(math) => Some(Place::Math),
            _ => None,
        }
    }

    fn scopes(self) -> Vec<Scope> {
        match self {
            Place::Anywhere => Vec::new(),
            Place::Text => vec![Scope::Context(InputContext::Text)],
            Place::Math => vec![Scope::Context(InputContext::Math)],
        }
    }
}

const FIRES: [(Fire, &str); 2] = [(Fire::Instant, "As you type"), (Fire::OnTab, "On Tab")];

/// The options a new snippet starts with: in math, as you type.
fn new_snippet_options() -> Options {
    Options {
        scopes: Place::Math.scopes(),
        fire: Fire::Instant,
        ..Options::default()
    }
}

/// The snippet open in the editor.
pub struct SnippetEditor {
    /// The snippet's line in the file, or `None` for a new one.
    pub line: Option<usize>,
    fields: [Entity<TextInput>; 3],
    /// The options as the controls set them. Ones without a control, such
    /// as a priority or being off, are kept as they were.
    pub options: Options,
    controls: [FocusHandle; 4],
    /// What's wrong with the snippet as written, and in which field.
    pub problem: Option<(EditorField, String)>,
    /// What typing the test text gives.
    pub result: Option<Preview>,
    _subscriptions: Vec<Subscription>,
}

impl SnippetEditor {
    pub fn field(&self, field: EditorField) -> &Entity<TextInput> {
        &self.fields[field as usize]
    }

    fn control(&self, control: OptionControl) -> &FocusHandle {
        &self.controls[control as usize]
    }

    fn stop_handle(&self, stop: EditorStop, cx: &gpui::App) -> FocusHandle {
        match stop {
            EditorStop::Field(field) => self.field(field).focus_handle(cx),
            EditorStop::Control(control) => self.control(control).clone(),
        }
    }

    /// The stop that has the keyboard, if one does.
    fn focused_stop(&self, window: &Window, cx: &gpui::App) -> Option<usize> {
        STOPS
            .iter()
            .position(|stop| self.stop_handle(*stop, cx).is_focused(window))
    }
}

impl SettingsView {
    /// The snippet open in the editor as one line of the file, what's
    /// wrong with it, and what typing the test text gives.
    pub fn snippet_editor_state(
        &self,
        cx: &gpui::App,
    ) -> Option<(String, Option<String>, Option<Preview>)> {
        let editor = self.snippet_editor.as_ref()?;
        let (line, _) = editor_line(editor, cx);
        let problem = editor.problem.as_ref().map(|(_, message)| message.clone());
        Some((line, problem, editor.result.clone()))
    }

    /// The editor's field, for tests and for focusing it.
    pub fn snippet_field(&self, field: EditorField) -> Option<Entity<TextInput>> {
        self.snippet_editor
            .as_ref()
            .map(|editor| editor.field(field).clone())
    }

    /// Where the keyboard is in the editor, if it's there.
    pub fn snippet_editor_stop(&self, window: &Window, cx: &gpui::App) -> Option<EditorStop> {
        let editor = self.snippet_editor.as_ref()?;
        editor.focused_stop(window, cx).map(|at| STOPS[at])
    }

    /// Opens the editor on the snippet at `line`, or on a new one.
    pub fn open_snippet_editor(
        &mut self,
        line: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let snippet = line.and_then(|line| match self.typing_lists.snippets.lines.get(line) {
            Some(FileLine::Snippet(snippet)) => Some(snippet.clone()),
            _ => None,
        });
        let (texts, options) = match &snippet {
            Some(snippet) => (
                [
                    format_trigger(&snippet.trigger),
                    format_expansion(snippet),
                    snippet.trigger.literal().unwrap_or_default().to_string(),
                ],
                snippet.options.clone(),
            ),
            None => (Default::default(), new_snippet_options()),
        };
        let fields = self.editor_fields(&texts, window, cx);
        let subscriptions = EditorField::ALL
            .iter()
            .map(|which| {
                cx.subscribe_in(
                    &fields[*which as usize],
                    window,
                    move |view, _, event: &TextInputEvent, window, cx| {
                        view.on_editor_event(event, window, cx)
                    },
                )
            })
            .collect();
        window.focus(&fields[0].focus_handle(cx));
        self.snippet_editor = Some(SnippetEditor {
            line,
            fields,
            options,
            controls: std::array::from_fn(|_| cx.focus_handle()),
            problem: None,
            result: None,
            _subscriptions: subscriptions,
        });
        self.error = None;
        self.check_snippet(cx);
        self.invalidate_layouts();
        self.reveal_snippet_editor(window, cx);
        cx.notify();
    }

    fn editor_fields(
        &self,
        texts: &[String; 3],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> [Entity<TextInput>; 3] {
        let placeholders = ["mk", "\\frac{●}{●}●", "Type to try it"];
        let code_font = self.style.code_font_family.clone();
        std::array::from_fn(|at| {
            let field = cx.new(|cx| {
                TextInput::new(window, cx)
                    .with_placeholder(placeholders[at])
                    .with_style(TextInputStyle::Query)
                    .with_font_family(code_font.clone())
            });
            field.update(cx, |field, cx| field.set_text(&texts[at], cx));
            field
        })
    }

    /// Points the page's focus at the editor and scrolls it into view.
    /// Rows far down a long page aren't measured yet, so one that isn't
    /// on screen is first brought to the top; once the editor has been
    /// drawn at its real height, a second scroll shows all of it.
    fn reveal_snippet_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self
            .layout()
            .rows
            .iter()
            .position(|row| *row == ControlRow::SnippetEditor)
        else {
            return;
        };
        self.focus = SettingsFocus::Control(index);
        let item = self.child_index(index);
        if self.list.bounds_for_item(item).is_some() {
            self.reveal_row(index, cx);
        } else {
            // The snippet's own row goes at the top, with the editor under it.
            self.list.scroll_to(ListOffset {
                item_ix: item.saturating_sub(1),
                offset_in_item: px(0.),
            });
        }
        let view = cx.entity().downgrade();
        window.on_next_frame(move |_, cx| {
            view.update(cx, |view, cx| {
                // The item after the editor comes into view with it, so its
                // buttons don't sit on the modal's bottom edge.
                let after = (item + 1).min(view.list.item_count().saturating_sub(1));
                view.list.scroll_to_reveal_item(after);
                view.list.scroll_to_reveal_item(item);
                cx.notify();
            })
            .ok();
        });
    }

    fn on_editor_event(
        &mut self,
        event: &TextInputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            TextInputEvent::Changed => self.check_snippet(cx),
            TextInputEvent::Submitted => self.save_snippet(window, cx),
            TextInputEvent::Cancelled => self.close_snippet_editor(window, cx),
            TextInputEvent::Blurred => {}
        }
    }

    /// Parses the snippet as written and tries it on the test text.
    fn check_snippet(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.snippet_editor.as_ref() else {
            return;
        };
        let (line, expansion_start) = editor_line(editor, cx);
        let test = editor.field(EditorField::Test).read(cx).text().to_string();
        let blank = is_blank(editor, cx);
        let parsed = parse_snippet(&line);
        let Some(editor) = self.snippet_editor.as_mut() else {
            return;
        };
        match parsed {
            // A new snippet with nothing written yet has nothing wrong.
            Err(_) if blank => {
                editor.problem = None;
                editor.result = None;
            }
            Ok(snippet) => {
                let (context, block) = test_context(&snippet);
                editor.problem = None;
                editor.result = SnippetEngine::new(vec![snippet])
                    .ok()
                    .map(|engine| preview(&engine, &test, context, block));
            }
            Err(error) => {
                let field = match error.column > expansion_start {
                    true => EditorField::Expansion,
                    false => EditorField::Trigger,
                };
                editor.problem = Some((field, capitalised(&error.message)));
                editor.result = None;
            }
        }
        cx.notify();
    }

    /// Sets where the snippet works. Choosing the place it already has
    /// keeps its exact scopes, such as inline math only.
    pub fn set_snippet_place(&mut self, place: Place, cx: &mut Context<Self>) {
        let Some(editor) = self.snippet_editor.as_mut() else {
            return;
        };
        if Place::of(&editor.options.scopes) != Some(place) {
            editor.options.scopes = place.scopes();
            self.check_snippet(cx);
        }
    }

    /// Sets when the snippet fires.
    pub fn set_snippet_fire(&mut self, fire: Fire, cx: &mut Context<Self>) {
        if let Some(editor) = self.snippet_editor.as_mut() {
            editor.options.fire = fire;
            self.check_snippet(cx);
        }
    }

    /// Flips the whole-word or after-a-space option.
    pub fn toggle_snippet_option(&mut self, control: OptionControl, cx: &mut Context<Self>) {
        let Some(editor) = self.snippet_editor.as_mut() else {
            return;
        };
        let options = &mut editor.options;
        match control {
            OptionControl::WholeWord => options.whole_word = !options.whole_word,
            OptionControl::AfterSpace => options.after_space = !options.after_space,
            OptionControl::Place | OptionControl::Fire => return,
        }
        self.check_snippet(cx);
    }

    /// Writes the snippet in the editor into the file and closes it.
    pub fn save_snippet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.snippet_editor.as_ref() else {
            return;
        };
        let (line, _) = editor_line(editor, cx);
        let blank = is_blank(editor, cx);
        let Ok(snippet) = parse_snippet(&line) else {
            self.check_snippet(cx);
            if blank && let Some(editor) = self.snippet_editor.as_mut() {
                let message = "Type the keys that start the snippet first.".to_string();
                editor.problem = Some((EditorField::Trigger, message));
            }
            return;
        };
        let mut file = self.typing_lists.snippets.clone();
        match self.editing_line() {
            Some(at) => file.lines[at] = FileLine::Snippet(snippet),
            None => add_snippet(&mut file, snippet),
        }
        if self.save_snippets(file, cx) {
            self.close_snippet_editor(window, cx);
        }
    }

    /// Removes the snippet in the editor from the file.
    pub fn delete_snippet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(line) = self.editing_line() else {
            self.close_snippet_editor(window, cx);
            return;
        };
        let mut file = self.typing_lists.snippets.clone();
        file.lines.remove(line);
        if self.save_snippets(file, cx) {
            self.close_snippet_editor(window, cx);
        }
    }

    pub fn close_snippet_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let line = self.editing_line();
        self.snippet_editor = None;
        self.invalidate_layouts();
        let back = self.layout().rows.iter().position(|row| match row {
            ControlRow::Snippet(row) => Some(row.line) == line,
            ControlRow::SnippetsFile => line.is_none(),
            _ => false,
        });
        match back {
            Some(index) => self.set_focus(SettingsFocus::Control(index), window, cx),
            None => window.focus(&self.focus_handle),
        }
        cx.notify();
    }

    /// Puts a tab stop at the cursor in the expansion field.
    fn insert_stop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.snippet_editor.as_ref() else {
            return;
        };
        let field = editor.field(EditorField::Expansion).clone();
        field.update(cx, |field, cx| {
            let range = field.selected_range();
            field.replace(range, &STOP_GLYPH.to_string(), cx);
        });
        window.focus(&field.focus_handle(cx));
        self.check_snippet(cx);
    }

    /// Keys inside the editor: Tab and Shift+Tab walk its fields and
    /// controls, and a focused control takes arrows, Space, Enter and
    /// Escape.
    pub(super) fn snippet_editor_key(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(editor) = self.snippet_editor.as_ref() else {
            return false;
        };
        let Some(at) = editor.focused_stop(window, cx) else {
            return false;
        };
        let modified = keystroke.modifiers.control || keystroke.modifiers.platform;
        if keystroke.key == "tab" && !modified {
            let step = if keystroke.modifiers.shift {
                STOPS.len() - 1
            } else {
                1
            };
            let next = editor.stop_handle(STOPS[(at + step) % STOPS.len()], cx);
            window.focus(&next);
            cx.notify();
            return true;
        }
        match STOPS[at] {
            EditorStop::Control(control) if !modified => {
                self.snippet_control_key(control, &keystroke.key, window, cx)
            }
            _ => false,
        }
    }

    fn snippet_control_key(
        &mut self,
        control: OptionControl,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match (control, key) {
            (_, "escape") => self.close_snippet_editor(window, cx),
            (OptionControl::Place | OptionControl::Fire, "enter") => self.save_snippet(window, cx),
            (OptionControl::Place | OptionControl::Fire, "left") => {
                self.step_snippet_choice(control, -1, cx)
            }
            (OptionControl::Place | OptionControl::Fire, "right") => {
                self.step_snippet_choice(control, 1, cx)
            }
            (OptionControl::Place | OptionControl::Fire, "space") => {
                self.step_snippet_choice(control, 0, cx)
            }
            (OptionControl::WholeWord | OptionControl::AfterSpace, "space" | "enter") => {
                self.toggle_snippet_option(control, cx)
            }
            _ => return false,
        }
        true
    }

    /// Moves a choice `step` places, stopping at the ends; a step of zero
    /// is Space's, which moves on one and wraps round.
    fn step_snippet_choice(&mut self, control: OptionControl, step: isize, cx: &mut Context<Self>) {
        let Some(editor) = self.snippet_editor.as_ref() else {
            return;
        };
        let options = &editor.options;
        let (at, count) = match control {
            OptionControl::Place => {
                let place = Place::of(&options.scopes);
                let at = Place::ALL.iter().position(|(p, _)| Some(*p) == place);
                (at, Place::ALL.len())
            }
            _ => (
                FIRES.iter().position(|(f, _)| *f == options.fire),
                FIRES.len(),
            ),
        };
        let next = match (at, step) {
            (None, _) => 0,
            (Some(at), 0) => (at + 1) % count,
            (Some(at), step) => at.saturating_add_signed(step).min(count - 1),
        };
        match control {
            OptionControl::Place => self.set_snippet_place(Place::ALL[next].0, cx),
            _ => self.set_snippet_fire(FIRES[next].0, cx),
        }
    }

    // ---- Drawing ----

    /// The editor, drawn in place of a row: the two parts of a snippet,
    /// where and when it fires, a box to try it in, and the buttons that
    /// save or drop it.
    pub(super) fn render_snippet_editor(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(editor) = self.snippet_editor.as_ref() else {
            return div().into_any_element();
        };
        let style = &self.style;
        let stop = button("insert-stop", "Add a tab stop ●", false, false, style)
            .debug_selector(|| "insert-stop".to_string())
            .on_click(cx.listener(|view, _: &ClickEvent, window, cx| view.insert_stop(window, cx)));
        // A new snippet has no row above it to say what's open.
        let title = editor
            .line
            .is_none()
            .then(|| div().font_weight(style.strong_weight).child("New snippet"));
        div()
            .debug_selector(|| "snippet-editor".to_string())
            .w_full()
            .flex()
            .flex_col()
            .gap(style.control_gap)
            .children(title)
            .child(self.labelled_field(
                editor,
                EditorField::Trigger,
                "What you type",
                None,
                window,
                cx,
            ))
            .child(self.labelled_field(
                editor,
                EditorField::Expansion,
                "What you get. ● marks where the cursor stops, ␣ a space and ⏎ a new line.",
                Some(stop.into_any_element()),
                window,
                cx,
            ))
            .child(self.render_option_controls(editor, window, cx))
            .child(self.labelled_field(editor, EditorField::Test, "Try it", None, window, cx))
            .child(self.render_test_result(editor))
            .child(self.render_editor_buttons(editor, cx))
            .into_any_element()
    }

    fn labelled_field(
        &self,
        editor: &SnippetEditor,
        field: EditorField,
        label: &str,
        extra: Option<AnyElement>,
        window: &Window,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let style = &self.style;
        let input = editor.field(field);
        let focused = input.focus_handle(cx).is_focused(window);
        let note = editor
            .problem
            .as_ref()
            .filter(|(at, _)| *at == field)
            .map(|(_, message)| control_note(message.clone(), style));
        labelled(
            label,
            div()
                .relative()
                .flex()
                .items_center()
                .gap(style.control_gap)
                .child(field_box(input.clone(), None, focused, style).flex_1())
                .children(extra)
                .children(note),
            style,
        )
    }

    /// Where and when the snippet fires, then its two switches.
    fn render_option_controls(
        &self,
        editor: &SnippetEditor,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let style = &self.style;
        let choices = div()
            .flex()
            .flex_wrap()
            .gap(style.row_gap)
            .child(labelled(
                "Where it works",
                self.place_control(editor, window, cx),
                style,
            ))
            .child(labelled(
                "When it fires",
                self.fire_control(editor, window, cx),
                style,
            ));
        let switches = div()
            .flex()
            .flex_wrap()
            .gap(style.row_gap)
            .child(self.option_switch(
                editor,
                OptionControl::WholeWord,
                "Whole word only",
                window,
                cx,
            ))
            .child(self.option_switch(
                editor,
                OptionControl::AfterSpace,
                "After a space",
                window,
                cx,
            ));
        div()
            .flex()
            .flex_col()
            .gap(style.control_gap)
            .child(choices)
            .child(switches)
    }

    fn rings_control(
        &self,
        editor: &SnippetEditor,
        control: OptionControl,
        window: &Window,
        cx: &Context<Self>,
    ) -> bool {
        crate::ui::focus_visible::ring(editor.control(control).is_focused(window), cx)
    }

    fn place_control(
        &self,
        editor: &SnippetEditor,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Stateful<gpui::Div> {
        let style = &self.style;
        let chosen = Place::of(&editor.options.scopes);
        let segments = Place::ALL.iter().map(|(place, label)| {
            let place = *place;
            segment(
                SharedString::from(format!("snippet-place-{label}")),
                *label,
                chosen == Some(place),
                style,
            )
            .on_click(
                cx.listener(move |view, _: &ClickEvent, _, cx| view.set_snippet_place(place, cx)),
            )
        });
        let focused = self.rings_control(editor, OptionControl::Place, window, cx);
        segmented(
            "snippet-place",
            segments.collect::<Vec<_>>(),
            focused,
            style,
        )
        .track_focus(editor.control(OptionControl::Place))
        .debug_selector(|| "snippet-place".to_string())
    }

    fn fire_control(
        &self,
        editor: &SnippetEditor,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Stateful<gpui::Div> {
        let style = &self.style;
        let segments = FIRES.iter().map(|(fire, label)| {
            let fire = *fire;
            segment(
                SharedString::from(format!("snippet-fire-{label}")),
                *label,
                editor.options.fire == fire,
                style,
            )
            .on_click(
                cx.listener(move |view, _: &ClickEvent, _, cx| view.set_snippet_fire(fire, cx)),
            )
        });
        let focused = self.rings_control(editor, OptionControl::Fire, window, cx);
        segmented("snippet-fire", segments.collect::<Vec<_>>(), focused, style)
            .track_focus(editor.control(OptionControl::Fire))
            .debug_selector(|| "snippet-fire".to_string())
    }

    /// A switch with its label beside it; clicking either flips it.
    fn option_switch(
        &self,
        editor: &SnippetEditor,
        control: OptionControl,
        label: &'static str,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let style = &self.style;
        let on = match control {
            OptionControl::WholeWord => editor.options.whole_word,
            _ => editor.options.after_space,
        };
        let focused = self.rings_control(editor, control, window, cx);
        let id = SharedString::from(format!("snippet-{control:?}"));
        div()
            .id(id.clone())
            .track_focus(editor.control(control))
            .debug_selector(move || id.to_string())
            .flex()
            .items_center()
            .gap(style.control_gap)
            .cursor_pointer()
            .child(toggle_switch(
                SharedString::from(format!("snippet-switch-{control:?}")),
                on,
                focused,
                style,
            ))
            .child(label)
            // Space and Enter are handled as they're pressed, so the click
            // GPUI makes of their release mustn't flip it back.
            .on_click(cx.listener(move |view, event: &ClickEvent, _, cx| {
                if !event.is_keyboard() {
                    view.toggle_snippet_option(control, cx)
                }
            }))
    }

    /// What typing the test text gives, with the cursor where it ends up.
    fn render_test_result(&self, editor: &SnippetEditor) -> AnyElement {
        let style = &self.style;
        let caption = match &editor.result {
            _ if editor.problem.is_some() => "Fix the snippet to try it.",
            None => "Type in the box above to see what it becomes.",
            Some(result) if result.expansions == 0 => "It doesn’t fire on this text.",
            Some(result) if result.tab => "With Tab pressed after it, that gives",
            Some(_) => "That gives",
        };
        let shown = editor
            .result
            .as_ref()
            .filter(|result| result.expansions > 0)
            .map(|result| {
                let (before, after) = result.text.split_at(result.caret);
                div()
                    .debug_selector(|| "snippet-test-result".to_string())
                    .flex()
                    .items_center()
                    .font_family(style.code_font_family.clone())
                    .child(before.to_string())
                    .child(
                        div()
                            .w(style.hairline * 2.)
                            .h(style.small_icon_size)
                            .bg(style.text),
                    )
                    .child(after.to_string())
            });
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(style.control_gap)
            .child(
                div()
                    .text_size(style.small_text_size)
                    .text_color(style.text_muted)
                    .child(caption),
            )
            .children(shown)
            .into_any_element()
    }

    fn render_editor_buttons(&self, editor: &SnippetEditor, cx: &mut Context<Self>) -> AnyElement {
        let style = &self.style;
        let delete = editor.line.map(|_| {
            button("delete-snippet", "Delete snippet", false, false, style)
                .debug_selector(|| "delete-snippet".to_string())
                .on_click(
                    cx.listener(|view, _: &ClickEvent, window, cx| view.delete_snippet(window, cx)),
                )
        });
        let cancel = button("cancel-snippet", "Cancel", false, false, style)
            .debug_selector(|| "cancel-snippet".to_string())
            .on_click(cx.listener(|view, _: &ClickEvent, window, cx| {
                view.close_snippet_editor(window, cx)
            }));
        let save = button("save-snippet", "Save", true, false, style)
            .debug_selector(|| "save-snippet".to_string())
            .on_click(
                cx.listener(|view, _: &ClickEvent, window, cx| view.save_snippet(window, cx)),
            );
        let error = self
            .error
            .as_ref()
            .filter(|(key, _)| key == SNIPPETS_KEY)
            .map(|(_, message)| control_note(message.clone(), style));
        div()
            .relative()
            .flex()
            .items_center()
            .gap(style.control_gap)
            .children(delete)
            .child(div().flex_1())
            .child(cancel)
            .child(save)
            .children(error)
            .into_any_element()
    }
}

/// The snippet as one line of the file, and where its expansion starts
/// in it, in characters, to point an error at the right field.
fn editor_line(editor: &SnippetEditor, cx: &gpui::App) -> (String, usize) {
    let text = |field: EditorField| editor.field(field).read(cx).text().to_string();
    let trigger = text(EditorField::Trigger);
    let expansion = text(EditorField::Expansion);
    let options = format_options(&editor.options);
    let expansion_start = trigger.chars().count() + 3;
    (
        format!("{trigger} → {expansion}  {options}"),
        expansion_start,
    )
}

/// Whether nothing has been written in the snippet's two fields.
fn is_blank(editor: &SnippetEditor, cx: &gpui::App) -> bool {
    [EditorField::Trigger, EditorField::Expansion]
        .iter()
        .all(|field| editor.field(*field).read(cx).text().trim().is_empty())
}

/// A small muted label above a control.
fn labelled(
    label: &str,
    control: impl IntoElement,
    style: &crate::theme::SettingsTheme,
) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap(style.gap_sm)
        .child(
            div()
                .text_size(style.small_text_size)
                .text_color(style.text_muted)
                .child(label.to_string()),
        )
        .child(control)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_read_from_scopes() {
        assert_eq!(Place::of(&[]), Some(Place::Anywhere));
        let text = [Scope::Context(InputContext::Text)];
        assert_eq!(Place::of(&text), Some(Place::Text));
        assert_eq!(Place::of(&[Scope::InlineMath]), Some(Place::Math));
        let code = [Scope::Context(InputContext::Code)];
        assert_eq!(Place::of(&code), None);
        let mixed = [Scope::Context(InputContext::Text), Scope::InlineMath];
        assert_eq!(Place::of(&mixed), None);
    }

    #[test]
    fn a_new_snippet_is_for_math_as_you_type() {
        assert_eq!(format_options(&new_snippet_options()), "math, instant");
    }
}
