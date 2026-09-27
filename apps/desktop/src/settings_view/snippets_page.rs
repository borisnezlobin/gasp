//! The Snippets and replacements page: every snippet in the vault's
//! `.editor/snippets.txt` (or the built-in list) and every replacement in
//! `.editor/replacements.toml`, grouped as their files group them, each
//! with a switch. A snippet opens in place into an editor with a test box
//! that shows what typing gives as you type. Changes are written straight
//! away and reported as `snippets` or `replacements`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use editor_config::loader::CONFIG_DIR;
use editor_snippets::{
    FileLine, InputContext, Preview, ReplacementFire, Replacements, STOP_GLYPH, Scope, Snippet,
    SnippetEngine, SnippetFile, format_expansion, format_options, format_trigger, parse_snippet,
    preview,
};
use gpui::{
    AnyElement, AppContext, ClickEvent, Context, Entity, Focusable, Keystroke, SharedString,
    Subscription, Window, div, prelude::*,
};

use super::controls::{button, control_note, field_box, toggle_switch};
use super::model::words_match;
use super::store;
use super::view::{ControlRow, PaneLayout, SettingsEvent, SettingsFocus, SettingsView};
use crate::text_input::{TextInput, TextInputEvent, TextInputStyle};

pub const SNIPPETS_FILE: &str = "snippets.txt";
pub const REPLACEMENTS_FILE: &str = "replacements.toml";

/// The key a snippet change is reported and its errors kept under.
pub const SNIPPETS_KEY: &str = "snippets";
/// The key a replacement change is reported and its errors kept under.
pub const REPLACEMENTS_KEY: &str = "replacements";

/// The group snippets before any heading comment fall in.
const UNGROUPED: &str = "Snippets";

/// The heading new snippets go under.
const ADDED_GROUP: &str = "Added in settings";

fn snippets_path(vault_root: &Path) -> PathBuf {
    vault_root.join(CONFIG_DIR).join(SNIPPETS_FILE)
}

fn replacements_path(vault_root: &Path) -> PathBuf {
    vault_root.join(CONFIG_DIR).join(REPLACEMENTS_FILE)
}

/// The snippets and replacements the page lists.
#[derive(Clone, Debug, Default)]
pub struct TypingLists {
    pub snippets: SnippetFile,
    pub snippets_from_vault: bool,
    pub replacements: Replacements,
    pub replacements_from_vault: bool,
    /// Why a vault file couldn't be read. The page says so and doesn't
    /// write over the file.
    pub problem: Option<String>,
}

impl TypingLists {
    /// Reads the vault's files, or takes the built-in lists where there
    /// are none.
    pub fn load(vault_root: &Path) -> TypingLists {
        let mut lists = TypingLists {
            snippets: SnippetFile::builtin(),
            replacements: Replacements::builtin(),
            ..TypingLists::default()
        };
        match read_optional(&snippets_path(vault_root)) {
            Ok(Some(text)) => match SnippetFile::parse(&text) {
                Ok(file) => {
                    lists.snippets = file;
                    lists.snippets_from_vault = true;
                }
                Err(errors) => lists.problem = Some(format!("snippets.txt, {}", errors[0])),
            },
            Ok(None) => {}
            Err(error) => lists.problem = Some(error.to_string()),
        }
        match read_optional(&replacements_path(vault_root)) {
            Ok(Some(text)) => match Replacements::from_toml(&text) {
                Ok(table) => {
                    lists.replacements = table;
                    lists.replacements_from_vault = true;
                }
                Err(error) => lists.problem = Some(format!("replacements.toml: {error}")),
            },
            Ok(None) => {}
            Err(error) => lists.problem = Some(error.to_string()),
        }
        lists
    }
}

fn read_optional(path: &Path) -> io::Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// One snippet as its row shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct SnippetRow {
    /// The snippet's line in the file.
    pub line: usize,
    pub trigger: String,
    pub expansion: String,
    /// Where and when it fires, in words.
    pub when: String,
    pub on: bool,
}

impl SnippetRow {
    fn new(line: usize, snippet: &Snippet) -> SnippetRow {
        SnippetRow {
            line,
            trigger: format_trigger(&snippet.trigger),
            expansion: format_expansion(snippet),
            when: describe(snippet),
            on: !snippet.options.off,
        }
    }
}

/// One replacement as its row shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct ReplacementRow {
    pub index: usize,
    pub from: String,
    pub to: String,
    pub when: String,
    pub on: bool,
}

