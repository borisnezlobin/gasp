//! Snippets, replacements and the math helpers, typed through the editor
//! with the owner's Latex Suite and Smart Typography settings as the
//! migrator converts them from `reference/obsidian`.

use std::path::{Path, PathBuf};

use std::cell::RefCell;
use std::fs;
use std::rc::Rc;

use editor_config::{Config, ConfigLoader, Platform, RuleSet};
use editor_desktop::EditorView;
use editor_desktop::actions::bind_keys;
use editor_desktop::keymap::editor_bindings;
use editor_desktop::settings_view::snippets_page::EditorField;
use editor_desktop::settings_view::{ControlRow, SettingsEvent, SettingsView};
use editor_desktop::text_input;
use gpui::{Entity, Focusable, TestAppContext, VisualTestContext};

/// A vault whose `.editor` folder is what the migrator makes of the
/// owner's `.obsidian` settings.
fn migrated_vault() -> tempfile::TempDir {
    let reference = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reference/obsidian");
    let vault = tempfile::tempdir().expect("a temp vault");
    let migration = editor_migrate::migrate_obsidian(&reference).expect("the reference migrates");
    migration
        .write_to(&vault.path().join(".editor"))
        .expect("the migrated files write");
    vault
}

fn migrated_config() -> Config {
    let vault = migrated_vault();
    let mut loader = ConfigLoader::for_vault(vault.path());
    let problems: Vec<_> = loader
        .load_all()
        .into_iter()
        .filter(|problem| problem.is_error())
        .collect();
    assert!(
        problems.is_empty(),
        "the migrated config loads: {problems:?}"
    );
    let config = loader.config().clone();
    assert!(config.typing.snippets.from_vault);
    assert!(config.typing.replacements.from_vault);
    config
}

fn open_with<'a>(
    cx: &'a mut TestAppContext,
    text: &str,
    config: Config,
) -> (Entity<EditorView>, &'a mut VisualTestContext) {
    cx.update(bind_keys);
    let end = text.len();
    let text = text.to_owned();
    let (view, cx) = cx.add_window_view(move |_, cx| {
        EditorView::with_config(&text, Vec::<PathBuf>::new(), &config, cx)
    });
    cx.update(|window, cx| window.focus(&view.focus_handle(cx)));
    view.update(cx, |view, cx| view.move_to(end, false, cx));
    cx.run_until_parked();
    (view, cx)
}

fn open<'a>(
    cx: &'a mut TestAppContext,
    text: &str,
) -> (Entity<EditorView>, &'a mut VisualTestContext) {
    open_with(cx, text, migrated_config())
}

fn press(cx: &mut VisualTestContext, command: &str) {
    let keystroke = editor_bindings(&RuleSet::defaults(), Platform::current())
        .into_iter()
        .find(|(_, id)| id == command)
        .map(|(keystroke, _)| keystroke)
        .unwrap_or_else(|| panic!("{command} has no key on this platform"));
    cx.simulate_keystrokes(&keystroke);
    cx.run_until_parked();
}

fn tab(cx: &mut VisualTestContext) {
    press(cx, "edit.indent");
}

/// The text with `|` at the cursor, or `[` and `]` around the selection.
fn shown(view: &Entity<EditorView>, cx: &mut VisualTestContext) -> String {
    view.read_with(cx, |view, _| {
        let mut text = view.text();
        let range = view.selected_range();
        if range.is_empty() {
            text.insert(range.start, '|');
        } else {
            text.insert(range.end, ']');
            text.insert(range.start, '[');
        }
        text
    })
}

#[gpui::test]
fn the_migrated_file_loads_every_active_snippet(cx: &mut TestAppContext) {
    let config = migrated_config();
    // 205 active Latex Suite snippets less the JavaScript one, with the
    // matrix family written once for inline and once for block math.
    assert!(config.typing.snippets.engine.len() >= 204);
    let (view, cx) = open_with(cx, "", config);
    cx.simulate_input("mk");
    assert_eq!(shown(&view, cx), "$|$");
}

