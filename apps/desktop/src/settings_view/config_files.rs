//! Writing the vault's `.editor/theme.toml` one token at a time and
//! adding or removing the user's own key rules in `.editor/rules.toml`,
//! keeping everything else in both files as the user wrote it.

use std::path::{Path, PathBuf};

use editor_config::commands::BUILTIN_COMMANDS;
use editor_config::loader::{CONFIG_DIR, build_rules, build_theme};
use editor_config::theme::Theme as Tokens;
use editor_config::{Config, RuleSet};
use serde_json::Value;
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, value};

use super::model::user_rule_id;
use super::store::{SettingsFile, save};

pub const THEME_FILE: &str = "theme.toml";
pub const RULES_FILE: &str = "rules.toml";

pub fn theme_path(vault_root: &Path) -> PathBuf {
    vault_root.join(CONFIG_DIR).join(THEME_FILE)
}

pub fn rules_path(vault_root: &Path) -> PathBuf {
    vault_root.join(CONFIG_DIR).join(RULES_FILE)
}

/// The vault's theme tokens layered on the built-in ones, or the built-in
/// ones when the file is missing or broken.
pub fn load_tokens(vault_root: &Path) -> Tokens {
    let text = std::fs::read_to_string(theme_path(vault_root)).ok();
    build_theme(THEME_FILE, text.as_deref())
        .map(|(tokens, _)| tokens)
        .unwrap_or_else(|_| Config::defaults().theme)
}

/// The built-in value of a theme token, resolved.
pub fn default_token(name: &str) -> Option<String> {
    Config::defaults().theme.text(name).map(str::to_string)
}

/// Whether two token values mean the same, ignoring case and spaces, so
/// `#FFFFFF` matches `#ffffff`.
fn same_token(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// Writes one theme token: `None`, or the built-in value, removes it.
/// The result must still resolve, or nothing is written and the reason
/// comes back. Returns the tokens now in effect.
pub fn write_theme_token(
    vault_root: &Path,
    name: &str,
    new_value: Option<&str>,
) -> Result<Tokens, String> {
    let path = theme_path(vault_root);
    let mut file = SettingsFile::load(&path)?;
    let default = default_token(name);
    match new_value {
        Some(text) if !default.as_deref().is_some_and(|d| same_token(d, text)) => {
            file.set(name, &Value::from(text.trim()))?;
        }
        _ => {
            file.remove(name);
        }
    }
    let text = file.to_string();
    let (tokens, _) = build_theme(THEME_FILE, Some(&text)).map_err(|diagnostics| {
        diagnostics.first().map_or_else(
            || "that value isn't allowed".to_string(),
            |d| d.message.clone(),
        )
    })?;
    save(&path, &text).map_err(|error| error.to_string())?;
    Ok(tokens)
}

fn known_commands() -> Vec<&'static str> {
    BUILTIN_COMMANDS.iter().map(|spec| spec.id).collect()
}

/// The built-in rules with the vault's on top, or just the built-in ones
/// when the vault's file is missing or broken.
pub fn load_rules(vault_root: &Path) -> RuleSet {
    let text = std::fs::read_to_string(rules_path(vault_root)).ok();
    build_rules(RULES_FILE, text.as_deref(), &known_commands())
        .map(|(rules, _)| rules)
        .unwrap_or_else(|_| RuleSet::defaults())
}

fn load_rules_doc(path: &Path) -> Result<DocumentMut, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => text.parse::<DocumentMut>().map_err(|e| e.to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(DocumentMut::new()),
        Err(error) => Err(error.to_string()),
    }
}

