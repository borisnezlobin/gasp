//! Theme tokens: named colours, fonts, sizes, spacing, radii, shadows and curves.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use toml::{Table, Value};

use crate::merge::flatten;

/// A resolved token value.
#[derive(Clone, Debug, PartialEq)]
pub enum TokenValue {
    Text(String),
    Integer(i64),
    Float(f64),
}

impl TokenValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            TokenValue::Text(text) => Some(text),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            TokenValue::Integer(n) => Some(*n as f64),
            TokenValue::Float(n) => Some(*n),
            TokenValue::Text(_) => None,
        }
    }
}

impl fmt::Display for TokenValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenValue::Text(text) => f.write_str(text),
            TokenValue::Integer(n) => write!(f, "{n}"),
            TokenValue::Float(n) => write!(f, "{n}"),
        }
    }
}

/// Why a theme couldn't be resolved. `token` names the entry at fault.
#[derive(Clone, Debug, PartialEq)]
pub enum ThemeError {
    UnsupportedValue { token: String },
    UnknownReference { token: String, reference: String },
    UnclosedReference { token: String },
    Cycle { token: String, path: Vec<String> },
}

impl ThemeError {
    /// Every token involved, starting with the one at fault.
    pub fn tokens(&self) -> Vec<&str> {
        match self {
            ThemeError::Cycle { path, .. } => path.iter().map(String::as_str).collect(),
            _ => vec![self.token()],
        }
    }

    pub fn token(&self) -> &str {
        match self {
            ThemeError::UnsupportedValue { token }
            | ThemeError::UnknownReference { token, .. }
            | ThemeError::UnclosedReference { token }
            | ThemeError::Cycle { token, .. } => token,
        }
    }
}

impl fmt::Display for ThemeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ThemeError::UnsupportedValue { token } => {
                write!(f, "`{token}` must be a string or a number")
            }
            ThemeError::UnknownReference { token, reference } => {
                write!(f, "`{token}` refers to unknown token `{reference}`")
            }
            ThemeError::UnclosedReference { token } => {
                write!(f, "`{token}` has a `{{` without a matching `}}`")
            }
            ThemeError::Cycle { path, .. } => {
                write!(
                    f,
                    "tokens refer to each other in a loop: {}",
                    path.join(" -> ")
                )
            }
        }
    }
}

/// Unresolved tokens by dotted name, straight from TOML.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TokenSet(BTreeMap<String, Value>);

impl TokenSet {
    pub fn from_table(table: &Table) -> TokenSet {
        TokenSet(flatten(table).into_iter().collect())
    }

    /// Overrides tokens by name.
    pub fn layer(&mut self, overlay: TokenSet) {
        self.0.extend(overlay.0);
    }

    /// Resolves every reference, for both modes. Fails on unknown names,
    /// bad values or loops in either.
    pub fn resolve(&self) -> Result<Theme, ThemeError> {
        let dark = self.dark_tokens().resolve_one()?;
        let mut light = self.resolve_one()?;
        light.dark = Some(Arc::new(dark));
        Ok(light)
    }

    fn resolve_one(&self) -> Result<Theme, ThemeError> {
        let mut resolver = Resolver {
            raw: &self.0,
            done: BTreeMap::new(),
            stack: Vec::new(),
        };
        for name in self.0.keys() {
            resolver.resolve(name)?;
        }
        Ok(Theme {
            tokens: resolver.done,
            dark: None,
        })
    }

    /// The tokens in dark mode: each `dark.<name>` replaces `<name>`, so
    /// references such as `{color.gray-800}` follow the dark values.
    fn dark_tokens(&self) -> TokenSet {
        let mut dark = self.clone();
        for (name, value) in &self.0 {
            if let Some(base) = name.strip_prefix(DARK_PREFIX) {
                dark.0.insert(base.to_string(), value.clone());
            }
        }
        dark
    }
}

/// Tokens under this prefix replace their namesakes in dark mode.
pub const DARK_PREFIX: &str = "dark.";

/// A fully resolved theme.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Theme {
    tokens: BTreeMap<String, TokenValue>,
    /// The same tokens resolved for dark mode.
    dark: Option<Arc<Theme>>,
}

impl Theme {
    /// The tokens for light or dark mode. A theme built by hand, with no
    /// dark variant, is the same in both.
    pub fn for_mode(&self, dark: bool) -> &Theme {
        match (&self.dark, dark) {
            (Some(variant), true) => variant,
            _ => self,
        }
    }

    pub fn get(&self, name: &str) -> Option<&TokenValue> {
        self.tokens.get(name)
    }

    /// A token's value as a string, if it is one.
    pub fn text(&self, name: &str) -> Option<&str> {
        self.get(name).and_then(TokenValue::as_str)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.tokens.keys().map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.tokens.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }
}

struct Resolver<'a> {
    raw: &'a BTreeMap<String, Value>,
    done: BTreeMap<String, TokenValue>,
    stack: Vec<String>,
}

