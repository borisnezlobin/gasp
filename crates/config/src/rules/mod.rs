//! Rules bind events (keys, pointer, focus, typing, idle, files, window) to commands.

mod clock;
mod engine;
mod parse;

use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::commands::Args;
use crate::diagnostics::Diagnostic;
use crate::keys::KeyChord;
use crate::platform::{InputContext, Platform, PlatformFilter, filter_admits};

pub use clock::{Clock, ManualClock, SystemClock};
pub use engine::{Dispatch, Event, MatchContext, RuleEngine};
pub use parse::parse_duration;

/// The built-in rules, including the whole default keymap.
pub const DEFAULT_RULES: &str = include_str!("../../defaults/rules.toml");
const DEFAULT_RULES_FILE: &str = "defaults/rules.toml";

/// What a rule listens for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EventKind {
    #[serde(rename = "key")]
    Key,
    #[serde(rename = "pointer.enter")]
    PointerEnter,
    #[serde(rename = "pointer.leave")]
    PointerLeave,
    #[serde(rename = "focus")]
    Focus,
    #[serde(rename = "blur")]
    Blur,
    #[serde(rename = "typing")]
    Typing,
    #[serde(rename = "idle")]
    Idle,
    #[serde(rename = "file.open")]
    FileOpen,
    #[serde(rename = "window.resize")]
    WindowResize,
}

/// One compiled rule.
#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    /// Lets user files override or delete this rule.
    pub id: Option<String>,
    pub on: EventKind,
    /// The chord for key rules, with `Mod` still unresolved.
    pub keys: Option<KeyChord>,
    /// The UI target, such as `sidebar.files`. `None` matches any target.
    pub at: Option<String>,
    /// Delay before the command runs. The opposite event cancels it.
    pub after: Duration,
    pub platform: Option<PlatformFilter>,
    pub when: Option<InputContext>,
    /// Setting values that must hold, by dotted key.
    pub conditions: BTreeMap<String, toml::Value>,
    pub command: String,
    pub args: Args,
}

impl Rule {
    pub fn applies_on(&self, platform: Platform) -> bool {
        filter_admits(self.platform, platform)
    }

    /// The chord with `Mod` resolved, for key rules.
    pub fn chord_for(&self, platform: Platform) -> Option<KeyChord> {
        self.keys.map(|chord| chord.resolve(platform))
    }

    pub fn is_key(&self) -> bool {
        self.on == EventKind::Key
    }
}

/// An ordered list of rules. Later rules win when two key rules match equally well.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RuleSet {
    rules: Vec<Rule>,
}

impl RuleSet {
    /// The built-in rules. They are checked by tests, so this can't fail at runtime.
    pub fn defaults() -> RuleSet {
        RuleSet::from_toml(DEFAULT_RULES_FILE, DEFAULT_RULES).expect("built-in rules are valid")
    }

    /// Parses a standalone rules file. Deletions are not allowed here.
    pub fn from_toml(file: &str, text: &str) -> Result<RuleSet, Vec<Diagnostic>> {
        let mut set = RuleSet::default();
        let warnings = set.layer(file, text)?;
        if warnings.is_empty() {
            Ok(set)
        } else {
            Err(warnings)
        }
    }

    /// Applies a user file: new ids append, known ids replace, `delete = true` removes.
    /// Returns warnings on success; on error nothing changes.
    pub fn layer(&mut self, file: &str, text: &str) -> Result<Vec<Diagnostic>, Vec<Diagnostic>> {
        let entries = parse::parse_rules(file, text)?;
        let mut next = self.clone();
        let mut warnings = Vec::new();
        for entry in entries {
            if let Some(warning) = next.apply(entry, file, text) {
                warnings.push(warning);
            }
        }
        *self = next;
        Ok(warnings)
    }

    fn apply(&mut self, entry: parse::RuleEntry, file: &str, text: &str) -> Option<Diagnostic> {
        match entry {
            parse::RuleEntry::Add(rule) => {
                self.upsert(*rule);
                None
            }
            parse::RuleEntry::Delete { id, span } => {
                let before = self.rules.len();
                self.rules
                    .retain(|rule| rule.id.as_deref() != Some(id.as_str()));
                let message = format!("no rule with id `{id}` to delete");
                (before == self.rules.len())
                    .then(|| Diagnostic::warning(file, text, Some(span), message))
            }
        }
    }

    fn upsert(&mut self, rule: Rule) {
        let existing = rule
            .id
            .as_ref()
            .and_then(|id| self.rules.iter().position(|r| r.id.as_ref() == Some(id)));
        match existing {
            Some(index) => self.rules[index] = rule,
            None => self.rules.push(rule),
        }
    }

    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    pub fn get(&self, id: &str) -> Option<&Rule> {
        self.rules
            .iter()
            .find(|rule| rule.id.as_deref() == Some(id))
    }

    /// Key rules that apply on `platform`.
    pub fn key_rules(&self, platform: Platform) -> impl Iterator<Item = &Rule> {
        self.rules
            .iter()
            .filter(move |rule| rule.is_key() && rule.applies_on(platform))
    }

    /// The resolved chords that run `command` on `platform`, for the palette and menus.
    pub fn keys_for(&self, command: &str, platform: Platform) -> Vec<KeyChord> {
        self.key_rules(platform)
            .filter(|rule| rule.command == command)
            .filter_map(|rule| rule.chord_for(platform))
            .collect()
    }

    /// Commands named by rules that aren't in `known`.
    pub fn unknown_commands<'a>(&'a self, known: &'a [&str]) -> impl Iterator<Item = &'a Rule> {
        self.rules
            .iter()
            .filter(move |rule| !known.contains(&rule.command.as_str()))
    }
}