#[gpui::test]
fn a_fraction_fills_in_with_tab(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "");
    cx.simulate_input("mk//");
    assert_eq!(shown(&view, cx), "$\\frac{|}{}$");
    let marked = view.read_with(cx, |view, _| view.pending_tab_stops());
    assert_eq!(marked.len(), 2, "the denominator and the end are marked");
    cx.simulate_input("a");
    tab(cx);
    cx.simulate_input("b");
    assert_eq!(shown(&view, cx), "$\\frac{a}{b|}$");
    tab(cx);
    assert_eq!(shown(&view, cx), "$\\frac{a}{b}|$");
    let marked = view.read_with(cx, |view, _| view.pending_tab_stops());
    assert!(marked.is_empty(), "the snippet is done");
}

#[gpui::test]
fn shift_tab_goes_back_a_stop(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "");
    cx.simulate_input("mk//a");
    tab(cx);
    press(cx, "edit.outdent");
    assert_eq!(shown(&view, cx), "$\\frac{[a]}{}$");
}

#[gpui::test]
fn moving_away_ends_the_snippet(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "x ");
    cx.simulate_input("mk//");
    view.update(cx, |view, cx| view.move_to(0, false, cx));
    let marked = view.read_with(cx, |view, _| view.pending_tab_stops());
    assert!(marked.is_empty());
}

#[gpui::test]
fn placeholders_are_selected_and_tab_triggers_wait_for_tab(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "");
    cx.simulate_input("mksum");
    assert_eq!(shown(&view, cx), "$\\sum|$");
    tab(cx);
    assert_eq!(shown(&view, cx), "$\\sum_{[i]=1}^{N} $");
    cx.simulate_input("k");
    tab(cx);
    assert_eq!(shown(&view, cx), "$\\sum_{k=[1]}^{N} $");
}

#[gpui::test]
fn whole_word_snippets_fire_on_the_space_after(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "");
    cx.simulate_input("mkforall x");
    assert_eq!(shown(&view, cx), "$\\forall x|$");
}

#[gpui::test]
fn named_patterns_and_greek(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "");
    cx.simulate_input("mkx2 + alpha");
    assert_eq!(shown(&view, cx), "$x_{2} + \\alpha|$");
}

#[gpui::test]
fn math_snippets_stay_quiet_in_text(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "");
    cx.simulate_input("reals and x2 or 1 // 2");
    assert_eq!(shown(&view, cx), "reals and x2 or 1 // 2|");
}

#[gpui::test]
fn dm_opens_a_math_block(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "");
    cx.simulate_input("dm");
    assert_eq!(shown(&view, cx), "$$\n|\n$$");
}

#[gpui::test]
fn tab_indents_where_nothing_else_takes_it(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "- item");
    tab(cx);
    assert_eq!(view.read_with(cx, |view, _| view.text()), "\t- item");
}

#[gpui::test]
fn a_slash_makes_a_fraction_and_tab_leaves_it(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "");
    cx.simulate_input("mkx^2/3");
    assert_eq!(shown(&view, cx), "$\\frac{x^2}{3|}$");
    tab(cx);
    assert_eq!(shown(&view, cx), "$\\frac{x^2}{3}|$");
    tab(cx);
    assert_eq!(
        shown(&view, cx),
        "$\\frac{x^2}{3}$|",
        "tab-out leaves the math"
    );
}

#[gpui::test]
fn tab_and_enter_fill_in_a_matrix(cx: &mut TestAppContext) {
    let text = "$$\n\\begin{pmatrix}\na\n\\end{pmatrix}\n$$";
    let (view, cx) = open(cx, text);
    let at = text.find("a\n").unwrap() + 1;
    view.update(cx, |view, cx| view.move_to(at, false, cx));
    tab(cx);
    cx.simulate_input("b");
    press(cx, "edit.newline");
    cx.simulate_input("c");
    assert_eq!(
        shown(&view, cx),
        "$$\n\\begin{pmatrix}\na & b \\\\\nc|\n\\end{pmatrix}\n$$"
    );
}