/// Where and when a snippet fires, as a sentence.
fn describe(snippet: &Snippet) -> String {
    let options = &snippet.options;
    let place = match options.scopes.as_slice() {
        [] => "Anywhere".to_string(),
        scopes => {
            let names: Vec<String> = scopes.iter().map(|scope| scope_name(*scope)).collect();
            format!("In {}", names.join(" or "))
        }
    };
    let when = match options.fire {
        editor_snippets::Fire::Instant => "as you type",
        editor_snippets::Fire::OnTab => "on Tab",
    };
    let mut extras = Vec::new();
    if options.whole_word {
        extras.push("as a whole word");
    }
    if options.after_space {
        extras.push("after a space");
    }
    if options.on_selection {
        extras.push("around a selection");
    }
    let extras = if extras.is_empty() {
        String::new()
    } else {
        format!(", {}", extras.join(", "))
    };
    format!("{place}, {when}{extras}.")
}

fn scope_name(scope: Scope) -> String {
    match scope {
        Scope::Context(context) => context.name().to_string(),
        Scope::InlineMath => "inline math".to_string(),
        Scope::BlockMath => "block math".to_string(),
    }
}

/// Where a snippet is tried in the test box: its first place.
fn test_context(snippet: &Snippet) -> (InputContext, bool) {
    match snippet.options.scopes.first() {
        Some(Scope::Context(context)) => (*context, false),
        Some(Scope::InlineMath) => (InputContext::Math, false),
        Some(Scope::BlockMath) => (InputContext::Math, true),
        None => (InputContext::Text, false),
    }
}

/// The snippets grouped by the heading comments above them. A comment
/// starts a group when a blank line or the start of the file comes before
/// it; other comments are notes inside a group.
fn snippet_groups(file: &SnippetFile) -> Vec<(String, Vec<SnippetRow>)> {
    let mut groups: Vec<(String, Vec<SnippetRow>)> = Vec::new();
    let mut after_blank = true;
    for (line, entry) in file.lines.iter().enumerate() {
        match entry {
            FileLine::Comment(text) if after_blank => groups.push((text.clone(), Vec::new())),
            FileLine::Snippet(snippet) => {
                if groups.is_empty() {
                    groups.push((UNGROUPED.to_string(), Vec::new()));
                }
                let rows = &mut groups.last_mut().expect("a group was just made").1;
                rows.push(SnippetRow::new(line, snippet));
            }
            _ => {}
        }
        after_blank = matches!(entry, FileLine::Blank);
    }
    groups.retain(|(_, rows)| !rows.is_empty());
    groups
}

/// The replacements by group, in file order, leaving out curly quotes:
/// the smart quotes setting on the Editor page owns those.
fn replacement_groups(table: &Replacements) -> Vec<(String, Vec<ReplacementRow>)> {
    let mut groups: Vec<(String, Vec<ReplacementRow>)> = Vec::new();
    for (index, entry) in table.entries.iter().enumerate() {
        if entry.closing.is_some() {
            continue;
        }
        let group = capitalised(if entry.group.is_empty() {
            "Other"
        } else {
            &entry.group
        });
        let row = ReplacementRow {
            index,
            from: entry.from.clone(),
            to: entry.to.clone(),
            when: replacement_when(entry),
            on: entry.enabled,
        };
        match groups.iter_mut().find(|(name, _)| *name == group) {
            Some((_, rows)) => rows.push(row),
            None => groups.push((group, vec![row])),
        }
    }
    groups
}

fn replacement_when(entry: &editor_snippets::Replacement) -> String {
    let when = match entry.fire {
        ReplacementFire::Instant => "as you type",
        ReplacementFire::AfterSpace => "after a space",
    };
    match &entry.contexts {
        Some(contexts) => {
            let names: Vec<&str> = contexts.iter().map(|c| c.name()).collect();
            format!("In {}, {when}.", names.join(" or "))
        }
        None => format!("Outside code and math, {when}."),
    }
}

fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// A field of the snippet editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorField {
    Trigger,
    Expansion,
    Options,
    Test,
}

impl EditorField {
    const ALL: [EditorField; 4] = [
        EditorField::Trigger,
        EditorField::Expansion,
        EditorField::Options,
        EditorField::Test,
    ];
}

/// The snippet open in the editor.
pub struct SnippetEditor {
    /// The snippet's line in the file, or `None` for a new one.
    pub line: Option<usize>,
    fields: [Entity<TextInput>; 4],
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
}

