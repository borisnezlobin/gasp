//! Merges Smart Typography and Enhanced Symbols Prettifier settings into one table.
//!
//! Smart Typography entries fire as soon as they are typed and never in code, math or
//! frontmatter, like the plugin. Prettifier entries fire on the space typed after them
//! (assumed from how the plugin behaved; its source is no longer published). Entries
//! whose result is a LaTeX command, such as `\equil`, are limited to math.

use gasp_snippets::{InputContext, Replacement, ReplacementFire, Replacements};
use serde_json::Value;

/// The merged table and notes for the report.
#[derive(Clone, Debug)]
pub struct ReplacementsMigration {
    pub table: Replacements,
    pub notes: Vec<String>,
}

/// Builds the table from the two plugins' `data.json` contents; either may be missing.
pub fn migrate(
    smart_typography: Option<&str>,
    prettifier: Option<&str>,
) -> Result<ReplacementsMigration, String> {
    let mut entries = Vec::new();
    let mut notes = Vec::new();
    if let Some(text) = smart_typography {
        let settings = parse_json(text, "obsidian-smart-typography.json")?;
        entries.extend(smart_typography_entries(&settings));
    }
    if let Some(text) = prettifier {
        let settings = parse_json(text, "enhanced-symbols-prettifier.json")?;
        entries.extend(prettifier_entries(&settings, &mut notes));
    }
    let table = Replacements::new(entries);
    notes.extend(shadow_notes(&table));
    Ok(ReplacementsMigration { table, notes })
}

fn parse_json(text: &str, name: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|error| format!("{name} isn't valid JSON: {error}"))
}

fn flag(settings: &Value, key: &str, default: bool) -> bool {
    settings
        .get(key)
        .and_then(Value::as_bool)
        .unwrap_or(default)
}

fn text<'a>(settings: &'a Value, key: &str, default: &'a str) -> &'a str {
    settings.get(key).and_then(Value::as_str).unwrap_or(default)
}

fn entry(from: &str, to: &str, group: &str, enabled: bool) -> Replacement {
    let mut replacement = Replacement::new(from, to, group);
    replacement.enabled = enabled;
    replacement
}

const FRACTIONS: [(&str, &str); 18] = [
    ("1/2", "½"),
    ("1/3", "⅓"),
    ("2/3", "⅔"),
    ("1/4", "¼"),
    ("3/4", "¾"),
    ("1/5", "⅕"),
    ("2/5", "⅖"),
    ("3/5", "⅗"),
    ("4/5", "⅘"),
    ("1/6", "⅙"),
    ("5/6", "⅚"),
    ("1/7", "⅐"),
    ("1/8", "⅛"),
    ("3/8", "⅜"),
    ("5/8", "⅝"),
    ("7/8", "⅞"),
    ("1/9", "⅑"),
    ("1/10", "⅒"),
];

fn smart_typography_entries(settings: &Value) -> Vec<Replacement> {
    let mut entries = dash_entries(settings);
    entries.push(entry(
        "...",
        "…",
        "Ellipsis",
        flag(settings, "ellipsis", true),
    ));
    entries.extend(quote_entries(settings));
    let arrows = flag(settings, "arrows", true);
    entries.push(entry(
        "<-",
        text(settings, "leftArrow", "←"),
        "Arrows",
        arrows,
    ));
    entries.push(entry(
        "->",
        text(settings, "rightArrow", "→"),
        "Arrows",
        arrows,
    ));
    let guillemets = flag(settings, "guillemets", false);
    let open = text(settings, "openGuillemet", "«");
    let close = text(settings, "closeGuillemet", "»");
    entries.push(entry("<<", open, "Guillemets", guillemets));
    entries.push(entry(">>", close, "Guillemets", guillemets));
    let comparisons = flag(settings, "comparisons", true);
    for (from, to) in [(">=", "≥"), ("<=", "≤"), ("/=", "≠")] {
        entries.push(entry(from, to, "Comparisons", comparisons));
    }
    let fractions = flag(settings, "fractions", false);
    entries.extend(FRACTIONS.iter().map(|(from, to)| {
        let mut fraction = entry(from, to, "Fractions", fractions);
        fraction.word_start = true;
        fraction
    }));
    entries
}

fn dash_entries(settings: &Value) -> Vec<Replacement> {
    let enabled = flag(settings, "emDash", true);
    let mut entries = if flag(settings, "skipEnDash", false) {
        vec![entry("--", "—", "Dashes", enabled)]
    } else {
        vec![
            entry("--", "–", "Dashes", enabled),
            entry("–-", "—", "Dashes", enabled),
        ]
    };
    entries.push(entry("—-", "---", "Dashes", enabled));
    entries
}

fn quote_entries(settings: &Value) -> Vec<Replacement> {
    let enabled = flag(settings, "curlyQuotes", true);
    let quote = |from: &str, open: &str, close: &str| {
        let mut replacement = entry(from, text(settings, open, ""), "Quotes", enabled);
        replacement.closing = Some(text(settings, close, "").to_string());
        replacement
    };
    let mut double = quote("\"", "openDouble", "closeDouble");
    let mut single = quote("'", "openSingle", "closeSingle");
    fill_default(&mut double, "“", "”");
    fill_default(&mut single, "‘", "’");
    vec![double, single]
}

fn fill_default(replacement: &mut Replacement, open: &str, close: &str) {
    if replacement.to.is_empty() {
        replacement.to = open.to_string();
    }
    if replacement.closing.as_deref() == Some("") {
        replacement.closing = Some(close.to_string());
    }
}

