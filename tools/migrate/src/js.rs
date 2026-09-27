//! A tolerant parser for the JavaScript subset Latex Suite snippet files use: arrays,
//! objects with unquoted keys, strings, regex literals, numbers, `//` comments and
//! function values (kept as source text).

use std::fmt;
use std::ops::Range;

#[derive(Clone, Debug, PartialEq)]
pub enum JsValue {
    String(String),
    Regex {
        source: String,
        flags: String,
    },
    Number(f64),
    Bool(bool),
    Null,
    Array(Vec<Spanned>),
    Object(Vec<(String, Spanned)>),
    /// A function value, as its source text.
    Function(String),
}

/// A value with where it came from.
#[derive(Clone, Debug, PartialEq)]
pub struct Spanned {
    pub value: JsValue,
    /// 1-based line of the value's first character.
    pub line: usize,
    /// Character range in the source.
    pub span: Range<usize>,
    /// Line comments directly before this value, when it is an array element.
    pub comments: Vec<String>,
}

impl Spanned {
    /// Looks a key up when this is an object.
    pub fn get(&self, key: &str) -> Option<&Spanned> {
        match &self.value {
            JsValue::Object(fields) => fields.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsError {
    pub line: usize,
    pub column: usize,
    pub message: String,
}

impl fmt::Display for JsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "line {}, column {}: {}",
            self.line, self.column, self.message
        )
    }
}

impl std::error::Error for JsError {}

/// Parses one JavaScript value, such as the array in Latex Suite's `snippets` setting.
pub fn parse(text: &str) -> Result<Spanned, JsError> {
    let mut parser = Parser {
        chars: text.chars().collect(),
        pos: 0,
    };
    parser.skip_trivia();
    let value = parser.value(Vec::new())?;
    parser.skip_trivia();
    if parser.pos < parser.chars.len() {
        return Err(parser.error("unexpected text after the value"));
    }
    Ok(value)
}

