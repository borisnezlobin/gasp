//! Converts Obsidian's `hotkeys.json` into key rules.

use serde_json::Value;

use crate::toml_text::quote;

/// Obsidian command IDs and the commands they become.
const COMMANDS: &[(&str, &str)] = &[
    ("app:open-settings", "settings.open"),
    ("app:open-vault", "vault.open"),
    ("app:go-back", "history.back"),
    ("app:go-forward", "history.forward"),
    ("app:toggle-left-sidebar", "sidebar.files.toggle"),
    ("command-palette:open", "palette.open"),
    ("switcher:open", "switcher.open"),
    ("editor:toggle-bold", "format.bold"),
    ("editor:toggle-italics", "format.italic"),
    ("editor:toggle-strikethrough", "format.strikethrough"),
    ("editor:toggle-highlight", "format.highlight"),
    ("editor:toggle-code", "format.code"),
    ("editor:toggle-inline-math", "format.math-inline"),
    ("editor:toggle-comments", "format.comment"),
    ("editor:insert-link", "format.link"),
    ("editor:follow-link", "link.follow"),
    ("editor:open-search", "find.open"),
    ("editor:open-search-replace", "find.replace"),
    ("global-search:open", "search.open"),
    ("omnisearch:show-modal", "search.open"),
    ("file-explorer:new-file", "note.new"),
    ("file-explorer:open", "file-tree.focus"),
    ("workspace:new-tab", "tab.new"),
    ("workspace:close", "tab.close"),
    ("workspace:undo-close-pane", "tab.reopen"),
    ("workspace:next-tab", "tab.next"),
    ("workspace:previous-tab", "tab.previous"),
    ("workspace:export-pdf", "app.export"),
    (
        "musical-text-highlighter:toggle-sentence-highlighting",
        "prose.toggle-sentence-highlighting",
    ),
    (
        "obsidian-footnotes:insert-autonumbered-footnote",
        "footnote.insert-or-jump",
    ),
    ("footnotes-plus:insert-footnote", "footnote.insert-or-jump"),
    ("google-drive-sync:push", "sync.now"),
    ("pdf-footnotes:export-pdf-footnotes", "app.export"),
];

/// Plugins whose hotkeys are dropped, and why.
const DROPPED_PLUGINS: &[(&str, &str)] = &[(
    "obsidian-hider:",
    "Obsidian Hider is no longer installed (PLAN.md, Assumptions)",
)];

/// Bindings that move in the new default keymap: (command, old keys, new keys).
const MOVED: &[(&str, &str, &str)] = &[("app.export", "Mod+Shift+P", "Mod+Shift+S")];

/// Bindings the new default keymap adds on keys the old setup used differently.
const ADDED: &[(&str, &str)] = &[("Mod+Shift+P", "palette.open"), ("Mod+P", "app.print")];

const MODIFIER_ORDER: [(&str, &str); 6] = [
    ("Mod", "Mod"),
    ("Ctrl", "Ctrl"),
    ("Alt", "Alt"),
    ("Shift", "Shift"),
    ("Meta", "Cmd"),
    ("Win", "Win"),
];

const KEY_NAMES: &[(&str, &str)] = &[
    ("ArrowLeft", "Left"),
    ("ArrowRight", "Right"),
    ("ArrowUp", "Up"),
    ("ArrowDown", "Down"),
    (" ", "Space"),
    ("Esc", "Escape"),
];

/// One `[[rule]]` with `on = "key"`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyRule {
    pub keys: String,
    pub command: String,
}

#[derive(Clone, Debug, Default)]
pub struct HotkeyMigration {
    pub rules: Vec<KeyRule>,
    /// Obsidian command IDs with keys but no equivalent command.
    pub unmapped: Vec<(String, Vec<String>)>,
    /// Obsidian command IDs with an empty list: a default binding the owner removed.
    pub cleared: Vec<String>,
    pub notes: Vec<String>,
}

pub fn migrate(hotkeys_json: &str) -> Result<HotkeyMigration, String> {
    let hotkeys: Value = serde_json::from_str(hotkeys_json)
        .map_err(|error| format!("hotkeys.json isn't valid JSON: {error}"))?;
    let entries = hotkeys.as_object().ok_or("hotkeys.json isn't an object")?;
    let mut migration = HotkeyMigration::default();
    for (id, bindings) in entries {
        migration.add_command(id, bindings);
    }
    migration.add_new_defaults();
    Ok(migration)
}

impl HotkeyMigration {
    fn add_command(&mut self, id: &str, bindings: &Value) {
        let bindings = bindings.as_array().cloned().unwrap_or_default();
        if bindings.is_empty() {
            self.cleared.push(id.to_string());
            return;
        }
        if let Some((_, reason)) = DROPPED_PLUGINS.iter().find(|(p, _)| id.starts_with(p)) {
            self.notes.push(format!("Dropped `{id}`: {reason}."));
            return;
        }
        let chords: Vec<String> = bindings.iter().filter_map(chord).collect();
        let Some(command) = COMMANDS.iter().find(|(o, _)| *o == id).map(|(_, c)| *c) else {
            self.unmapped.push((id.to_string(), chords));
            return;
        };
        for keys in chords {
            let keys = self.moved_keys(command, keys);
            self.add_rule(keys, command, id);
        }
    }

