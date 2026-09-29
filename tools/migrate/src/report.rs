//! The plain-text migration report.

use std::fmt::Write;

use crate::Migration;
use crate::app_settings::SettingsMigration;
use crate::hotkeys::HotkeyMigration;
use crate::latex_suite::{Converted, LatexSuiteMigration, Outcome, SourceKind};
use crate::replacements::ReplacementsMigration;

/// Renders the report for a migration. The output is deterministic.
pub fn render_report(migration: &Migration) -> String {
    let mut out = String::from("gasp-migrate report\n");
    if !migration.missing.is_empty() {
        let _ = writeln!(
            out,
            "\nNot found, skipped: {}",
            migration.missing.join(", ")
        );
    }
    if let Some(latex_suite) = &migration.latex_suite {
        latex_suite_section(&mut out, latex_suite);
    }
    if let Some(replacements) = &migration.replacements {
        replacements_section(&mut out, replacements);
    }
    if let Some(hotkeys) = &migration.hotkeys {
        hotkeys_section(&mut out, hotkeys);
    }
    if let Some(settings) = &migration.settings {
        settings_section(&mut out, settings);
    }
    out
}

fn latex_suite_section(out: &mut String, migration: &LatexSuiteMigration) {
    let readable_lines: usize = migration
        .readable()
        .map(|c| match &c.outcome {
            Outcome::Readable(snippets) => snippets.len(),
            _ => 0,
        })
        .sum();
    let _ = writeln!(out, "\nLatex Suite snippets ({})", crate::SNIPPETS_FILE);
    let _ = writeln!(
        out,
        "  Source snippets: {} ({} plain, {} regex, {} JavaScript function), plus {} commented out and skipped",
        migration.converted.len(),
        migration.count_kind(SourceKind::Plain),
        migration.count_kind(SourceKind::Regex),
        migration.count_kind(SourceKind::Function),
        migration.commented_out,
    );
    let _ = writeln!(
        out,
        "  Converted to readable snippets: {} (written as {readable_lines} lines)",
        migration.readable().count()
    );
    let _ = writeln!(out, "  Kept as regex: {}", migration.regex_form().count());
    let _ = writeln!(out, "  Needs review: {}", migration.review().count());
    list(
        out,
        "Kept as regex, because the regex",
        migration.regex_form(),
    );
    list(out, "Needs review", migration.review());
    let notes: Vec<&String> = migration
        .converted
        .iter()
        .filter_map(|c| c.note.as_ref())
        .collect();
    notes_list(out, notes);
}

fn list<'a>(out: &mut String, title: &str, items: impl Iterator<Item = &'a Converted>) {
    let items: Vec<&Converted> = items.collect();
    if items.is_empty() {
        return;
    }
    let _ = writeln!(out, "  {title}:");
    for item in items {
        let reason = match &item.outcome {
            Outcome::RegexForm(_, reason) | Outcome::Review(reason) => reason.as_str(),
            Outcome::Readable(_) => "",
        };
        let _ = writeln!(
            out,
            "    line {}: `{}` {reason}",
            item.line,
            item.trigger.replace('\n', " ")
        );
    }
}

fn notes_list<'a>(out: &mut String, notes: impl IntoIterator<Item = &'a String>) {
    let notes: Vec<&String> = notes.into_iter().collect();
    if notes.is_empty() {
        return;
    }
    let _ = writeln!(out, "  Notes:");
    for note in notes {
        let _ = writeln!(out, "    {note}");
    }
}

fn replacements_section(out: &mut String, migration: &ReplacementsMigration) {
    let entries = &migration.table.entries;
    let enabled = entries.iter().filter(|e| e.enabled).count();
    let _ = writeln!(out, "\nReplacements ({})", crate::REPLACEMENTS_FILE);
    let _ = writeln!(
        out,
        "  {} entries, {enabled} switched on and {} switched off.",
        entries.len(),
        entries.len() - enabled
    );
    notes_list(out, &migration.notes);
}

fn hotkeys_section(out: &mut String, migration: &HotkeyMigration) {
    let _ = writeln!(out, "\nHotkeys ({})", crate::RULES_FILE);
    let _ = writeln!(out, "  {} key rules.", migration.rules.len());
    if !migration.unmapped.is_empty() {
        let _ = writeln!(out, "  No matching command, not imported:");
        for (id, keys) in &migration.unmapped {
            let _ = writeln!(out, "    `{id}` ({})", keys.join(", "));
        }
    }
    if !migration.cleared.is_empty() {
        let _ = writeln!(
            out,
            "  Obsidian defaults you had removed (no keys, nothing to import): {}",
            migration.cleared.join(", ")
        );
    }
    notes_list(out, &migration.notes);
}

fn settings_section(out: &mut String, migration: &SettingsMigration) {
    let _ = writeln!(out, "\nSettings ({})", crate::SETTINGS_FILE);
    let _ = writeln!(out, "  {} settings imported.", migration.values.len());
    notes_list(out, &migration.notes);
}