impl Resolver<'_> {
    fn resolve(&mut self, name: &str) -> Result<TokenValue, ThemeError> {
        if let Some(value) = self.done.get(name) {
            return Ok(value.clone());
        }
        self.enter(name)?;
        let raw = self.raw.get(name).cloned().unwrap_or(Value::Boolean(false));
        let value = self.resolve_value(name, &raw)?;
        self.stack.pop();
        self.done.insert(name.to_string(), value.clone());
        Ok(value)
    }

    fn enter(&mut self, name: &str) -> Result<(), ThemeError> {
        if let Some(start) = self.stack.iter().position(|entry| entry == name) {
            let mut path = self.stack[start..].to_vec();
            path.push(name.to_string());
            return Err(ThemeError::Cycle {
                token: self.stack[start].clone(),
                path,
            });
        }
        self.stack.push(name.to_string());
        Ok(())
    }

    fn resolve_value(&mut self, name: &str, value: &Value) -> Result<TokenValue, ThemeError> {
        match value {
            Value::String(text) => self.resolve_text(name, text),
            Value::Integer(n) => Ok(TokenValue::Integer(*n)),
            Value::Float(n) => Ok(TokenValue::Float(*n)),
            _ => Err(ThemeError::UnsupportedValue {
                token: name.to_string(),
            }),
        }
    }

    fn resolve_text(&mut self, name: &str, text: &str) -> Result<TokenValue, ThemeError> {
        if let Some(reference) = whole_reference(text) {
            return self.follow(name, reference);
        }
        let mut output = String::new();
        let mut rest = text;
        while let Some(open) = rest.find('{') {
            output.push_str(&rest[..open]);
            let close = rest[open..]
                .find('}')
                .ok_or_else(|| ThemeError::UnclosedReference {
                    token: name.to_string(),
                })?;
            let reference = &rest[open + 1..open + close];
            output.push_str(&self.follow(name, reference)?.to_string());
            rest = &rest[open + close + 1..];
        }
        output.push_str(rest);
        Ok(TokenValue::Text(output))
    }

    fn follow(&mut self, name: &str, reference: &str) -> Result<TokenValue, ThemeError> {
        let reference = reference.trim();
        if !self.raw.contains_key(reference) {
            return Err(ThemeError::UnknownReference {
                token: name.to_string(),
                reference: reference.to_string(),
            });
        }
        self.resolve(reference)
    }
}

fn whole_reference(text: &str) -> Option<&str> {
    let inner = text.strip_prefix('{')?.strip_suffix('}')?;
    (!inner.contains(['{', '}'])).then_some(inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(text: &str) -> TokenSet {
        TokenSet::from_table(&toml::from_str(text).unwrap())
    }

    #[test]
    fn dark_tokens_replace_their_namesakes() {
        let theme = tokens(
            "[color]\ngray = \"#eee\"\ntext = \"{color.gray}\"\nlink = \"#00f\"\n\
             [dark.color]\ngray = \"#222\"\n",
        )
        .resolve()
        .unwrap();
        assert_eq!(theme.text("color.text"), Some("#eee"));
        assert_eq!(theme.for_mode(false).text("color.text"), Some("#eee"));
        let dark = theme.for_mode(true);
        assert_eq!(dark.text("color.text"), Some("#222"));
        assert_eq!(dark.text("color.link"), Some("#00f"));
    }

    #[test]
    fn errors_in_dark_tokens_fail_the_theme() {
        let error = tokens("[color]\na = \"#000\"\n[dark.color]\na = \"{color.nope}\"\n")
            .resolve()
            .unwrap_err();
        assert!(matches!(error, ThemeError::UnknownReference { .. }));
    }

    #[test]
    fn whole_references_keep_their_type() {
        let theme = tokens("[space]\nmd = 8\ngap = \"{space.md}\"\n")
            .resolve()
            .unwrap();
        assert_eq!(theme.get("space.gap"), Some(&TokenValue::Integer(8)));
    }

    #[test]
    fn references_chain() {
        let theme = tokens(
            "[color]\nblack = \"#000\"\naccent = \"{color.black}\"\nlink = \"{color.accent}\"\n",
        )
        .resolve()
        .unwrap();
        assert_eq!(theme.text("color.link"), Some("#000"));
    }

    #[test]
    fn embedded_references_interpolate() {
        let theme =
            tokens("[color]\nshadow = \"black\"\n[shadow]\nsm = \"0 1px {color.shadow}\"\n")
                .resolve()
                .unwrap();
        assert_eq!(theme.text("shadow.sm"), Some("0 1px black"));
    }

    #[test]
    fn detects_cycles() {
        let error = tokens("[a]\nx = \"{a.y}\"\ny = \"{a.z}\"\nz = \"{a.x}\"\n")
            .resolve()
            .unwrap_err();
        let ThemeError::Cycle { path, .. } = error else {
            panic!("expected a cycle, got {error:?}");
        };
        assert_eq!(path, ["a.x", "a.y", "a.z", "a.x"]);
    }

    #[test]
    fn self_reference_is_a_cycle() {
        let error = tokens("[a]\nx = \"{a.x}\"\n").resolve().unwrap_err();
        assert!(matches!(error, ThemeError::Cycle { .. }));
    }

    #[test]
    fn unknown_references_fail() {
        let error = tokens("[a]\nx = \"{b.y}\"\n").resolve().unwrap_err();
        assert_eq!(
            error,
            ThemeError::UnknownReference {
                token: "a.x".into(),
                reference: "b.y".into()
            }
        );
    }

    #[test]
    fn arrays_are_not_tokens() {
        let error = tokens("[a]\nx = [1, 2]\n").resolve().unwrap_err();
        assert_eq!(error.token(), "a.x");
    }

    #[test]
    fn layering_overrides_by_name() {
        let mut base = tokens("[color]\nblack = \"#000\"\naccent = \"{color.black}\"\n");
        base.layer(tokens("[color]\naccent = \"#123456\"\n"));
        assert_eq!(
            base.resolve().unwrap().text("color.accent"),
            Some("#123456")
        );
    }
}
