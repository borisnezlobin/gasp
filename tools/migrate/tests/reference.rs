//! Runs the migrator against the real Obsidian settings in `reference/obsidian`.

use std::path::PathBuf;
use std::process::Command;

use gasp_migrate::latex_suite::{Outcome, SourceKind};
use gasp_migrate::{Migration, migrate_obsidian};
use gasp_snippets::{
    InputContext, ReplacementFire, Replacements, Request, SnippetEngine, SnippetFile, TriggerKey,
    parse_line,
};

fn reference_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../reference/obsidian")
}

fn migration() -> Migration {
    migrate_obsidian(&reference_dir()).expect("the reference settings migrate")
}

#[test]
fn latex_suite_counts_match_the_real_file() {
    let migration = migration();
    let latex = migration.latex_suite.as_ref().unwrap();
    // PLAN.md's 212 counts the 7 snippets that are commented out in the file.
    assert_eq!(latex.converted.len(), 205);
    assert_eq!(latex.commented_out, 7);
    assert_eq!(latex.count_kind(SourceKind::Plain), 163);
    assert_eq!(latex.count_kind(SourceKind::Regex), 41);
    assert_eq!(latex.count_kind(SourceKind::Function), 1);
    assert_eq!(latex.readable().count(), 202);
    assert_eq!(latex.regex_form().count(), 2);
    assert_eq!(latex.review().count(), 1);
}

#[test]
fn every_migrated_snippet_parses_back_identically() {
    let migration = migration();
    let latex = migration.latex_suite.as_ref().unwrap();
    for converted in &latex.converted {
        let snippets = match &converted.outcome {
            Outcome::Readable(snippets) => snippets.clone(),
            Outcome::RegexForm(snippet, _) => vec![snippet.clone()],
            Outcome::Review(_) => continue,
        };
        for snippet in snippets {
            let line = snippet.to_string();
            let parsed = parse_line(&line, converted.line)
                .unwrap_or_else(|error| panic!("`{line}` doesn't parse: {error}"));
            assert_eq!(parsed, snippet, "`{line}` reads back differently");
        }
    }
}

#[test]
fn the_snippets_file_parses_and_compiles() {
    let migration = migration();
    let text = migration.latex_suite.as_ref().unwrap().file.to_string();
    let file = SnippetFile::parse(&text).unwrap_or_else(|errors| panic!("{errors:?}"));
    assert_eq!(file.snippets().count(), 226);
    assert_eq!(file.to_string(), text, "writing the parsed file changes it");
    let engine = SnippetEngine::new(file.snippets().cloned().collect()).unwrap();
    assert_eq!(engine.len(), 226);
}

fn expand(engine: &SnippetEngine, before: &str, key: TriggerKey) -> Option<String> {
    let edit = engine.expand(&Request::new(before, InputContext::Math, key))?;
    let mut out = before.to_string();
    out.replace_range(edit.replace, &edit.text);
    Some(out)
}

#[test]
fn migrated_snippets_behave_like_latex_suite() {
    let migration = migration();
    let file = &migration.latex_suite.as_ref().unwrap().file;
    let engine = SnippetEngine::new(file.snippets().cloned().collect()).unwrap();
    let typed = |before: &str| {
        let last = before.chars().last().unwrap();
        expand(&engine, before, TriggerKey::Char(last))
    };
    assert_eq!(typed("a forall ").as_deref(), Some("a \\forall "));
    assert_eq!(typed("x2").as_deref(), Some("x_{2}"));
    assert_eq!(typed("2alpha").as_deref(), Some("2\\alpha"));
    assert_eq!(typed("\\alpha sr").as_deref(), Some("\\alpha^{2}"));
    assert_eq!(typed("xhat").as_deref(), Some("\\hat{x}"));
    assert_eq!(typed("1+sin").as_deref(), Some("1+\\sin"));
    assert_eq!(typed("\\sinx").as_deref(), Some("\\sin x"));
    assert_eq!(typed("x_12").as_deref(), Some("x_{12}"));
    assert_eq!(typed("cdot").as_deref(), Some("\\cdot"));
    assert_eq!(
        expand(&engine, "\\sum", TriggerKey::Tab).as_deref(),
        Some("\\sum_{i=1}^{N} ")
    );
}

#[test]
fn replacements_keep_disabled_entries_off() {
    let migration = migration();
    let table = &migration.replacements.as_ref().unwrap().table;
    let find = |from: &str, fire: ReplacementFire| {
        table
            .entries
            .iter()
            .find(|e| e.from == from && e.fire == fire)
            .unwrap_or_else(|| panic!("no `{from}` entry"))
    };
    assert!(!find("(1)", ReplacementFire::AfterSpace).enabled);
    assert!(!find("...", ReplacementFire::AfterSpace).enabled);
    assert!(!find("alpha", ReplacementFire::AfterSpace).enabled);
    assert!(find("(c)", ReplacementFire::AfterSpace).enabled);
    assert!(!find("1/2", ReplacementFire::Instant).enabled);
    assert!(!find("<<", ReplacementFire::Instant).enabled);
    assert_eq!(find("--", ReplacementFire::Instant).to, "—");
    assert!(
        !table.entries.iter().any(|e| e.to == "–"),
        "skipEnDash is on"
    );
    assert_eq!(table.entries.len(), 82);
    assert_eq!(table.entries.iter().filter(|e| !e.enabled).count(), 40);

    let text = table.to_toml();
    assert_eq!(&Replacements::from_toml(&text).unwrap(), table);
}

