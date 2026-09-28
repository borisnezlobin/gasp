//! Source clean-up before parsing: frontmatter and `%%comments%%` are
//! removed, the frontmatter `title` is kept for the title heading, and
//! curly quotes around HTML attribute values, which Smart Typography
//! writes, are made straight so the tags read as HTML.

/// A note with its frontmatter and comments removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CleanNote {
    pub body: String,
    pub frontmatter_title: Option<String>,
    /// The frontmatter's `description`, which the website shows under the
    /// title.
    pub frontmatter_description: Option<String>,
}

pub(crate) fn clean(source: &str) -> CleanNote {
    let (frontmatter, body) = split_frontmatter(source);
    CleanNote {
        body: editor_core::syntax::straighten_tag_quotes(&strip_comments(body)).into_owned(),
        frontmatter_title: frontmatter.and_then(|yaml| field(yaml, "title")),
        frontmatter_description: frontmatter.and_then(|yaml| field(yaml, "description")),
    }
}

/// Splits a leading `---` YAML block from the body.
fn split_frontmatter(source: &str) -> (Option<&str>, &str) {
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let Some(rest) = source
        .strip_prefix("---\n")
        .or_else(|| source.strip_prefix("---\r\n"))
    else {
        return (None, source);
    };
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        if matches!(line.trim_end(), "---" | "...") {
            return (Some(&rest[..offset]), &rest[offset + line.len()..]);
        }
        offset += line.len();
    }
    (None, source)
}

/// A one-line string field of the frontmatter.
fn field(yaml: &str, name: &str) -> Option<String> {
    yaml.lines().find_map(|line| {
        let value = line.strip_prefix(name)?.strip_prefix(':')?.trim();
        let value = value.trim_matches(|c| c == '"' || c == '\'').trim();
        (!value.is_empty()).then(|| value.to_owned())
    })
}

fn fence_marker(line: &str) -> Option<&str> {
    let trimmed = line.trim_start_matches(' ');
    if line.len() - trimmed.len() > 3 {
        return None;
    }
    let marker_char = trimmed.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let length = trimmed.chars().take_while(|c| *c == marker_char).count();
    (length >= 3).then(|| &trimmed[..length])
}

struct CommentStripper {
    in_comment: bool,
    fence: Option<String>,
    out: String,
}

impl CommentStripper {
    fn line(&mut self, line: &str) {
        if !self.in_comment && self.handle_fence(line) {
            self.out.push_str(line);
            return;
        }
        let kept = self.strip_line(line);
        let only_comment = kept.trim().is_empty() && !line.trim().is_empty();
        if !only_comment {
            self.out.push_str(&kept);
        }
    }

    /// Tracks fenced code; returns true when `line` belongs to a fence.
    fn handle_fence(&mut self, line: &str) -> bool {
        if let Some(open) = &self.fence {
            if fence_marker(line).is_some_and(|marker| marker.starts_with(open.as_str())) {
                self.fence = None;
            }
            return true;
        }
        if let Some(marker) = fence_marker(line) {
            self.fence = Some(marker.to_owned());
            return true;
        }
        false
    }

    fn strip_line(&mut self, line: &str) -> String {
        let mut kept = String::with_capacity(line.len());
        let mut rest = line;
        let mut in_code = false;
        while !rest.is_empty() {
            if self.in_comment {
                match rest.find("%%") {
                    Some(end) => {
                        rest = &rest[end + 2..];
                        self.in_comment = false;
                    }
                    None => {
                        kept.extend(rest.chars().filter(|c| *c == '\n'));
                        break;
                    }
                }
                continue;
            }
            let character = rest.chars().next().unwrap_or_default();
            if character == '`' {
                in_code = !in_code;
            } else if !in_code && rest.starts_with("%%") {
                self.in_comment = true;
                rest = &rest[2..];
                continue;
            }
            kept.push(character);
            rest = &rest[character.len_utf8()..];
        }
        kept
    }
}

/// Removes Obsidian `%%comments%%` outside code. Lines that held nothing but
/// a comment are dropped so a comment never splits a paragraph.
fn strip_comments(body: &str) -> String {
    let mut stripper = CommentStripper {
        in_comment: false,
        fence: None,
        out: String::with_capacity(body.len()),
    };
    for line in body.split_inclusive('\n') {
        stripper.line(line);
    }
    stripper.out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_frontmatter_and_keeps_title() {
        let note = clean("---\ntitle: \"A Title\"\nupdated: 2024\n---\nBody\n");
        assert_eq!(note.body, "Body\n");
        assert_eq!(note.frontmatter_title.as_deref(), Some("A Title"));
    }

    #[test]
    fn unterminated_frontmatter_is_body() {
        let note = clean("---\nnot closed\n");
        assert_eq!(note.body, "---\nnot closed\n");
        assert_eq!(note.frontmatter_title, None);
    }

    #[test]
    fn strips_inline_comment() {
        assert_eq!(clean("a %%hidden%% b\n").body, "a  b\n");
    }

    #[test]
    fn strips_block_comment_lines() {
        let note = clean("before\n\n%%\nsecret\nlines\n%%\n\nafter\n");
        assert_eq!(note.body, "before\n\n\nafter\n");
    }

    #[test]
    fn comment_line_does_not_split_paragraph() {
        assert_eq!(clean("one\n%%note%%\ntwo\n").body, "one\ntwo\n");
    }

    #[test]
    fn keeps_comments_in_code() {
        let source = "```\n%% kept %%\n```\n`%%also%%`\n";
        assert_eq!(clean(source).body, source);
    }
}