fn prettifier_entries(settings: &Value, notes: &mut Vec<String>) -> Vec<Replacement> {
    let flexible_start = flag(settings, "flexibleWordsStart", false);
    if flexible_start {
        notes.push(
            "The prettifier's flexible word start wasn't kept: triggers that begin with a letter, such as `pi` and `ppy`, fire only at the start of a word, so \"api\" and \"happy\" stay as typed.".to_string(),
        );
    }
    if let Some(count) = settings
        .get("exclusions")
        .and_then(Value::as_array)
        .map(Vec::len)
    {
        notes.push(format!(
            "The prettifier's `exclusions` list ({count} items, mostly repeated copies of the table's own keys) wasn't imported; what it did in the plugin couldn't be confirmed."
        ));
    }
    let Some(table) = settings.get("replacements").and_then(Value::as_object) else {
        return Vec::new();
    };
    table
        .iter()
        .filter_map(|(key, value)| prettifier_entry(key, value, flexible_start, notes))
        .collect()
}

/// Whether a trigger must start a word: any trigger that begins with a
/// letter or digit does. The plugin's flexible start would let those fire
/// inside ordinary words (`pi` in "api", `ppy` in "happy", `w/` in
/// "saw/"), so it's kept only as a note; triggers made of symbols, such as
/// `->`, fire anywhere either way.
fn word_start(from: &str, _flexible_start: bool) -> bool {
    from.starts_with(char::is_alphanumeric)
}

fn prettifier_entry(
    key: &str,
    value: &Value,
    flexible_start: bool,
    notes: &mut Vec<String>,
) -> Option<Replacement> {
    let from = value.get("replaced").and_then(Value::as_str).unwrap_or(key);
    let to = value.get("value").and_then(Value::as_str)?;
    let group = value.get("group").and_then(Value::as_str).unwrap_or("");
    let disabled = value
        .get("disabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut replacement = entry(from, to, group, !disabled);
    replacement.fire = ReplacementFire::AfterSpace;
    replacement.word_start = word_start(from, flexible_start);
    if to.starts_with('\\') {
        replacement.contexts = Some(vec![InputContext::Math]);
        notes.push(format!(
            "`{from}` → `{to}` produces a LaTeX command, so it fires in math only."
        ));
    }
    Some(replacement)
}

fn shadow_notes(table: &Replacements) -> Vec<String> {
    table
        .shadowed()
        .into_iter()
        .map(|(shadowed, by)| shadow_note(&table.entries[shadowed], &table.entries[by]))
        .collect()
}

fn shadow_note(shadowed: &Replacement, by: &Replacement) -> String {
    if shadowed.from == by.from && shadowed.to == by.to {
        return format!(
            "`{}` → `{}` ({}) repeats a typing rule that already fires instantly, so it never fires on its own.",
            shadowed.from, shadowed.to, shadowed.group
        );
    }
    format!(
        "`{}` → `{}` ({}) never fires: `{}` → `{}` ({}) replaces part of it while you type, as it did in Obsidian.",
        shadowed.from, shadowed.to, shadowed.group, by.from, by.to, by.group
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn letter_triggers_wait_for_a_word_start_even_when_flexible() {
        assert!(word_start("pi", true));
        assert!(word_start("ppy", true));
        assert!(word_start("w/", true));
        assert!(!word_start("->", true));
        assert!(!word_start("(c)", true));
        assert!(word_start("pi", false));
    }

    use super::*;

    #[test]
    fn skip_en_dash_goes_straight_to_em_dash() {
        let migration = migrate(Some(r#"{"skipEnDash": true}"#), None).unwrap();
        let dashes: Vec<(&str, &str)> = migration
            .table
            .entries
            .iter()
            .filter(|e| e.group == "Dashes")
            .map(|e| (e.from.as_str(), e.to.as_str()))
            .collect();
        assert_eq!(dashes, vec![("--", "—"), ("—-", "---")]);

        let migration = migrate(Some(r#"{"skipEnDash": false}"#), None).unwrap();
        let first = &migration.table.entries[0];
        assert_eq!((first.from.as_str(), first.to.as_str()), ("--", "–"));
    }

    #[test]
    fn switched_off_groups_stay_off() {
        let migration = migrate(Some(r#"{"fractions": false, "arrows": true}"#), None).unwrap();
        let half = migration
            .table
            .entries
            .iter()
            .find(|e| e.from == "1/2")
            .unwrap();
        assert!(!half.enabled && half.word_start);
    }

    #[test]
    fn prettifier_entries_fire_after_space_and_keep_disabled() {
        let prettifier = r#"{"replacements": {
            "(1)": {"replaced": "(1)", "value": "1️⃣", "group": "Numbers", "disabled": true},
            "pi": {"replaced": "pi", "value": "π", "group": "Math"},
            "\\equil": {"replaced": "\\equil", "value": "\\rightleftharpoons", "group": "misc"}
        }, "flexibleWordsStart": false}"#;
        let migration = migrate(None, Some(prettifier)).unwrap();
        let entries = &migration.table.entries;
        assert!(!entries[0].enabled);
        assert_eq!(entries[1].fire, ReplacementFire::AfterSpace);
        assert!(entries[1].word_start);
        assert_eq!(entries[2].contexts, Some(vec![InputContext::Math]));
    }
}
