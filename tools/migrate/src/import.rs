//! Importing a vault's `.obsidian` folder straight into its config folder,
//! as the app's "Import settings from Obsidian" does.
//!
//! What's already there wins. Settings and theme tokens the vault has set
//! stay as they are and only the missing ones are added; an imported key
//! rule on keys the vault already binds is left out; snippets and
//! replacements are written only when the vault has none, since a table
//! merged line by line would be neither.

use std::fs;
use std::path::Path;

use toml_edit::{ArrayOfTables, DocumentMut, Item, Table};

use crate::{
    Migration, OutputFile, REPORT_FILE, RULES_FILE, SETTINGS_FILE, THEME_FILE, migrate_obsidian,
};

/// What happened to each file the migration made.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportOutcome {
    /// New files in the config folder.
    pub written: Vec<&'static str>,
    /// Files the vault had, with what was missing added.
    pub merged: Vec<&'static str>,
    /// Files the vault had and that were left alone.
    pub kept: Vec<&'static str>,
}

impl ImportOutcome {
    /// Whether anything in the config folder changed.
    pub fn changed(&self) -> bool {
        !self.written.is_empty() || !self.merged.is_empty()
    }
}

/// Reads `obsidian` (a `.obsidian` folder) and imports it into `config`
/// (a vault's `.gasp` folder), with the report beside the other files.
pub fn import_into(obsidian: &Path, config: &Path) -> Result<ImportOutcome, String> {
    let migration = migrate_obsidian(obsidian)?;
    import_migration(&migration, config)
}

/// Imports an already-read migration into `config`.
pub fn import_migration(migration: &Migration, config: &Path) -> Result<ImportOutcome, String> {
    fs::create_dir_all(config)
        .map_err(|error| format!("can’t create {}: {error}", config.display()))?;
    let mut outcome = ImportOutcome::default();
    for file in migration.output_files() {
        let path = config.join(file.name);
        let existing = fs::read_to_string(&path).ok();
        let (text, fate) = combine(&file, existing.as_deref())?;
        if let Some(text) = text {
            fs::write(&path, text)
                .map_err(|error| format!("can’t write {}: {error}", path.display()))?;
        }
        match fate {
            Fate::Written => outcome.written.push(file.name),
            Fate::Merged => outcome.merged.push(file.name),
            Fate::Kept => outcome.kept.push(file.name),
        }
    }
    Ok(outcome)
}

enum Fate {
    Written,
    Merged,
    Kept,
}

/// The text to write for `file` given what the vault has, if anything
/// changes, and what became of it.
fn combine(file: &OutputFile, existing: Option<&str>) -> Result<(Option<String>, Fate), String> {
    let Some(existing) = existing.filter(|text| !text.trim().is_empty()) else {
        return Ok((Some(file.contents.clone()), Fate::Written));
    };
    let merged = match file.name {
        SETTINGS_FILE | THEME_FILE => merge_tables(existing, &file.contents)?,
        RULES_FILE => merge_rules(existing, &file.contents)?,
        REPORT_FILE => return Ok((Some(file.contents.clone()), Fate::Written)),
        _ => return Ok((None, Fate::Kept)),
    };
    if merged == existing {
        return Ok((None, Fate::Kept));
    }
    Ok((Some(merged), Fate::Merged))
}

fn parse(text: &str, what: &str) -> Result<DocumentMut, String> {
    text.parse::<DocumentMut>()
        .map_err(|error| format!("{what} isn’t valid TOML: {error}"))
}

/// `existing` with every key of `incoming` it lacks added, at any depth.
fn merge_tables(existing: &str, incoming: &str) -> Result<String, String> {
    let mut document = parse(existing, "the vault’s file")?;
    let incoming = parse(incoming, "the imported file")?;
    add_missing(document.as_table_mut(), incoming.as_table());
    Ok(document.to_string())
}