#[gpui::test]
fn brackets_grow_around_a_big_operator(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "");
    cx.simulate_input("mk(");
    assert_eq!(shown(&view, cx), "$(|)$");
    cx.simulate_input("sum");
    assert_eq!(shown(&view, cx), "$\\left(\\sum|\\right)$");
}

#[gpui::test]
fn replacements_fire_in_text(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "");
    cx.simulate_input("a -- b... c -> d >= e");
    assert_eq!(shown(&view, cx), "a — b… c → d ≥ e|");
}

#[gpui::test]
fn replacements_stay_out_of_code(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "`x`");
    view.update(cx, |view, cx| view.move_to(2, false, cx));
    cx.simulate_input("--");
    assert_eq!(shown(&view, cx), "`x--|`");
}

#[gpui::test]
fn prettifier_entries_fire_on_the_space_after(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, "");
    cx.simulate_input("go w/ me (c) ");
    assert_eq!(shown(&view, cx), "go with me © |");
}

#[gpui::test]
fn quotes_are_left_to_smart_quotes(cx: &mut TestAppContext) {
    let mut config = migrated_config();
    config.settings.editor.smart_quotes = false;
    let (view, cx) = open_with(cx, "", config);
    cx.simulate_input("say \"hi");
    assert_eq!(shown(&view, cx), "say \"hi|\"");
}

#[gpui::test]
fn the_settings_turn_snippets_and_replacements_off(cx: &mut TestAppContext) {
    let mut config = migrated_config();
    config.settings.editor.snippets = false;
    config.settings.editor.replacements = false;
    config.settings.math.auto_fraction = false;
    let (view, cx) = open_with(cx, "", config);
    cx.simulate_input("mk a -- b $x/");
    assert_eq!(shown(&view, cx), "mk a -- b $x/|$");
}

#[gpui::test]
fn the_built_in_snippets_work_without_a_vault_file(cx: &mut TestAppContext) {
    let (view, cx) = open_with(cx, "", Config::defaults());
    cx.simulate_input("x -- y mk@a");
    assert_eq!(shown(&view, cx), "x — y $\\alpha|$");
}

// ---- The settings page ----

fn open_settings<'a>(
    cx: &'a mut TestAppContext,
    root: &Path,
) -> (
    Entity<SettingsView>,
    &'a mut VisualTestContext,
    Rc<RefCell<Vec<String>>>,
) {
    cx.update(|cx| text_input::bind_keys(&RuleSet::defaults(), cx));
    let root = root.to_path_buf();
    let (view, cx) = cx.add_window_view(move |window, cx| {
        SettingsView::with_rules(root.clone(), &RuleSet::defaults(), window, cx)
    });
    let changed = Rc::new(RefCell::new(Vec::new()));
    let record = changed.clone();
    cx.update(|window, cx| {
        window.focus(&view.focus_handle(cx));
        cx.subscribe(&view, move |_, event: &SettingsEvent, _| {
            let SettingsEvent::Changed(key) = event;
            record.borrow_mut().push(key.clone());
        })
        .detach();
    });
    view.update(cx, |view, cx| view.show_section("snippets", cx));
    cx.run_until_parked();
    (view, cx, changed)
}

fn rows(view: &Entity<SettingsView>, cx: &mut VisualTestContext) -> Vec<ControlRow> {
    view.read_with(cx, |view, _| view.rows())
}

/// Focuses the first row `wanted` picks, as arrows would.
fn focus_row(
    view: &Entity<SettingsView>,
    cx: &mut VisualTestContext,
    wanted: impl Fn(&ControlRow) -> bool,
) {
    let index = rows(view, cx)
        .iter()
        .position(wanted)
        .expect("the row is on the page");
    view.update_in(cx, |view, window, cx| view.focus_control(index, window, cx));
}

fn is_snippet(trigger: &'static str) -> impl Fn(&ControlRow) -> bool {
    move |row| matches!(row, ControlRow::Snippet(row) if row.trigger == trigger)
}

