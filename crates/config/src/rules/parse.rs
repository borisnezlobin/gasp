//! Turning `[[rule]]` tables into [`Rule`]s, with positioned diagnostics.

use std::collections::BTreeMap;
use std::ops::Range;
use std::time::Duration;

use serde::Deserialize;
use toml::Spanned;

use super::{EventKind, Rule};
use crate::commands::Args;
use crate::diagnostics::Diagnostic;
use crate::keys::KeyChord;
use crate::platform::{InputContext, PlatformFilter};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFile {
    #[serde(default)]
    rule: Vec<Spanned<RawRule>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRule {
    id: Option<Spanned<String>>,
    on: Option<Spanned<EventKind>>,
    keys: Option<Spanned<String>>,
    at: Option<String>,
    after: Option<Spanned<String>>,
    platform: Option<PlatformFilter>,
    when: Option<InputContext>,
    #[serde(rename = "if", default)]
    conditions: BTreeMap<String, toml::Value>,
    #[serde(rename = "do")]
    command: Option<Spanned<String>>,
    #[serde(default)]
    args: toml::Table,
    #[serde(default)]
    delete: bool,
}

/// One entry of a rules file.
pub(super) enum RuleEntry {
    Add(Box<Rule>),
    Delete { id: String, span: Range<usize> },
}

/// A problem with one field, before it is placed in a file.
struct FieldError {
    span: Range<usize>,
    message: String,
}

impl FieldError {
    fn new(span: Range<usize>, message: impl Into<String>) -> FieldError {
        FieldError {
            span,
            message: message.into(),
        }
    }
}

/// Parses every rule. Any error rejects the whole file.
pub(super) fn parse_rules(file: &str, text: &str) -> Result<Vec<RuleEntry>, Vec<Diagnostic>> {
    let raw: RawFile =
        toml::from_str(text).map_err(|error| vec![Diagnostic::from_toml(file, text, &error)])?;
    let mut entries = Vec::new();
    let mut errors = Vec::new();
    for rule in raw.rule {
        let span = rule.span();
        match compile(rule.into_inner(), span) {
            Ok(entry) => entries.push(entry),
            Err(e) => errors.push(Diagnostic::error(file, text, Some(e.span), e.message)),
        }
    }
    if errors.is_empty() {
        Ok(entries)
    } else {
        Err(errors)
    }
}

fn compile(raw: RawRule, span: Range<usize>) -> Result<RuleEntry, FieldError> {
    if raw.delete {
        let id = raw
            .id
            .ok_or_else(|| FieldError::new(span, "a rule with `delete = true` needs an `id`"))?;
        return Ok(RuleEntry::Delete {
            span: id.span(),
            id: id.into_inner(),
        });
    }
    compile_rule(raw, span).map(|rule| RuleEntry::Add(Box::new(rule)))
}

fn compile_rule(raw: RawRule, span: Range<usize>) -> Result<Rule, FieldError> {
    let on = raw
        .on
        .ok_or_else(|| FieldError::new(span.clone(), "a rule needs an `on` event"))?;
    let command = raw
        .command
        .ok_or_else(|| FieldError::new(span.clone(), "a rule needs a `do` command"))?;
    check_command(&command)?;
    let keys = compile_keys(&on, raw.keys)?;
    check_target(&on, raw.at.as_deref())?;
    Ok(Rule {
        id: raw.id.map(Spanned::into_inner),
        on: *on.get_ref(),
        keys,
        at: raw.at,
        after: raw
            .after
            .map(compile_delay)
            .transpose()?
            .unwrap_or_default(),
        platform: raw.platform,
        when: raw.when,
        conditions: raw.conditions,
        command: command.into_inner(),
        args: Args::from(raw.args),
    })
}

fn check_command(command: &Spanned<String>) -> Result<(), FieldError> {
    let valid = !command.get_ref().is_empty()
        && command
            .get_ref()
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-');
    if valid {
        return Ok(());
    }
    Err(FieldError::new(
        command.span(),
        "command ids are lowercase words joined by dots, like `format.bold`",
    ))
}

fn compile_keys(
    on: &Spanned<EventKind>,
    keys: Option<Spanned<String>>,
) -> Result<Option<KeyChord>, FieldError> {
    let is_key = *on.get_ref() == EventKind::Key;
    match (is_key, keys) {
        (true, Some(keys)) => KeyChord::parse(keys.get_ref())
            .map(Some)
            .map_err(|error| FieldError::new(keys.span(), error.to_string())),
        (true, None) => Err(FieldError::new(on.span(), "a key rule needs `keys`")),
        (false, Some(keys)) => Err(FieldError::new(
            keys.span(),
            "`keys` only applies to rules with `on = \"key\"`",
        )),
        (false, None) => Ok(None),
    }
}

const NEEDS_TARGET: &[EventKind] = &[EventKind::PointerEnter, EventKind::PointerLeave];

fn check_target(on: &Spanned<EventKind>, at: Option<&str>) -> Result<(), FieldError> {
    if at.is_none() && NEEDS_TARGET.contains(on.get_ref()) {
        return Err(FieldError::new(
            on.span(),
            "pointer rules need an `at` target",
        ));
    }
    Ok(())
}

const DURATION_UNITS: &[(&str, f64)] = &[("ms", 1.0), ("s", 1_000.0), ("m", 60_000.0)];

fn compile_delay(after: Spanned<String>) -> Result<Duration, FieldError> {
    parse_duration(after.get_ref()).ok_or_else(|| {
        FieldError::new(
            after.span(),
            format!(
                "`{}` isn’t a delay; write it like \"300ms\" or \"2s\"",
                after.get_ref()
            ),
        )
    })
}

/// Parses delays like `300ms`, `2s` or `1.5m`.
pub fn parse_duration(text: &str) -> Option<Duration> {
    let text = text.trim();
    DURATION_UNITS.iter().find_map(|(suffix, millis_per_unit)| {
        let amount: f64 = text.strip_suffix(suffix)?.trim().parse().ok()?;
        let millis = amount * millis_per_unit;
        (millis.is_finite() && millis >= 0.0).then(|| Duration::from_secs_f64(millis / 1_000.0))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error_for(text: &str) -> Diagnostic {
        match parse_rules("rules.toml", text) {
            Ok(_) => panic!("expected an error"),
            Err(errors) => errors.into_iter().next().unwrap(),
        }
    }

    #[test]
    fn parses_durations() {
        assert_eq!(parse_duration("300ms"), Some(Duration::from_millis(300)));
        assert_eq!(parse_duration("2s"), Some(Duration::from_secs(2)));
        assert_eq!(parse_duration("1.5s"), Some(Duration::from_millis(1500)));
        assert_eq!(parse_duration("1m"), Some(Duration::from_secs(60)));
        assert_eq!(parse_duration("soon"), None);
        assert_eq!(parse_duration("-5ms"), None);
    }

    #[test]
    fn bad_chord_points_at_the_keys_field() {
        let text = "[[rule]]\non = \"key\"\nkeys = \"Mod+Banana\"\ndo = \"format.bold\"\n";
        let error = error_for(text);
        assert_eq!((error.line, error.column), (3, 8));
        assert!(error.message.contains("Banana"));
    }

    #[test]
    fn unknown_event_points_at_on() {
        let error = error_for("[[rule]]\non = \"pointer.hover\"\ndo = \"x.y\"\n");
        assert_eq!(error.line, 2);
    }

    #[test]
    fn missing_command_is_an_error() {
        let error = error_for("[[rule]]\non = \"key\"\nkeys = \"Mod+B\"\n");
        assert!(error.message.contains("`do`"));
    }

    #[test]
    fn keys_only_belong_on_key_rules() {
        let error = error_for("[[rule]]\non = \"focus\"\nkeys = \"Mod+B\"\ndo = \"x.y\"\n");
        assert_eq!(error.line, 3);
    }

    #[test]
    fn pointer_rules_need_a_target() {
        let error = error_for("[[rule]]\non = \"pointer.enter\"\ndo = \"x.y\"\n");
        assert!(error.message.contains("`at`"));
    }

    #[test]
    fn bad_delay_is_reported() {
        let text = "[[rule]]\non = \"idle\"\nafter = \"later\"\ndo = \"x.y\"\n";
        assert_eq!(error_for(text).line, 3);
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let error = error_for("[[rule]]\non = \"idle\"\ndo = \"x.y\"\ncolour = 1\n");
        assert_eq!(error.line, 4);
    }

    #[test]
    fn delete_needs_an_id() {
        let error = error_for("[[rule]]\ndelete = true\n");
        assert!(error.message.contains("id"));
    }

    #[test]
    fn command_ids_are_lowercase() {
        let error = error_for("[[rule]]\non = \"idle\"\ndo = \"Format.Bold\"\n");
        assert_eq!(error.line, 3);
    }
}