fn add_missing(into: &mut Table, from: &Table) {
    for (key, item) in from.iter() {
        match (into.get_mut(key), item) {
            (None, _) => {
                into.insert(key, item.clone());
            }
            (Some(Item::Table(inner)), Item::Table(incoming)) => add_missing(inner, incoming),
            _ => {}
        }
    }
}

/// `existing` with the imported key rules on keys it doesn't bind yet.
fn merge_rules(existing: &str, incoming: &str) -> Result<String, String> {
    let mut document = parse(existing, "the vault’s rules")?;
    let incoming = parse(incoming, "the imported rules")?;
    let taken: Vec<String> = rules(document.as_table())
        .map(|rules| rules.iter().filter_map(rule_keys).collect())
        .unwrap_or_default();
    let Some(new_rules) = rules(incoming.as_table()) else {
        return Ok(existing.to_owned());
    };
    let additions: Vec<Table> = new_rules
        .iter()
        .filter(|rule| rule_keys(rule).is_none_or(|keys| !taken.contains(&keys)))
        .cloned()
        .collect();
    if additions.is_empty() {
        return Ok(existing.to_owned());
    }
    let table = document.as_table_mut();
    if !matches!(table.get("rule"), Some(Item::ArrayOfTables(_))) {
        table.insert("rule", Item::ArrayOfTables(ArrayOfTables::new()));
    }
    if let Some(Item::ArrayOfTables(list)) = table.get_mut("rule") {
        for rule in additions {
            list.push(rule);
        }
    }
    Ok(document.to_string())
}

fn rules(table: &Table) -> Option<&ArrayOfTables> {
    table.get("rule").and_then(Item::as_array_of_tables)
}

/// The keys a key rule binds, in a form two spellings of one chord share.
fn rule_keys(rule: &Table) -> Option<String> {
    let keys = rule.get("keys")?.as_str()?;
    Some(keys.to_lowercase().replace(' ', ""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_setting_the_vault_has_wins_and_missing_ones_are_added() {
        let merged = merge_tables(
            "# mine\n[files]\ntrash = \"vault\"\n",
            "[files]\ntrash = \"system\"\nattachments-folder = \"./images\"\n\n[editor]\nshow-inline-title = true\n",
        )
        .unwrap();
        let parsed: toml::Table = toml::from_str(&merged).unwrap();
        assert_eq!(parsed["files"]["trash"].as_str(), Some("vault"));
        assert_eq!(
            parsed["files"]["attachments-folder"].as_str(),
            Some("./images")
        );
        assert_eq!(parsed["editor"]["show-inline-title"].as_bool(), Some(true));
        assert!(merged.starts_with("# mine"));
    }

    #[test]
    fn a_rule_on_keys_the_vault_binds_is_left_out() {
        let merged = merge_rules(
            "[[rule]]\non = \"key\"\nkeys = \"Mod+J\"\ndo = \"format.bold\"\n",
            "[[rule]]\non = \"key\"\nkeys = \"Mod+J\"\ndo = \"prose.toggle-sentence-highlighting\"\n\n[[rule]]\non = \"key\"\nkeys = \"Alt+0\"\ndo = \"footnote.insert-or-jump\"\n",
        )
        .unwrap();
        let parsed: toml::Table = toml::from_str(&merged).unwrap();
        let rules = parsed["rule"].as_array().unwrap();
        let commands: Vec<&str> = rules
            .iter()
            .map(|rule| rule["do"].as_str().unwrap())
            .collect();
        assert_eq!(commands, vec!["format.bold", "footnote.insert-or-jump"]);
    }

    #[test]
    fn snippets_the_vault_has_are_kept() {
        let file = OutputFile {
            name: crate::SNIPPETS_FILE,
            contents: "mk → $●$\n".to_owned(),
        };
        let (text, fate) = combine(&file, Some("mine → yours\n")).unwrap();
        assert!(text.is_none());
        assert!(matches!(fate, Fate::Kept));
        let (text, fate) = combine(&file, None).unwrap();
        assert_eq!(text.as_deref(), Some("mk → $●$\n"));
        assert!(matches!(fate, Fate::Written));
    }
}