    fn moved_keys(&mut self, command: &str, keys: String) -> String {
        match MOVED
            .iter()
            .find(|(c, from, _)| *c == command && *from == keys)
        {
            Some((_, from, to)) => {
                self.notes.push(format!(
                    "`{command}` moves from {from} to {to} (PLAN.md, Default keymap)."
                ));
                to.to_string()
            }
            None => keys,
        }
    }

    fn add_rule(&mut self, keys: String, command: &str, id: &str) {
        match self.rules.iter().find(|rule| rule.keys == keys) {
            Some(existing) if existing.command == command => {
                self.notes.push(format!(
                    "`{id}` on {keys} duplicates `{command}`, so it was merged."
                ));
            }
            Some(existing) => {
                let taken_by = existing.command.clone();
                self.notes.push(format!(
                    "`{id}` on {keys} was skipped because {keys} already runs `{taken_by}`."
                ));
            }
            None => self.rules.push(KeyRule {
                keys,
                command: command.to_string(),
            }),
        }
    }

    fn add_new_defaults(&mut self) {
        for (keys, command) in ADDED {
            if let Some(index) = self.rules.iter().position(|rule| rule.keys == *keys) {
                let replaced = self.rules.remove(index);
                self.notes.push(format!(
                    "{keys} now runs `{command}` instead of `{}` (PLAN.md, Default keymap).",
                    replaced.command
                ));
            }
            self.rules.push(KeyRule {
                keys: keys.to_string(),
                command: command.to_string(),
            });
        }
    }

    /// The rules as TOML, in the shared `[[rule]]` format.
    pub fn to_toml(&self) -> String {
        let mut out =
            String::from("# Key rules imported from Obsidian's hotkeys.json by editor-migrate.\n");
        for rule in &self.rules {
            out.push_str(&format!(
                "\n[[rule]]\non   = \"key\"\nkeys = {}\ndo   = {}\n",
                quote(&rule.keys),
                quote(&rule.command)
            ));
        }
        out
    }
}

/// Turns `{"modifiers": ["Alt", "Mod"], "key": "S"}` into `Mod+Alt+S`.
fn chord(binding: &Value) -> Option<String> {
    let key = binding.get("key")?.as_str()?;
    let modifiers: Vec<&str> = binding
        .get("modifiers")
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let mut parts: Vec<&str> = MODIFIER_ORDER
        .iter()
        .filter(|(obsidian, _)| modifiers.contains(obsidian))
        .map(|(_, ours)| *ours)
        .collect();
    let key_name = KEY_NAMES
        .iter()
        .find(|(obsidian, _)| *obsidian == key)
        .map_or_else(|| single_key(key), |(_, ours)| ours.to_string());
    parts.push(&key_name);
    Some(parts.join("+"))
}

fn single_key(key: &str) -> String {
    let mut chars = key.chars();
    match (chars.next(), chars.next()) {
        (Some(only), None) => only.to_uppercase().to_string(),
        _ => key.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chords_follow_the_modifier_order() {
        let binding = serde_json::json!({"modifiers": ["Shift", "Alt", "Mod"], "key": "s"});
        assert_eq!(chord(&binding).unwrap(), "Mod+Alt+Shift+S");
        let binding = serde_json::json!({"modifiers": ["Meta"], "key": "ArrowLeft"});
        assert_eq!(chord(&binding).unwrap(), "Cmd+Left");
    }

    #[test]
    fn export_moves_and_palette_takes_its_keys() {
        let json = r#"{"pdf-footnotes:export-pdf-footnotes": [{"modifiers": ["Mod","Shift"], "key": "P"}]}"#;
        let migration = migrate(json).unwrap();
        let rules: Vec<(&str, &str)> = migration
            .rules
            .iter()
            .map(|r| (r.keys.as_str(), r.command.as_str()))
            .collect();
        assert_eq!(
            rules,
            vec![
                ("Mod+Shift+S", "app.export"),
                ("Mod+Shift+P", "palette.open"),
                ("Mod+P", "app.print")
            ]
        );
    }

    #[test]
    fn unknown_commands_are_reported_not_guessed() {
        let json = r#"{"editor:save-file": [{"modifiers": ["Alt","Mod"], "key": "S"}], "markdown:toggle-preview": []}"#;
        let migration = migrate(json).unwrap();
        assert_eq!(
            migration.unmapped,
            vec![(
                "editor:save-file".to_string(),
                vec!["Mod+Alt+S".to_string()]
            )]
        );
        assert_eq!(
            migration.cleared,
            vec!["markdown:toggle-preview".to_string()]
        );
    }

    #[test]
    fn toml_uses_the_rule_format() {
        let migration =
            migrate(r#"{"app:open-settings": [{"modifiers": ["Mod"], "key": "\\"}]}"#).unwrap();
        let text = migration.to_toml();
        assert!(
            text.contains(
                "[[rule]]\non   = \"key\"\nkeys = \"Mod+\\\\\"\ndo   = \"settings.open\"\n"
            )
        );
        let parsed: toml::Table = toml::from_str(&text).unwrap();
        assert_eq!(parsed["rule"].as_array().unwrap().len(), 3);
    }
}