/// Returns the source text of a character range.
pub fn slice(text: &str, span: &Range<usize>) -> String {
    text.chars()
        .skip(span.start)
        .take(span.end - span.start)
        .collect()
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    fn line_at(&self, pos: usize) -> usize {
        self.chars[..pos.min(self.chars.len())]
            .iter()
            .filter(|c| **c == '\n')
            .count()
            + 1
    }

    fn error(&self, message: &str) -> JsError {
        let before = &self.chars[..self.pos.min(self.chars.len())];
        let line_start = before.iter().rposition(|c| *c == '\n').map_or(0, |i| i + 1);
        JsError {
            line: self.line_at(self.pos),
            column: self.pos - line_start + 1,
            message: message.to_string(),
        }
    }

    /// Skips whitespace and comments, returning the text of line comments.
    fn skip_trivia(&mut self) -> Vec<String> {
        let mut comments = Vec::new();
        loop {
            match (self.peek(), self.peek_at(1)) {
                (Some(c), _) if c.is_whitespace() => self.pos += 1,
                (Some('/'), Some('/')) => comments.push(self.line_comment()),
                (Some('/'), Some('*')) => self.block_comment(),
                _ => return comments,
            }
        }
    }

    fn line_comment(&mut self) -> String {
        let start = self.pos + 2;
        while self.peek().is_some_and(|c| c != '\n') {
            self.pos += 1;
        }
        self.chars[start..self.pos]
            .iter()
            .collect::<String>()
            .trim()
            .to_string()
    }

    fn block_comment(&mut self) {
        self.pos += 2;
        while self.pos < self.chars.len()
            && !(self.peek() == Some('*') && self.peek_at(1) == Some('/'))
        {
            self.pos += 1;
        }
        self.pos = (self.pos + 2).min(self.chars.len());
    }

    fn value(&mut self, comments: Vec<String>) -> Result<Spanned, JsError> {
        let start = self.pos;
        let value = self.bare_value()?;
        Ok(Spanned {
            value,
            line: self.line_at(start),
            span: start..self.pos,
            comments,
        })
    }

    fn bare_value(&mut self) -> Result<JsValue, JsError> {
        match self.peek() {
            Some('[') => self.array(),
            Some('{') => self.object(),
            Some(quote @ ('"' | '\'' | '`')) => self.string(quote).map(JsValue::String),
            Some('/') => self.regex(),
            Some('(') => self.function(),
            Some(c) if c == '-' || c.is_ascii_digit() => self.number(),
            Some(c) if is_identifier_start(c) => self.word_value(),
            Some(_) => Err(self.error("expected a value")),
            None => Err(self.error("unexpected end of input")),
        }
    }

    fn array(&mut self) -> Result<JsValue, JsError> {
        self.pos += 1;
        let mut items = Vec::new();
        loop {
            let comments = self.skip_trivia();
            if self.peek() == Some(']') {
                self.pos += 1;
                return Ok(JsValue::Array(items));
            }
            items.push(self.value(comments)?);
            self.after_item(']')?;
        }
    }

    fn object(&mut self) -> Result<JsValue, JsError> {
        self.pos += 1;
        let mut fields = Vec::new();
        loop {
            self.skip_trivia();
            if self.peek() == Some('}') {
                self.pos += 1;
                return Ok(JsValue::Object(fields));
            }
            let key = self.key()?;
            self.skip_trivia();
            if self.peek() != Some(':') {
                return Err(self.error("expected `:` after the key"));
            }
            self.pos += 1;
            self.skip_trivia();
            fields.push((key, self.value(Vec::new())?));
            self.after_item('}')?;
        }
    }

    /// After an array item or object field: a comma, or the closing bracket (left unread).
    fn after_item(&mut self, close: char) -> Result<(), JsError> {
        self.skip_trivia();
        match self.peek() {
            Some(',') => {
                self.pos += 1;
                Ok(())
            }
            Some(c) if c == close => Ok(()),
            _ => Err(self.error(&format!("expected `,` or `{close}`"))),
        }
    }

    fn key(&mut self) -> Result<String, JsError> {
        match self.peek() {
            Some(quote @ ('"' | '\'')) => self.string(quote),
            Some(c) if is_identifier_start(c) => Ok(self.identifier()),
            _ => Err(self.error("expected a key")),
        }
    }

    fn identifier(&mut self) -> String {
        let start = self.pos;
        while self.peek().is_some_and(is_identifier_char) {
            self.pos += 1;
        }
        self.chars[start..self.pos].iter().collect()
    }

    fn word_value(&mut self) -> Result<JsValue, JsError> {
        let start = self.pos;
        let word = self.identifier();
        match word.as_str() {
            "true" => Ok(JsValue::Bool(true)),
            "false" => Ok(JsValue::Bool(false)),
            "null" | "undefined" => Ok(JsValue::Null),
            _ => {
                self.pos = start;
                self.function()
            }
        }
    }

    fn number(&mut self) -> Result<JsValue, JsError> {
        let start = self.pos;
        self.pos += 1;
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_digit() || c == '.' || c == 'e' || c == 'E')
        {
            self.pos += 1;
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        text.parse()
            .map(JsValue::Number)
            .map_err(|_| self.error("invalid number"))
    }

    fn string(&mut self, quote: char) -> Result<String, JsError> {
        self.pos += 1;
        let mut out = String::new();
        loop {
            match self.peek() {
                None => return Err(self.error("unterminated string")),
                Some(c) if c == quote => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some('\\') => {
                    self.pos += 1;
                    self.escape(&mut out)?;
                }
                Some(c) => {
                    out.push(c);
                    self.pos += 1;
                }
            }
        }
    }

    fn escape(&mut self, out: &mut String) -> Result<(), JsError> {
        let Some(c) = self.peek() else {
            return Err(self.error("unterminated escape"));
        };
        self.pos += 1;
        match c {
            'u' => out.push(self.unicode_escape()?),
            'x' => out.push(self.hex_digits(2)?),
            '\n' => {}
            other => out.push(simple_escape(other)),
        }
        Ok(())
    }

    fn unicode_escape(&mut self) -> Result<char, JsError> {
        if self.peek() != Some('{') {
            return self.hex_digits(4);
        }
        self.pos += 1;
        let start = self.pos;
        while self.peek().is_some_and(|c| c != '}') {
            self.pos += 1;
        }
        let digits: String = self.chars[start..self.pos].iter().collect();
        self.pos += 1;
        parse_code_point(&digits).ok_or_else(|| self.error("invalid unicode escape"))
    }

    fn hex_digits(&mut self, count: usize) -> Result<char, JsError> {
        let end = (self.pos + count).min(self.chars.len());
        let digits: String = self.chars[self.pos..end].iter().collect();
        self.pos = end;
        parse_code_point(&digits).ok_or_else(|| self.error("invalid hex escape"))
    }

    fn regex(&mut self) -> Result<JsValue, JsError> {
        self.pos += 1;
        let start = self.pos;
        let mut in_class = false;
        loop {
            match self.peek() {
                None | Some('\n') => return Err(self.error("unterminated regex literal")),
                Some('\\') => self.pos += 2,
                Some('[') => {
                    in_class = true;
                    self.pos += 1;
                }
                Some(']') => {
                    in_class = false;
                    self.pos += 1;
                }
                Some('/') if !in_class => break,
                Some(_) => self.pos += 1,
            }
        }
        let source = self.chars[start..self.pos].iter().collect();
        self.pos += 1;
        let flags = self.identifier();
        Ok(JsValue::Regex { source, flags })
    }

    /// Reads a function value up to the `,`, `}` or `]` that ends it.
    fn function(&mut self) -> Result<JsValue, JsError> {
        let start = self.pos;
        let mut depth = 0usize;
        while let Some(c) = self.peek() {
            if depth == 0 && matches!(c, ',' | '}' | ']') {
                break;
            }
            self.skip_function_char(c, &mut depth)?;
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        Ok(JsValue::Function(text.trim_end().to_string()))
    }

    fn skip_function_char(&mut self, c: char, depth: &mut usize) -> Result<(), JsError> {
        match c {
            '(' | '[' | '{' => *depth += 1,
            ')' | ']' | '}' => *depth = depth.saturating_sub(1),
            '"' | '\'' => {
                self.string(c)?;
                return Ok(());
            }
            '`' => {
                self.template()?;
                return Ok(());
            }
            '/' if matches!(self.peek_at(1), Some('/' | '*')) => {
                self.skip_trivia();
                return Ok(());
            }
            _ => {}
        }
        self.pos += 1;
        Ok(())
    }

    /// Skips a template literal, including `${...}` substitutions.
    fn template(&mut self) -> Result<(), JsError> {
        self.pos += 1;
        let mut substitution_depth = 0usize;
        loop {
            match (self.peek(), self.peek_at(1)) {
                (None, _) => return Err(self.error("unterminated template literal")),
                (Some('\\'), _) => self.pos += 2,
                (Some('$'), Some('{')) => {
                    substitution_depth += 1;
                    self.pos += 2;
                }
                (Some('}'), _) if substitution_depth > 0 => {
                    substitution_depth -= 1;
                    self.pos += 1;
                }
                (Some('`'), _) if substitution_depth == 0 => {
                    self.pos += 1;
                    return Ok(());
                }
                _ => self.pos += 1,
            }
        }
    }
}

fn simple_escape(c: char) -> char {
    match c {
        'n' => '\n',
        't' => '\t',
        'r' => '\r',
        'b' => '\u{8}',
        'f' => '\u{c}',
        'v' => '\u{b}',
        '0' => '\0',
        other => other,
    }
}

fn parse_code_point(digits: &str) -> Option<char> {
    u32::from_str_radix(digits, 16)
        .ok()
        .and_then(char::from_u32)
}

fn is_identifier_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '$'
}

