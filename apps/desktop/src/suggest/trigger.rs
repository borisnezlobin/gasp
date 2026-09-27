//! Spotting what the cursor is completing: a note name after `[[`, a
//! heading after `[[Note#`, a tag after `#`, or an emoji or symbol name
//! after `:`.

use std::ops::Range;

/// The characters a tag may contain after its `#`, as the parser reads
/// them.
pub fn is_tag_char(ch: char) -> bool {
    ch.is_alphanumeric() || matches!(ch, '_' | '-' | '/')
}

/// What is being completed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TriggerKind {
    /// A note name, after `[[` or `![[`.
    Note,
    /// A heading of `note` (the current note when empty), after `#`.
    Heading { note: String },
    /// A tag, after `#`.
    Tag,
    /// An emoji, symbol or emoticon name, after `:`. The replaced range
    /// starts at the colon.
    Emoji,
}

/// A completion the cursor is in the middle of.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trigger {
    pub kind: TriggerKind,
    /// What has been typed so far.
    pub query: String,
    /// The document range an accepted suggestion replaces: the query plus
    /// the rest of the name after the cursor when editing a link.
    pub replace: Range<usize>,
    /// Whether the link already has its closing `]]`, so accepting leaves
    /// it alone and steps over it.
    pub closed: bool,
    /// Where the closing `]]` ends, when `closed`.
    pub close_end: usize,
}

impl Trigger {
    /// Where the typed query starts, which the popover lines up with.
    pub fn query_start(&self) -> usize {
        self.replace.start
    }

    /// Where the popover lines up: the query, or a tag's `#`, since tag
    /// rows show it.
    pub fn anchor(&self) -> usize {
        match self.kind {
            TriggerKind::Tag => self.replace.start - 1,
            TriggerKind::Emoji => self.replace.start + 1,
            _ => self.replace.start,
        }
    }
}

/// The trigger around the cursor on a line split at the cursor into
/// `before` and `after`. `line_start` is the line's document offset.
pub fn find_trigger(before: &str, after: &str, line_start: usize) -> Option<Trigger> {
    wikilink_trigger(before, after, line_start)
        .or_else(|| tag_trigger(before, after, line_start))
        .or_else(|| emoji_trigger(before, after, line_start))
}

/// Longest emoji name worth completing.
const MAX_EMOJI_QUERY: usize = 40;

/// The characters of an emoji shortcode or symbol name.
fn is_emoji_name_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '+')
}

/// What may come right before the colon: nothing, a space, or an opening
/// bracket or quote. Anything else is a time (`10:30`), a URL scheme
/// (`https:`), a label (`Note:`) or math (`f:A`), which stay quiet.
fn opens_emoji(previous: Option<char>) -> bool {
    previous.is_none_or(|ch| ch.is_whitespace() || "([{\"'“‘".contains(ch))
}

fn emoji_trigger(before: &str, after: &str, line_start: usize) -> Option<Trigger> {
    let body_len: usize = before
        .chars()
        .rev()
        .take_while(|&ch| is_emoji_name_char(ch))
        .map(char::len_utf8)
        .sum();
    let query = &before[before.len() - body_len..];
    let head = before[..before.len() - body_len].strip_suffix(':')?;
    let starts_with_letter = query
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic());
    if !starts_with_letter || body_len > MAX_EMOJI_QUERY || !opens_emoji(head.chars().next_back()) {
        return None;
    }
    let rest: usize = after
        .chars()
        .take_while(|&ch| is_emoji_name_char(ch))
        .map(char::len_utf8)
        .sum();
    // A closing colon, as in `:smile:`, goes with the name.
    let closing = usize::from(after[rest..].starts_with(':'));
    let cursor = line_start + before.len();
    Some(Trigger {
        kind: TriggerKind::Emoji,
        query: query.to_owned(),
        replace: line_start + head.len()..cursor + rest + closing,
        closed: false,
        close_end: cursor,
    })
}

/// Longest query worth completing; past this the `[[` is surely not a link
/// being typed.
const MAX_QUERY: usize = 200;

fn wikilink_trigger(before: &str, after: &str, line_start: usize) -> Option<Trigger> {
    let open = before.rfind("[[")?;
    let inside = &before[open + 2..];
    if inside.contains("]]") || inside.contains('|') || inside.len() > MAX_QUERY {
        return None;
    }
    let cursor = line_start + before.len();
    let closing = closing_bracket(after);
    let tail = closing.map_or("", |close| &after[..close]);
    let (kind, query, start) = match inside.split_once('#') {
        Some((note, heading)) => (
            TriggerKind::Heading {
                note: note.to_owned(),
            },
            heading,
            cursor - heading.len(),
        ),
        None => (TriggerKind::Note, inside, line_start + open + 2),
    };
    let stops: &[char] = match kind {
        TriggerKind::Note => &['|', '#'],
        _ => &['|'],
    };
    let rest = tail.find(stops).unwrap_or(tail.len());
    Some(Trigger {
        kind,
        query: query.to_owned(),
        replace: start..cursor + rest,
        closed: closing.is_some(),
        close_end: closing.map_or(cursor, |close| cursor + close + 2),
    })
}