fn type_into(
    view: &Entity<SettingsView>,
    field: EditorField,
    text: &str,
    cx: &mut VisualTestContext,
) {
    let input = view
        .read_with(cx, |view, _| view.snippet_field(field))
        .expect("the editor is open");
    cx.update(|window, cx| window.focus(&input.focus_handle(cx)));
    // Typed as the keyboard would, whether or not the field is scrolled
    // into view.
    input.update(cx, |input, cx| {
        let all = 0..input.text().len();
        input.replace(all, text, cx);
    });
    cx.run_until_parked();
}

fn snippets_file(vault: &tempfile::TempDir) -> String {
    fs::read_to_string(vault.path().join(".editor/snippets.txt")).unwrap_or_default()
}

#[gpui::test]
fn the_page_lists_the_migrated_snippets_by_group(cx: &mut TestAppContext) {
    let vault = migrated_vault();
    let (view, cx, _) = open_settings(cx, vault.path());
    let rows = rows(&view, cx);
    let snippets = rows
        .iter()
        .filter(|row| matches!(row, ControlRow::Snippet(_)))
        .count();
    assert!(snippets >= 204, "{snippets} snippets listed");
    let replacements = rows
        .iter()
        .filter(|row| matches!(row, ControlRow::Replacement(_)))
        .count();
    assert_eq!(replacements, 80, "every replacement but the two quotes");
    let titles: Vec<String> = view.read_with(cx, |view, _| {
        view.layout()
            .cards
            .iter()
            .filter_map(|card| card.title.clone())
            .collect()
    });
    assert!(titles.contains(&"Greek letters".to_string()));
    assert!(titles.contains(&"Arrows".to_string()));
}

#[gpui::test]
fn search_narrows_the_snippets(cx: &mut TestAppContext) {
    let vault = migrated_vault();
    let (view, cx, _) = open_settings(cx, vault.path());
    view.update(cx, |view, cx| view.search("mathbb", cx));
    let triggers: Vec<String> = rows(&view, cx)
        .into_iter()
        .filter_map(|row| match row {
            ControlRow::Snippet(row) => Some(row.trigger),
            _ => None,
        })
        .collect();
    assert!(triggers.contains(&"reals".to_string()));
    assert!(!triggers.contains(&"mk".to_string()));
}

#[gpui::test]
fn space_switches_a_snippet_off_in_the_file(cx: &mut TestAppContext) {
    let vault = migrated_vault();
    let (view, cx, changed) = open_settings(cx, vault.path());
    focus_row(&view, cx, is_snippet("mk"));
    cx.simulate_keystrokes("space");
    assert!(snippets_file(&vault).contains("text, instant, off"));
    assert_eq!(changed.borrow().as_slice(), ["snippets"]);
    let row = rows(&view, cx).into_iter().find(is_snippet("mk")).unwrap();
    assert!(matches!(row, ControlRow::Snippet(row) if !row.on));
}

#[gpui::test]
fn the_editor_tries_a_snippet_as_you_type(cx: &mut TestAppContext) {
    let vault = migrated_vault();
    let (view, cx, _) = open_settings(cx, vault.path());
    focus_row(&view, cx, is_snippet("//"));
    cx.simulate_keystrokes("enter");
    let (line, problem, _) = view
        .read_with(cx, |view, cx| view.snippet_editor_state(cx))
        .expect("the editor opens");
    assert_eq!(line, "// → \\frac{●}{●}●  math, instant");
    assert!(problem.is_none());
    type_into(&view, EditorField::Test, "a//b", cx);
    let (_, _, result) = view
        .read_with(cx, |view, cx| view.snippet_editor_state(cx))
        .unwrap();
    let result = result.expect("the test box shows a result");
    assert_eq!(result.text, "a\\frac{b}{}");
    assert_eq!(result.caret, "a\\frac{b".len());
}

