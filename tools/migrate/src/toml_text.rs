//! Small helpers for writing TOML by hand, so output keeps a fixed layout.

/// A TOML basic (double-quoted) string with the needed escapes.
pub fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_backslashes_quotes_and_controls() {
        assert_eq!(quote("Mod+\\"), "\"Mod+\\\\\"");
        assert_eq!(quote("say \"hi\""), "\"say \\\"hi\\\"\"");
        assert_eq!(quote("a\u{1}"), "\"a\\u0001\"");
        let parsed: toml::Table =
            toml::from_str(&format!("x = {}", quote("\\a\"\n\u{1}"))).unwrap();
        assert_eq!(parsed["x"].as_str(), Some("\\a\"\n\u{1}"));
    }
}
