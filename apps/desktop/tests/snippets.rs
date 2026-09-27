//! Snippets, replacements and the math helpers, typed through the editor
//! with the owner's Latex Suite and Smart Typography settings as the
//! migrator converts them from `reference/obsidian`.

use std::path::{Path, PathBuf};

use editor_config::{Config, ConfigLoader, Platform, RuleSet};
use editor_desktop::EditorView;
use editor_desktop::actions::bind_keys;
use editor_desktop::keymap::editor_bindings;
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