#[gpui::test]
fn a_mistake_is_pointed_out_and_nothing_is_written(cx: &mut TestAppContext) {
    let vault = migrated_vault();
    let before = snippets_file(&vault);
    let (view, cx, changed) = open_settings(cx, vault.path());
    focus_row(&view, cx, is_snippet("mk"));
    cx.simulate_keystrokes("enter");
    type_into(&view, EditorField::Options, "text, sometimes", cx);
    let (_, problem, _) = view
        .read_with(cx, |view, cx| view.snippet_editor_state(cx))
        .unwrap();
    assert!(problem.unwrap().starts_with("Unknown option `sometimes`"));
    cx.simulate_keystrokes("enter");
    assert_eq!(snippets_file(&vault), before);
    assert!(changed.borrow().is_empty());
}

#[gpui::test]
fn saving_a_changed_snippet_rewrites_its_line(cx: &mut TestAppContext) {
    let vault = migrated_vault();
    let (view, cx, _) = open_settings(cx, vault.path());
    focus_row(&view, cx, is_snippet("mk"));
    cx.simulate_keystrokes("enter");
    type_into(&view, EditorField::Expansion, "\\(●\\)", cx);
    cx.simulate_keystrokes("enter");
    let file = snippets_file(&vault);
    assert!(file.contains("mk              → \\(●\\)"), "{file}");
    assert!(
        view.read_with(cx, |view, cx| view.snippet_editor_state(cx))
            .is_none()
    );
}

#[gpui::test]
fn a_new_snippet_goes_under_its_own_heading(cx: &mut TestAppContext) {
    let vault = tempfile::tempdir().unwrap();
    let (view, cx, changed) = open_settings(cx, vault.path());
    focus_row(&view, cx, |row| *row == ControlRow::SnippetsFile);
    cx.simulate_keystrokes("enter");
    type_into(&view, EditorField::Trigger, "qq", cx);
    type_into(&view, EditorField::Expansion, "\\quad", cx);
    type_into(&view, EditorField::Test, "a qq", cx);
    let (_, _, result) = view
        .read_with(cx, |view, cx| view.snippet_editor_state(cx))
        .unwrap();
    assert_eq!(result.unwrap().text, "a \\quad");
    cx.simulate_keystrokes("enter");
    let file = snippets_file(&vault);
    assert!(
        file.starts_with("# The built-in snippets"),
        "the built-in list is copied"
    );
    let last: Vec<&str> = file.lines().rev().take(2).collect();
    assert_eq!(last[1], "# Added in settings");
    let words: Vec<&str> = last[0].split_whitespace().collect();
    assert_eq!(words, ["qq", "→", "\\quad", "math,", "instant"]);
    assert_eq!(changed.borrow().as_slice(), ["snippets"]);
}

#[gpui::test]
fn deleting_a_snippet_removes_its_line(cx: &mut TestAppContext) {
    let vault = migrated_vault();
    let (view, cx, _) = open_settings(cx, vault.path());
    focus_row(&view, cx, is_snippet("ooo"));
    cx.simulate_keystrokes("enter");
    view.update_in(cx, |view, window, cx| view.delete_snippet(window, cx));
    assert!(!snippets_file(&vault).contains("ooo"));
    assert!(!rows(&view, cx).iter().any(is_snippet("ooo")));
}

#[gpui::test]
fn a_replacement_switches_off_in_its_file(cx: &mut TestAppContext) {
    let vault = migrated_vault();
    let (view, cx, changed) = open_settings(cx, vault.path());
    focus_row(
        &view,
        cx,
        |row| matches!(row, ControlRow::Replacement(row) if row.from == "--"),
    );
    cx.simulate_keystrokes("space");
    let text = fs::read_to_string(vault.path().join(".editor/replacements.toml")).unwrap();
    let table = editor_snippets::Replacements::from_toml(&text).unwrap();
    let dash = table
        .entries
        .iter()
        .find(|entry| entry.from == "--")
        .unwrap();
    assert!(!dash.enabled);
    assert_eq!(changed.borrow().as_slice(), ["replacements"]);
}