impl SettingsView {
    /// The snippet open in the editor as one line of the file, what's
    /// wrong with it, and what typing the test text gives.
    pub fn snippet_editor_state(
        &self,
        cx: &gpui::App,
    ) -> Option<(String, Option<String>, Option<Preview>)> {
        let editor = self.snippet_editor.as_ref()?;
        let text = |field: EditorField| editor.field(field).read(cx).text().to_string();
        let line = format!(
            "{} → {}  {}",
            text(EditorField::Trigger),
            text(EditorField::Expansion),
            text(EditorField::Options)
        );
        let problem = editor.problem.as_ref().map(|(_, message)| message.clone());
        Some((line, problem, editor.result.clone()))
    }

    /// The editor's field, for tests and for focusing it.
    pub fn snippet_field(&self, field: EditorField) -> Option<Entity<TextInput>> {
        self.snippet_editor
            .as_ref()
            .map(|editor| editor.field(field).clone())
    }

    // ---- Rows ----

    /// The page's rows after its setting cards: the snippets and then the
    /// replacements, one card per group.
    pub(super) fn snippet_cards(&self, query: &str, layout: &mut PaneLayout) {
        let lists = &self.typing_lists;
        if query.is_empty() || words_match("add snippet new", query) {
            layout.push_card(None, vec![ControlRow::SnippetsFile]);
        }
        if self
            .snippet_editor
            .as_ref()
            .is_some_and(|e| e.line.is_none())
        {
            layout.push_card(None, vec![ControlRow::SnippetEditor]);
        }
        for (group, rows) in snippet_groups(&lists.snippets) {
            let mut shown = Vec::new();
            for row in rows {
                let haystack = format!("{} {} {} {group}", row.trigger, row.expansion, row.when);
                if !words_match(&haystack, query) {
                    continue;
                }
                let editing = self.editing_line() == Some(row.line);
                shown.push(ControlRow::Snippet(row));
                if editing {
                    shown.push(ControlRow::SnippetEditor);
                }
            }
            layout.push_card(Some(group), shown);
        }
        for (group, rows) in replacement_groups(&lists.replacements) {
            let shown = rows
                .into_iter()
                .filter(|row| {
                    let haystack = format!("{} {} {} {group}", row.from, row.to, row.when);
                    words_match(&haystack, query)
                })
                .map(ControlRow::Replacement)
                .collect();
            layout.push_card(Some(group), shown);
        }
    }

    fn editing_line(&self) -> Option<usize> {
        self.snippet_editor.as_ref().and_then(|editor| editor.line)
    }

    /// What the row at the top of the snippets says about their file.
    pub(super) fn snippets_file_description(&self) -> String {
        if let Some(problem) = &self.typing_lists.problem {
            return format!("The file can't be read, so nothing here changes it: {problem}");
        }
        if self.typing_lists.snippets_from_vault {
            "Kept in .editor/snippets.txt, so they sync with your notes.".to_string()
        } else {
            "These are the built-in snippets. Your first change saves a copy to .editor/snippets.txt.".to_string()
        }
    }

    /// The title and description of a snippet or replacement row, with
    /// the typed text in the code font.
    pub(super) fn typing_row_text(&self, row: &ControlRow) -> Option<AnyElement> {
        let style = &self.style;
        let (typed, becomes, when) = match row {
            ControlRow::Snippet(row) => (&row.trigger, &row.expansion, &row.when),
            ControlRow::Replacement(row) => (&row.from, &row.to, &row.when),
            _ => return None,
        };
        let code = |text: &str| {
            div()
                .font_family(style.code_font_family.clone())
                .child(text.to_string())
        };
        let becomes = div()
            .flex()
            .flex_wrap()
            .items_baseline()
            .gap(style.gap_sm)
            .text_size(style.small_text_size)
            .text_color(style.text_muted)
            .child(code(becomes).text_color(style.text))
            .child(when.clone());
        Some(
            div()
                .flex()
                .flex_col()
                .gap(style.text_gap)
                .child(code(typed))
                .child(becomes)
                .into_any_element(),
        )
    }