#[test]
fn replacements_apply_like_the_plugins() {
    let migration = migration();
    let table = &migration.replacements.as_ref().unwrap().table;
    let apply = |before: &str, context: InputContext| {
        let typed = before.chars().last().unwrap();
        let Some(edit) = table.find(before, typed, context) else {
            return before.to_string();
        };
        let mut out = before.to_string();
        out.replace_range(edit.replace, &edit.text);
        out
    };
    assert_eq!(apply("wait--", InputContext::Text), "wait—");
    assert_eq!(apply("say \"", InputContext::Text), "say “");
    assert_eq!(apply("“hi\"", InputContext::Text), "“hi”");
    assert_eq!(apply("it'", InputContext::Text), "it’");
    assert_eq!(apply("a ->", InputContext::Text), "a →");
    assert_eq!(apply("a <=", InputContext::Text), "a ≤");
    assert_eq!(apply("wait...", InputContext::Text), "wait…");
    assert_eq!(apply("(c) ", InputContext::Text), "© ");
    assert_eq!(apply("go w/o ", InputContext::Text), "go without ");
    assert_eq!(apply("x--", InputContext::Code), "x--");
    assert_eq!(apply("x--", InputContext::Math), "x--");
    assert_eq!(
        apply("\\equil ", InputContext::Math),
        "\\rightleftharpoons "
    );
}

#[test]
fn hotkeys_become_rules_with_the_new_defaults() {
    let migration = migration();
    let hotkeys = migration.hotkeys.as_ref().unwrap();
    let rules: Vec<(&str, &str)> = hotkeys
        .rules
        .iter()
        .map(|rule| (rule.keys.as_str(), rule.command.as_str()))
        .collect();
    assert_eq!(
        rules,
        vec![
            ("Mod+,", "settings.open"),
            ("Mod+L", "settings.open"),
            ("Mod+J", "prose.toggle-sentence-highlighting"),
            ("Mod+Shift+R", "find.replace"),
            ("Mod+Shift+F", "search.open"),
            ("Alt+0", "footnote.insert-or-jump"),
            ("Mod+Shift+N", "vault.open"),
            ("Mod+S", "sync.now"),
            ("Mod+Shift+S", "app.export"),
        ]
    );
    assert!(!rules.iter().any(|(keys, _)| *keys == "Mod+M"));
    assert_eq!(hotkeys.unmapped.len(), 1);
    assert_eq!(hotkeys.unmapped[0].0, "editor:save-file");
    let parsed: toml::Table = toml::from_str(&hotkeys.to_toml()).unwrap();
    assert_eq!(parsed["rule"].as_array().unwrap().len(), 9);
}

#[test]
fn app_settings_become_a_settings_fragment() {
    let migration = migration();
    let text = migration.settings.as_ref().unwrap().to_toml();
    let parsed: toml::Table = toml::from_str(&text).unwrap();
    assert_eq!(
        parsed["files"]["attachments-folder"].as_str(),
        Some("./images")
    );
    assert_eq!(
        parsed["files"]["update-links-on-rename"].as_bool(),
        Some(true)
    );
    assert_eq!(parsed["files"]["trash"].as_str(), Some("system"));
    assert_eq!(parsed["editor"]["show-inline-title"].as_bool(), Some(true));
    assert_eq!(
        parsed["appearance"]["base-font-size"].as_integer(),
        Some(12)
    );
    let theme: toml::Table = toml::from_str(
        &migration
            .settings
            .as_ref()
            .unwrap()
            .theme_to_toml()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(theme["font"]["text"].as_str(), Some("Charter"));
    assert_eq!(theme["font"]["ui"].as_str(), Some("Charter"));
    assert_eq!(theme["font"]["code"].as_str(), Some("Courier New"));
    assert_eq!(theme["color"]["accent"].as_str(), Some("#000000"));
}

#[test]
fn output_is_deterministic() {
    let first = migration().output_files();
    let second = migration().output_files();
    assert_eq!(first, second);
    let names: Vec<&str> = first.iter().map(|file| file.name).collect();
    assert_eq!(
        names,
        vec![
            "snippets.txt",
            "replacements.toml",
            "rules.toml",
            "settings.toml",
            "theme.toml",
            "migration-report.txt"
        ]
    );
}

#[test]
fn command_line_writes_every_file() {
    let out = tempfile::tempdir().unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_gasp-migrate"))
        .arg("--obsidian")
        .arg(reference_dir())
        .arg("--out")
        .arg(out.path())
        .output()
        .unwrap();
    assert!(status.status.success(), "{status:?}");
    for name in [
        "snippets.txt",
        "replacements.toml",
        "rules.toml",
        "settings.toml",
        "theme.toml",
        "migration-report.txt",
    ] {
        assert!(out.path().join(name).is_file(), "{name} is missing");
    }
    let report = std::fs::read_to_string(out.path().join("migration-report.txt")).unwrap();
    assert!(report.contains("Needs review: 1"));

    let usage = Command::new(env!("CARGO_BIN_EXE_gasp-migrate"))
        .arg("--out")
        .output()
        .unwrap();
    assert!(!usage.status.success());
}