fn rule_ids(doc: &DocumentMut) -> Vec<String> {
    doc.get("rule")
        .and_then(Item::as_array_of_tables)
        .map(|rules| {
            rules
                .iter()
                .filter_map(|rule| rule.get("id")?.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// `user.key.<command>`, or the first `~2`, `~3`… that the file doesn't
/// use yet.
fn free_rule_id(doc: &DocumentMut, command: &str) -> String {
    let taken = rule_ids(doc);
    let base = user_rule_id(command);
    std::iter::once(base.clone())
        .chain((2..).map(|n| format!("{base}~{n}")))
        .find(|id| !taken.contains(id))
        .unwrap_or(base)
}

fn rules_mut(doc: &mut DocumentMut) -> Result<&mut ArrayOfTables, String> {
    if doc.get("rule").is_none() {
        doc.insert("rule", Item::ArrayOfTables(ArrayOfTables::new()));
    }
    doc.get_mut("rule")
        .and_then(Item::as_array_of_tables_mut)
        .ok_or_else(|| "`rule` in rules.toml isn't a list of [[rule]] tables".to_string())
}

/// Checks the rules file still loads, saves it and returns the rules now
/// in effect.
fn save_rules(path: &Path, doc: &DocumentMut) -> Result<RuleSet, String> {
    let text = doc.to_string();
    let (rules, _) = build_rules(RULES_FILE, Some(&text), &known_commands()).map_err(|errors| {
        errors.first().map_or_else(
            || "the rules file doesn't load".to_string(),
            |d| d.message.clone(),
        )
    })?;
    save(path, &text).map_err(|error| error.to_string())?;
    Ok(rules)
}

/// Adds a key rule running `command` on `chord` (portable, such as
/// `Mod+Shift+K`). Returns the new rule's id and the rules now in effect.
pub fn add_user_key(
    vault_root: &Path,
    command: &str,
    chord: &str,
) -> Result<(String, RuleSet), String> {
    let path = rules_path(vault_root);
    let mut doc = load_rules_doc(&path)?;
    let id = free_rule_id(&doc, command);
    let mut rule = Table::new();
    rule.insert("id", value(id.as_str()));
    rule.insert("on", value("key"));
    rule.insert("keys", value(chord));
    rule.insert("do", value(command));
    rules_mut(&mut doc)?.push(rule);
    let rules = save_rules(&path, &doc)?;
    Ok((id, rules))
}

/// The comment lines written above a `[[rule]]`, which removing the rule
/// must not lose.
fn comments_above(rule: &Table) -> String {
    rule.decor()
        .prefix()
        .and_then(|prefix| prefix.as_str())
        .filter(|prefix| prefix.contains('#'))
        .unwrap_or_default()
        .to_string()
}

/// Removes every rule with id `rule_id` from the vault's rules file. Any
/// comments above a removed rule move to the rule after it.
pub fn remove_user_key(vault_root: &Path, rule_id: &str) -> Result<RuleSet, String> {
    let path = rules_path(vault_root);
    let mut doc = load_rules_doc(&path)?;
    let rules = rules_mut(&mut doc)?;
    let mut kept_comments = String::new();
    rules.retain(|rule| {
        let remove = rule.get("id").and_then(Item::as_str) == Some(rule_id);
        if remove {
            kept_comments.push_str(&comments_above(rule));
        }
        !remove
    });
    let orphaned = match rules.iter_mut().next() {
        Some(next) if !kept_comments.is_empty() => {
            let prefix = format!("{kept_comments}{}", comments_above(next));
            next.decor_mut().set_prefix(prefix);
            None
        }
        _ => Some(kept_comments),
    };
    if let Some(comments) = orphaned.filter(|comments| !comments.is_empty()) {
        doc.set_trailing(comments.trim_start());
    }
    if doc
        .get("rule")
        .and_then(Item::as_array_of_tables)
        .is_some_and(|r| r.is_empty())
    {
        doc.remove("rule");
    }
    save_rules(&path, &doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor_config::Platform;
    use editor_config::keys::KeyChord;
    use std::fs;

    const THEME: &str = "\
# Warm paper.
[color]
background = \"#fdf6e3\"   # solarized base3

[font]
code = \"Iosevka\"
";

    fn vault(file: &str, text: Option<&str>) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        if let Some(text) = text {
            fs::create_dir_all(dir.path().join(CONFIG_DIR)).unwrap();
            fs::write(dir.path().join(CONFIG_DIR).join(file), text).unwrap();
        }
        dir
    }

    fn read(dir: &tempfile::TempDir, file: &str) -> String {
        fs::read_to_string(dir.path().join(CONFIG_DIR).join(file)).unwrap_or_default()
    }

    #[test]
    fn theme_tokens_are_written_beside_the_users_own() {
        let dir = vault(THEME_FILE, Some(THEME));
        let tokens = write_theme_token(dir.path(), "font.text", Some("Georgia")).unwrap();
        assert_eq!(tokens.text("font.text"), Some("Georgia"));
        assert_eq!(tokens.text("font.code"), Some("Iosevka"));
        let text = read(&dir, THEME_FILE);
        assert!(
            text.starts_with(
                "# Warm paper.\n[color]\nbackground = \"#fdf6e3\"   # solarized base3\n"
            ),
            "{text}"
        );
        assert!(
            text.contains("[font]\ncode = \"Iosevka\"\ntext = \"Georgia\"\n"),
            "{text}"
        );
    }

    #[test]
    fn the_built_in_value_removes_the_token() {
        let dir = vault(THEME_FILE, Some(THEME));
        write_theme_token(dir.path(), "color.accent", Some("#2f5fd0")).unwrap();
        assert!(read(&dir, THEME_FILE).contains("accent = \"#2f5fd0\""));
        // The built-in accent is `{color.black}`, which resolves to #000000.
        let tokens = write_theme_token(dir.path(), "color.accent", Some("#000000")).unwrap();
        assert_eq!(tokens.text("color.accent"), Some("#000000"));
        assert!(!read(&dir, THEME_FILE).contains("accent"));
        write_theme_token(dir.path(), "font.code", Some("Courier New")).unwrap();
        let text = read(&dir, THEME_FILE);
        assert!(!text.contains("[font]"), "{text}");
        assert!(text.contains("background = \"#fdf6e3\""));
    }

    #[test]
    fn a_missing_theme_file_is_created() {
        let dir = vault(THEME_FILE, None);
        write_theme_token(dir.path(), "font.ui", Some("Inter")).unwrap();
        assert_eq!(read(&dir, THEME_FILE), "[font]\nui = \"Inter\"\n");
        assert_eq!(load_tokens(dir.path()).text("font.ui"), Some("Inter"));
    }

    #[test]
    fn a_broken_reference_is_not_written() {
        let dir = vault(THEME_FILE, Some(THEME));
        let result = write_theme_token(dir.path(), "color.accent", Some("{color.nope}"));
        assert!(result.is_err());
        assert_eq!(read(&dir, THEME_FILE), THEME);
    }

    const RULES: &str = "\
# My keys.
[[rule]]
id   = \"my.bold\"
on   = \"key\"
keys = \"Mod+Alt+B\"
do   = \"format.bold\"
";

    #[test]
    fn user_keys_are_added_and_removed() {
        let dir = vault(RULES_FILE, Some(RULES));
        let (id, rules) = add_user_key(dir.path(), "tab.new", "Mod+Shift+J").unwrap();
        assert_eq!(id, "user.key.tab.new");
        let chord = KeyChord::parse("Mod+Shift+J")
            .unwrap()
            .resolve(Platform::Linux);
        assert!(rules.keys_for("tab.new", Platform::Linux).contains(&chord));
        let text = read(&dir, RULES_FILE);
        assert!(text.starts_with(RULES), "{text}");
        assert!(text.contains("id = \"user.key.tab.new\""), "{text}");
        let (second, _) = add_user_key(dir.path(), "tab.new", "F7").unwrap();
        assert_eq!(second, "user.key.tab.new~2");
        let rules = remove_user_key(dir.path(), "user.key.tab.new").unwrap();
        assert!(!rules.keys_for("tab.new", Platform::Linux).contains(&chord));
        assert_eq!(rules.keys_for("tab.new", Platform::Linux).len(), 2);
        remove_user_key(dir.path(), "user.key.tab.new~2").unwrap();
        assert_eq!(read(&dir, RULES_FILE).trim_end(), RULES.trim_end());
    }

    #[test]
    fn removing_the_last_rule_leaves_an_empty_file() {
        let dir = vault(RULES_FILE, None);
        add_user_key(dir.path(), "note.new", "Mod+Alt+N").unwrap();
        remove_user_key(dir.path(), "user.key.note.new").unwrap();
        assert_eq!(read(&dir, RULES_FILE).trim(), "");
        assert_eq!(load_rules(dir.path()), RuleSet::defaults());
    }
}