    /// The control on a snippet or replacement row.
    pub(super) fn typing_row_control(
        &self,
        row: &ControlRow,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let style = &self.style;
        let control = match row {
            ControlRow::SnippetsFile => button("add-snippet", "Add snippet", false, focused, style)
                .debug_selector(|| "add-snippet".to_string())
                .on_click(cx.listener(|view, _: &ClickEvent, window, cx| {
                    view.open_snippet_editor(None, window, cx)
                })),
            ControlRow::Snippet(row) => {
                let line = row.line;
                let edit = button(
                    SharedString::from(format!("edit-snippet-{line}")),
                    "Edit",
                    false,
                    false,
                    style,
                )
                .debug_selector(move || format!("edit-snippet-{line}"))
                .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                    view.open_snippet_editor(Some(line), window, cx)
                }));
                let switch = toggle_switch(
                    SharedString::from(format!("toggle-snippet-{line}")),
                    row.on,
                    focused,
                    style,
                )
                .debug_selector(move || format!("toggle-snippet-{line}"))
                .on_click(
                    cx.listener(move |view, _: &ClickEvent, _, cx| view.toggle_snippet(line, cx)),
                );
                return Some(
                    div()
                        .flex()
                        .items_center()
                        .gap(style.control_gap)
                        .child(edit)
                        .child(switch)
                        .into_any_element(),
                );
            }
            ControlRow::Replacement(row) => {
                let index = row.index;
                toggle_switch(
                    SharedString::from(format!("toggle-replacement-{index}")),
                    row.on,
                    focused,
                    style,
                )
                .debug_selector(move || format!("toggle-replacement-{index}"))
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    view.toggle_replacement(index, cx)
                }))
            }
            _ => return None,
        };
        Some(control.into_any_element())
    }

    // ---- Changes ----

    pub(super) fn toggle_snippet(&mut self, line: usize, cx: &mut Context<Self>) {
        let mut file = self.typing_lists.snippets.clone();
        if let Some(FileLine::Snippet(snippet)) = file.lines.get_mut(line) {
            snippet.options.off = !snippet.options.off;
            self.save_snippets(file, cx);
        }
    }

    pub(super) fn toggle_replacement(&mut self, index: usize, cx: &mut Context<Self>) {
        let mut table = self.typing_lists.replacements.clone();
        if let Some(entry) = table.entries.get_mut(index) {
            entry.enabled = !entry.enabled;
            self.save_replacements(table, cx);
        }
    }

    fn can_write(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
        if let Some(problem) = &self.typing_lists.problem {
            self.error = Some((key.to_string(), format!("Fix the file first: {problem}")));
            cx.notify();
            return false;
        }
        true
    }

    fn save_snippets(&mut self, file: SnippetFile, cx: &mut Context<Self>) -> bool {
        if !self.can_write(SNIPPETS_KEY, cx) {
            return false;
        }
        let written = store::save(&snippets_path(&self.vault_root), &file.to_string());
        self.finish_write(SNIPPETS_KEY, written, cx, |lists| {
            lists.snippets = file;
            lists.snippets_from_vault = true;
        })
    }

    fn save_replacements(&mut self, table: Replacements, cx: &mut Context<Self>) -> bool {
        if !self.can_write(REPLACEMENTS_KEY, cx) {
            return false;
        }
        let text = format!(
            "# Replacements for typing, which the settings screen keeps.\n\n{}",
            table.to_toml()
        );
        let written = store::save(&replacements_path(&self.vault_root), &text);
        self.finish_write(REPLACEMENTS_KEY, written, cx, |lists| {
            lists.replacements = table;
            lists.replacements_from_vault = true;
        })
    }

    fn finish_write(
        &mut self,
        key: &str,
        written: io::Result<()>,
        cx: &mut Context<Self>,
        take: impl FnOnce(&mut TypingLists),
    ) -> bool {
        let saved = match written {
            Ok(()) => {
                take(&mut self.typing_lists);
                self.error = None;
                self.invalidate_layouts();
                cx.emit(SettingsEvent::Changed(key.to_string()));
                true
            }
            Err(error) => {
                self.error = Some((key.to_string(), error.to_string()));
                false
            }
        };
        cx.notify();
        saved
    }

    // ---- The editor ----

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
        let texts = match &snippet {
            Some(snippet) => [
                format_trigger(&snippet.trigger),
                format_expansion(snippet),
                format_options(&snippet.options),
                snippet.trigger.literal().unwrap_or_default().to_string(),
            ],
            None => [
                String::new(),
                String::new(),
                "math, instant".to_string(),
                String::new(),
            ],
        };
        let placeholders = ["mk", "\\frac{●}{●}●", "math, instant", "Type to try it"];
        let code_font = self.style.code_font_family.clone();
        let fields: [Entity<TextInput>; 4] = std::array::from_fn(|at| {
            let font = (at != EditorField::Options as usize).then(|| code_font.clone());
            let field = cx.new(|cx| {
                let input = TextInput::new(window, cx)
                    .with_placeholder(placeholders[at])
                    .with_style(TextInputStyle::Query);
                match &font {
                    Some(font) => input.with_font_family(font.clone()),
                    None => input,
                }
            });
            field.update(cx, |field, cx| field.set_text(&texts[at], cx));
            field
        });
        let subscriptions = EditorField::ALL
            .iter()
            .map(|which| {
                let which = *which;
                cx.subscribe_in(
                    &fields[which as usize],
                    window,
                    move |view, _, event: &TextInputEvent, window, cx| {
                        view.on_editor_event(which, event, window, cx)
                    },
                )
            })
            .collect();
        window.focus(&fields[0].focus_handle(cx));
        self.snippet_editor = Some(SnippetEditor {
            line,
            fields,
            problem: None,
            result: None,
            _subscriptions: subscriptions,
        });
        self.error = None;
        self.check_snippet(cx);
        self.invalidate_layouts();
        if let Some(index) = self
            .layout()
            .rows
            .iter()
            .position(|row| *row == ControlRow::SnippetEditor)
        {
            self.focus = SettingsFocus::Control(index);
            self.reveal_row(index, cx);
        }
        cx.notify();
    }

    fn on_editor_event(
        &mut self,
        _which: EditorField,
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

    /// The editor's fields as one snippet line.
    fn editor_line(&self, cx: &Context<Self>) -> Option<(String, [usize; 3])> {
        let editor = self.snippet_editor.as_ref()?;
        let text = |field: EditorField| editor.field(field).read(cx).text().to_string();
        let trigger = text(EditorField::Trigger);
        let expansion = text(EditorField::Expansion);
        let options = text(EditorField::Options);
        // Where each field starts in the line, in characters, to point an
        // error at the right one.
        let starts = [
            0,
            trigger.chars().count() + 3,
            trigger.chars().count() + expansion.chars().count() + 5,
        ];
        Some((format!("{trigger} → {expansion}  {options}"), starts))
    }

    /// Parses the snippet as written and tries it on the test text.
    fn check_snippet(&mut self, cx: &mut Context<Self>) {
        let Some((line, starts)) = self.editor_line(cx) else {
            return;
        };
        let parsed = parse_snippet(&line);
        let test = self
            .snippet_editor
            .as_ref()
            .map(|editor| editor.field(EditorField::Test).read(cx).text().to_string())
            .unwrap_or_default();
        let Some(editor) = self.snippet_editor.as_mut() else {
            return;
        };
        match parsed {
            Ok(snippet) => {
                let (context, block) = test_context(&snippet);
                editor.problem = None;
                editor.result = SnippetEngine::new(vec![snippet])
                    .ok()
                    .map(|engine| preview(&engine, &test, context, block));
            }
            Err(error) => {
                let field = match starts.iter().rposition(|start| error.column > *start) {
                    Some(0) | None => EditorField::Trigger,
                    Some(1) => EditorField::Expansion,
                    Some(_) => EditorField::Options,
                };
                editor.problem = Some((field, capitalised(&error.message)));
                editor.result = None;
            }
        }
        cx.notify();
    }

    /// Writes the snippet in the editor into the file and closes it.
    pub fn save_snippet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((line, _)) = self.editor_line(cx) else {
            return;
        };
        let Ok(snippet) = parse_snippet(&line) else {
            self.check_snippet(cx);
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

    /// Tab and Shift+Tab move between the editor's fields.
    pub(super) fn snippet_editor_key(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(editor) = self.snippet_editor.as_ref() else {
            return false;
        };
        let focused = EditorField::ALL
            .iter()
            .position(|field| editor.field(*field).focus_handle(cx).is_focused(window));
        let Some(at) = focused else {
            return false;
        };
        if keystroke.key != "tab" || keystroke.modifiers.control || keystroke.modifiers.platform {
            return false;
        }
        let step = if keystroke.modifiers.shift { 3 } else { 1 };
        let next = EditorField::ALL[(at + step) % EditorField::ALL.len()];
        window.focus(&editor.field(next).focus_handle(cx));
        true
    }

    /// The editor, drawn in place of a row: the three parts of a snippet,
    /// a box to try it in, and the buttons that save or drop it.
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
        let labelled = |field: EditorField, label: &str, extra: Option<AnyElement>| {
            let input = editor.field(field);
            let focused = input.focus_handle(cx).is_focused(window);
            let note = editor
                .problem
                .as_ref()
                .filter(|(at, _)| *at == field)
                .map(|(_, message)| control_note(message.clone(), style));
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
                .child(
                    div()
                        .relative()
                        .flex()
                        .items_center()
                        .gap(style.control_gap)
                        .child(field_box(input.clone(), None, focused, style).flex_1())
                        .children(extra)
                        .children(note),
                )
        };
        div()
            .debug_selector(|| "snippet-editor".to_string())
            .w_full()
            .flex()
            .flex_col()
            .gap(style.control_gap)
            .child(labelled(EditorField::Trigger, "What you type", None))
            .child(labelled(
                EditorField::Expansion,
                "What it becomes. ● marks a tab stop, ␣ a space and ⏎ a new line.",
                Some(stop.into_any_element()),
            ))
            .child(labelled(
                EditorField::Options,
                "Where and when: anywhere, text or math; instant or on tab; whole word, after space, off.",
                None,
            ))
            .child(labelled(EditorField::Test, "Try it", None))
            .child(self.render_test_result(editor))
            .child(self.render_editor_buttons(editor, cx))
            .into_any_element()
    }

    /// What typing the test text gives, with the cursor where it ends up.
    fn render_test_result(&self, editor: &SnippetEditor) -> AnyElement {
        let style = &self.style;
        let caption = match &editor.result {
            _ if editor.problem.is_some() => "Fix the snippet to try it.",
            None => "Type in the box above to see what it becomes.",
            Some(result) if result.expansions == 0 => "It doesn't fire on this text.",
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

/// Adds `snippet` at the end of the group new snippets go in, starting
/// the group when there isn't one yet.
fn add_snippet(file: &mut SnippetFile, snippet: Snippet) {
    let heading = FileLine::Comment(ADDED_GROUP.to_string());
    if let Some(at) = file.lines.iter().position(|line| *line == heading) {
        let end = file.lines[at + 1..]
            .iter()
            .position(|line| !matches!(line, FileLine::Snippet(_)))
            .map_or(file.lines.len(), |offset| at + 1 + offset);
        file.lines.insert(end, FileLine::Snippet(snippet));
        return;
    }
    if !matches!(file.lines.last(), None | Some(FileLine::Blank)) {
        file.push_blank();
    }
    file.lines.push(heading);
    file.push_snippet(snippet);
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "\
# Migrated from Latex Suite.
# A note under the heading.

# Math mode
mk → $●$  text, instant
# Auto letter subscript
{letter}{digit} → {letter}_{{digit}}  math, instant, off

# Empty group

# Greek
@a → \\alpha  math, instant
";

    #[test]
    fn headings_follow_blank_lines_and_empty_groups_drop() {
        let file = SnippetFile::parse(FILE).unwrap();
        let groups = snippet_groups(&file);
        let names: Vec<&str> = groups.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["Math mode", "Greek"]);
        assert_eq!(groups[0].1.len(), 2);
        assert!(!groups[0].1[1].on);
        assert_eq!(groups[0].1[0].when, "In text, as you type.");
    }

    #[test]
    fn new_snippets_gather_under_one_heading() {
        let mut file = SnippetFile::parse(FILE).unwrap();
        add_snippet(&mut file, parse_snippet("zz → z  anywhere").unwrap());
        add_snippet(&mut file, parse_snippet("yy → y  anywhere").unwrap());
        let tail: Vec<String> = file.lines[file.lines.len() - 4..]
            .iter()
            .map(|line| match line {
                FileLine::Snippet(snippet) => format_trigger(&snippet.trigger),
                FileLine::Comment(text) => format!("# {text}"),
                FileLine::Blank => String::new(),
            })
            .collect();
        assert_eq!(tail, ["", "# Added in settings", "zz", "yy"]);
    }

    #[test]
    fn quotes_stay_off_the_replacements_list() {
        let mut quote = editor_snippets::Replacement::new("\"", "“", "Quotes");
        quote.closing = Some("”".to_string());
        let table = Replacements::new(vec![
            quote,
            editor_snippets::Replacement::new("--", "—", "dashes"),
        ]);
        let groups = replacement_groups(&table);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].0, "Dashes");
        assert_eq!(groups[0].1[0].when, "Outside code and math, as you type.");
    }
}