/// Where `]]` starts in `after`, when it closes the link the cursor is in
/// rather than a later one.
fn closing_bracket(after: &str) -> Option<usize> {
    let close = after.find("]]")?;
    let reopened = after[..close].contains("[[");
    (!reopened).then_some(close)
}

fn tag_trigger(before: &str, after: &str, line_start: usize) -> Option<Trigger> {
    let body_len: usize = before
        .chars()
        .rev()
        .take_while(|&ch| is_tag_char(ch))
        .map(char::len_utf8)
        .sum();
    let head = before[..before.len() - body_len].strip_suffix('#')?;
    let hash = head.len();
    if body_len == 0 {
        return None;
    }
    let opens_tag = head
        .chars()
        .next_back()
        .is_none_or(|previous| previous.is_whitespace() || "([{,;".contains(previous));
    let query = &before[hash + 1..];
    if !opens_tag || query.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let rest: usize = after
        .chars()
        .take_while(|&ch| is_tag_char(ch))
        .map(char::len_utf8)
        .sum();
    let cursor = line_start + before.len();
    Some(Trigger {
        kind: TriggerKind::Tag,
        query: query.to_owned(),
        replace: line_start + hash + 1..cursor + rest,
        closed: false,
        close_end: cursor,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(line: &str) -> Option<Trigger> {
        let cursor = line
            .find('|')
            .expect("the test line marks the cursor with |");
        let before = &line[..cursor];
        let after = &line[cursor + 1..];
        find_trigger(before, after, 100)
    }

    #[test]
    fn a_note_name_after_double_brackets() {
        let trigger = at("see [[Wav|").unwrap();
        assert_eq!(trigger.kind, TriggerKind::Note);
        assert_eq!(trigger.query, "Wav");
        assert_eq!(trigger.replace, 106..109);
        assert!(!trigger.closed);
        assert_eq!(at("![[|").unwrap().query, "");
    }

    #[test]
    fn editing_a_closed_link_replaces_only_the_name() {
        let trigger = at("[[Wa|ve#Heading|alias]] after").unwrap();
        assert_eq!(trigger.kind, TriggerKind::Note);
        assert_eq!(trigger.replace, 102..106);
        assert!(trigger.closed);
        assert_eq!(trigger.close_end, 100 + "[[Wave#Heading|alias]]".len());
    }

    #[test]
    fn headings_after_a_hash_in_a_link() {
        let trigger = at("[[Wave Packets#Gro|]]").unwrap();
        assert_eq!(
            trigger.kind,
            TriggerKind::Heading {
                note: "Wave Packets".into()
            }
        );
        assert_eq!(trigger.query, "Gro");
        assert_eq!(trigger.replace, 115..118);
        let own = at("[[#|").unwrap();
        assert_eq!(own.kind, TriggerKind::Heading { note: "".into() });
    }

    #[test]
    fn nothing_to_complete_in_an_alias_or_after_a_closed_link() {
        assert_eq!(find_trigger("[[Note|ali", "", 0), None);
        assert_eq!(at("[[Note]] and |"), None);
        assert_eq!(at("plain text|"), None);
    }

    #[test]
    fn a_link_closing_later_is_not_ours() {
        let trigger = at("[[Fo| and [[Bar]]").unwrap();
        assert!(!trigger.closed);
        assert_eq!(trigger.replace, 102..104);
    }

    #[test]
    fn tags_after_a_hash_at_a_word_start() {
        let trigger = at("text #phy|sics more").unwrap();
        assert_eq!(trigger.kind, TriggerKind::Tag);
        assert_eq!(trigger.query, "phy");
        assert_eq!(trigger.replace, 106..113);
        assert_eq!(at("(#a/b|").unwrap().query, "a/b");
    }

    #[test]
    fn not_tags() {
        assert_eq!(at("#|"), None, "a lone # may become a heading");
        assert_eq!(at("x#no|"), None);
        assert_eq!(at("#123|"), None);
        assert_eq!(at("## Heading|"), None);
        assert_eq!(at("“i|"), None, "a wide character before the word");
        assert_eq!(at("“#i|"), None, "a tag can't follow a quote");
        assert_eq!(at("é #tag|").unwrap().replace, 104..107);
    }

    #[test]
    fn emoji_names_after_a_colon() {
        let trigger = at("nice :smi|").unwrap();
        assert_eq!(trigger.kind, TriggerKind::Emoji);
        assert_eq!(trigger.query, "smi");
        assert_eq!(trigger.replace, 105..109);
        assert_eq!(trigger.anchor(), 106);
        assert_eq!(at(":a|").unwrap().query, "a");
        assert_eq!(at("(:thumbs|").unwrap().query, "thumbs");
        assert_eq!(at(":smi|le: more").unwrap().replace, 100..107);
    }

    #[test]
    fn colons_that_are_not_emoji() {
        assert_eq!(at("at 10:30|"), None, "after a digit");
        assert_eq!(at("at 10:a|"), None, "after a digit");
        assert_eq!(at(":|"), None, "needs a letter");
        assert_eq!(at(":1|"), None, "needs a letter first");
        assert_eq!(at("https:|"), None);
        assert_eq!(at("f:A|"), None, "math-like labels");
        assert_eq!(at("Note:todo|"), None);
        assert_eq!(at("::smile|"), None);
        assert_eq!(at(":smile: and|"), None);
    }
}
