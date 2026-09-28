//! A note's YAML frontmatter, read into JSON for `read_note`.

use serde_json::Value;

/// The frontmatter block: its YAML and where the body starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frontmatter<'a> {
    pub yaml: &'a str,
    /// Byte offset just past the closing `---` line.
    pub body_start: usize,
}

/// The block between a first line of `---` and the next `---` (or `...`)
/// line, as Obsidian reads it.
pub fn split(text: &str) -> Option<Frontmatter<'_>> {
    let first_end = text.find('\n')?;
    if text[..first_end].trim_end() != "---" {
        return None;
    }
    let mut line_start = first_end + 1;
    while line_start <= text.len() {
        let line_end = text[line_start..]
            .find('\n')
            .map_or(text.len(), |at| line_start + at);
        let line = text[line_start..line_end].trim_end();
        if line == "---" || line == "..." {
            let body_start = (line_end + 1).min(text.len());
            return Some(Frontmatter {
                yaml: &text[first_end + 1..line_start],
                body_start,
            });
        }
        line_start = line_end + 1;
    }
    None
}

/// The frontmatter as JSON: `Ok(None)` when there is none, and the YAML
/// error in words when it doesn't parse.
pub fn parse(text: &str) -> Result<Option<Value>, String> {
    let Some(block) = split(text) else {
        return Ok(None);
    };
    if block.yaml.trim().is_empty() {
        return Ok(Some(Value::Object(Default::default())));
    }
    serde_yaml::from_str::<Value>(block.yaml)
        .map(Some)
        .map_err(|error| format!("the frontmatter isn't valid YAML: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn frontmatter_reads_as_json() {
        let text = "---\ntitle: Plan\ntags: [a, b]\nupdated: 2024-01-02\ncount: 3\n---\n# Body\n";
        let block = split(text).unwrap();
        assert_eq!(&text[block.body_start..], "# Body\n");
        assert_eq!(
            parse(text).unwrap(),
            Some(json!({"title": "Plan", "tags": ["a", "b"], "updated": "2024-01-02", "count": 3}))
        );
    }

    #[test]
    fn notes_without_frontmatter_have_none() {
        assert_eq!(parse("# Just a note\n---\n").unwrap(), None);
        assert_eq!(parse("").unwrap(), None);
        // An unclosed block isn't frontmatter.
        assert_eq!(parse("---\ntitle: x\n").unwrap(), None);
    }

    #[test]
    fn empty_and_broken_blocks() {
        assert_eq!(parse("---\n---\nbody").unwrap(), Some(json!({})));
        assert!(parse("---\ntitle: [unclosed\n---\n").is_err());
        let crlf = "---\r\na: 1\r\n---\r\nbody";
        assert_eq!(parse(crlf).unwrap(), Some(json!({"a": 1})));
        assert_eq!(&crlf[split(crlf).unwrap().body_start..], "body");
    }
}