fn is_identifier_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(text: &str) -> JsValue {
        parse(text).unwrap().value
    }

    #[test]
    fn strings_unescape_like_javascript() {
        assert_eq!(
            value(r#""\\frac{$0}\n\u00e9\x41\.""#),
            JsValue::String("\\frac{$0}\néA.".into())
        );
        assert_eq!(value("'it\\'s'"), JsValue::String("it's".into()));
    }

    #[test]
    fn regex_literals_keep_their_source() {
        assert_eq!(
            value(r"/(?<![\\A-Za-z])forall $/"),
            JsValue::Regex {
                source: r"(?<![\\A-Za-z])forall $".into(),
                flags: String::new()
            }
        );
        assert_eq!(
            value("/a[/]b/gi"),
            JsValue::Regex {
                source: "a[/]b".into(),
                flags: "gi".into()
            }
        );
    }

    #[test]
    fn objects_with_unquoted_keys_comments_and_trailing_commas() {
        let parsed = parse(
            "[\n  // Greek\n  {trigger: \"@a\", replacement: \"\\\\alpha\", options: \"mA\", priority: -1},\n  /* x */ {trigger: 'b',},\n]",
        )
        .unwrap();
        let JsValue::Array(items) = &parsed.value else {
            panic!("expected an array");
        };
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].comments, vec!["Greek".to_string()]);
        assert_eq!(items[0].line, 3);
        assert_eq!(
            items[0].get("priority").unwrap().value,
            JsValue::Number(-1.0)
        );
        assert_eq!(
            items[0].get("replacement").unwrap().value,
            JsValue::String("\\alpha".into())
        );
    }

    #[test]
    fn functions_are_kept_as_source() {
        let text = "{replacement: (match) => {\n const s = `a${match[1]}}`; return s.split(\",\");\n}, options: \"mA\"}";
        let parsed = parse(text).unwrap();
        let JsValue::Function(source) = &parsed.get("replacement").unwrap().value else {
            panic!("expected a function");
        };
        assert!(source.starts_with("(match) =>"));
        assert!(source.ends_with('}'));
        assert_eq!(
            parsed.get("options").unwrap().value,
            JsValue::String("mA".into())
        );

        let arrow = parse("[sel => sel.trim(), 2]").unwrap();
        let JsValue::Array(items) = arrow.value else {
            panic!("expected an array");
        };
        assert_eq!(
            items[0].value,
            JsValue::Function("sel => sel.trim()".into())
        );
    }

    #[test]
    fn errors_have_positions() {
        let error = parse("[\n  {trigger \"x\"}]").unwrap_err();
        assert_eq!((error.line, error.column), (2, 12));
        assert!(parse("\"open").is_err());
    }
}
