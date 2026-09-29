//! Inline content: text, emphasis, links, code, math, footnotes, images and
//! the drop cap on the first paragraph.

use pulldown_cmark::{Event, LinkType, Tag};

use super::{ActiveDropCap, Converter, DropCap, MathSite, Open, OpenKind};
use crate::pdf::escape;
use crate::pdf::images::{ResolvedImage, embed_width};

/// Typst opener for inline Markdown constructs; every one closes with `];`.
pub(super) fn opener(tag: &Tag<'_>) -> Option<String> {
    let opener = match tag {
        Tag::Emphasis => "#emph[".to_owned(),
        Tag::Strong => "#strong[".to_owned(),
        Tag::Strikethrough => "#strike[".to_owned(),
        Tag::Superscript => "#super[".to_owned(),
        Tag::Subscript => "#sub[".to_owned(),
        Tag::Link {
            link_type,
            dest_url,
            ..
        } => link_opener(*link_type, dest_url),
        _ => return None,
    };
    Some(opener)
}

fn link_opener(link_type: LinkType, destination: &str) -> String {
    let external = destination.contains("://") || destination.starts_with("mailto:");
    match link_type {
        LinkType::Email => format!(
            "#link({})[",
            escape::string(&format!("mailto:{destination}"))
        ),
        LinkType::WikiLink { .. } => "#underline[".to_owned(),
        _ if external => format!("#link({})[", escape::string(destination)),
        _ => "#underline[".to_owned(),
    }
}

/// Characters shown as part of the drop cap before the letter itself.
fn is_leading_punctuation(character: char) -> bool {
    matches!(character, '"' | '\'' | '“' | '‘' | '«' | '(' | '[')
}

/// Splits the drop cap letter (with any opening punctuation) off `text`.
fn split_letter(text: &str) -> Option<(&str, &str)> {
    let letter_start = text.find(|c: char| !is_leading_punctuation(c))?;
    let letter = text[letter_start..].chars().next()?;
    if !letter.is_alphanumeric() {
        return None;
    }
    Some(text.split_at(letter_start + letter.len_utf8()))
}

impl<'a> Converter<'a> {
    pub(super) fn open_inline(&mut self, opener: String) {
        self.before_inline();
        self.out.push_str(&opener);
        self.open.push(Open {
            kind: OpenKind::Inline(opener),
            closer: "];".to_owned(),
        });
    }

    /// Writes inline markup that is not plain text.
    pub(super) fn inline_markup(&mut self, markup: &str) {
        self.before_inline();
        self.out.push_str(markup);
    }

    /// Called before any non-text inline content: a paragraph that does not
    /// open with text gets no drop cap.
    fn before_inline(&mut self) {
        if let DropCap::Active(active) = &mut self.drop_cap
            && active.letter.is_none()
            && self.footnote_depth == 0
        {
            active.aborted = true;
        }
    }

    pub(super) fn text(&mut self, text: &str) {
        let mut pieces = text.split("==");
        if let Some(first) = pieces.next() {
            self.plain_text(first);
        }
        for piece in pieces {
            self.toggle_highlight();
            self.plain_text(piece);
        }
    }

    fn toggle_highlight(&mut self) {
        let closes = self
            .open
            .last()
            .is_some_and(|open| open.kind == OpenKind::Highlight);
        if closes {
            self.open.pop();
            self.out.push_str("];");
            return;
        }
        self.before_inline();
        self.out.push_str("#mark[");
        self.open.push(Open {
            kind: OpenKind::Highlight,
            closer: "];".to_owned(),
        });
    }

    fn plain_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let text = self.take_drop_cap_letter(text);
        if !self.records_splits() {
            self.out.push_str(&escape::markup(text));
            return;
        }
        for word in text.split_inclusive(' ') {
            self.out.push_str(&escape::markup(word));
            if word.ends_with(' ') {
                self.record_split();
            }
        }
    }

    fn take_drop_cap_letter<'t>(&mut self, text: &'t str) -> &'t str {
        let DropCap::Active(active) = &mut self.drop_cap else {
            return text;
        };
        let at_start = self.out.len() == active.start;
        if active.letter.is_some() || active.aborted || !at_start || self.footnote_depth > 0 {
            return text;
        }
        match split_letter(text) {
            Some((letter, rest)) => {
                active.letter = Some(letter.to_owned());
                rest
            }
            None => {
                active.aborted = true;
                text
            }
        }
    }

    fn records_splits(&self) -> bool {
        matches!(&self.drop_cap, DropCap::Active(active) if !active.aborted)
            && self.footnote_depth == 0
    }

    fn record_split(&mut self) {
        let DropCap::Active(active) = &mut self.drop_cap else {
            return;
        };
        let openers = self.open[active.base..]
            .iter()
            .map(|open| match &open.kind {
                OpenKind::Inline(opener) => opener.clone(),
                OpenKind::Html { opener, .. } => opener.clone(),
                OpenKind::Highlight => "#mark[".to_owned(),
                OpenKind::Markdown => "#[".to_owned(),
            })
            .collect();
        active.splits.push((self.out.len(), openers));
    }

    pub(super) fn inline_code(&mut self, code: &str) {
        self.inline_markup(&format!("#raw({});", escape::string(code)));
    }

    pub(super) fn math(&mut self, latex: &str, display: bool) {
        self.before_inline();
        let start = self.out.len();
        match gasp_math::latex_to_typst(latex, display) {
            Ok(equation) => {
                self.out
                    .push_str(&format!("#m({});", escape::string(&equation)));
                self.math.push(MathSite {
                    range: start..self.out.len(),
                    latex: latex.to_owned(),
                    display,
                });
            }
            Err(_) => {
                self.out.push_str(&math_error_markup(latex));
                self.unconverted_math.push(latex.to_owned());
            }
        }
    }

    /// Every reference gets its own footnote on the page that cites it,
    /// as in PDF Export Plus, which repeats the text of a footnote cited twice.
    pub(super) fn footnote_reference(&mut self, name: &str) {
        let definition = self
            .footnotes
            .get(name)
            .filter(|_| self.footnote_depth == 0);
        let Some(events) = definition.cloned() else {
            self.inline_markup(&escape::markup(&format!("[^{name}]")));
            return;
        };
        self.inline_markup("#footnote[");
        self.render_nested(events);
        let trimmed = self.out.trim_end().len();
        self.out.truncate(trimmed);
        self.out.push_str("];");
    }

    /// Renders footnote content in place, then resumes the main stream.
    fn render_nested(&mut self, events: Vec<Event<'a>>) {
        let saved_events = std::mem::replace(&mut self.events, events);
        let saved_pos = std::mem::replace(&mut self.pos, 0);
        let saved_depth = self.open.len();
        self.footnote_depth += 1;
        while let Some(event) = self.next_event() {
            self.event(event);
        }
        while self.open.len() > saved_depth {
            if let Some(open) = self.open.pop() {
                self.out.push_str(&open.closer);
            }
        }
        self.footnote_depth -= 1;
        self.events = saved_events;
        self.pos = saved_pos;
    }

    pub(super) fn image(&mut self, link_type: LinkType, destination: &str) {
        let mut alt = String::new();
        self.collect_until_end(|event| {
            if let Event::Text(text) = event {
                alt.push_str(&text);
            }
        });
        let size = match link_type {
            LinkType::WikiLink { has_pothole: true } => Some(alt.as_str()),
            LinkType::WikiLink { .. } => None,
            _ => alt.rsplit_once('|').map(|(_, size)| size),
        };
        let width = size.and_then(embed_width);
        let markup = self.image_markup(destination, width);
        self.inline_markup(&markup);
    }

    /// Markup for an image at `destination`, `width_px` CSS pixels wide.
    pub(super) fn image_markup(&mut self, destination: &str, width_px: Option<f64>) -> String {
        let name = escape::string(destination);
        match self.images.resolve(destination) {
            ResolvedImage::Found(path) => {
                let width =
                    width_px.map_or_else(String::new, |px| format!(", width: {}pt", px * 0.75));
                format!("#note-image({}{width});", escape::string(&path))
            }
            ResolvedImage::Missing => format!("#missing-image({name});"),
            ResolvedImage::NotAnImage => format!("#note-embed({name});"),
        }
    }

    pub(super) fn paragraph(&mut self) {
        let start = self.out.len();
        if let DropCap::Pending(lines) = self.drop_cap
            && self.open.is_empty()
            && self.footnote_depth == 0
        {
            self.drop_cap = DropCap::Active(ActiveDropCap {
                lines,
                start,
                base: 1,
                letter: None,
                aborted: false,
                splits: Vec::new(),
            });
        }
        self.push_open("", "\n\n");
    }

    pub(super) fn end_paragraph(&mut self, closer: &str) {
        let is_drop_cap_paragraph = matches!(&self.drop_cap, DropCap::Active(_))
            && self.open.is_empty()
            && self.footnote_depth == 0;
        if !is_drop_cap_paragraph {
            self.out.push_str(closer);
            return;
        }
        let DropCap::Active(active) = std::mem::take(&mut self.drop_cap) else {
            return;
        };
        if let (Some(letter), false) = (&active.letter, active.aborted) {
            let body = self.out.split_off(active.start);
            self.out.push_str(&drop_cap_markup(&active, letter, &body));
            self.shift_math_sites(active.start, &body);
        }
        self.out.push_str(closer);
    }

    /// Re-locates equations after the drop cap paragraph was rewritten.
    fn shift_math_sites(&mut self, paragraph_start: usize, body: &str) {
        let rewritten = &self.out[paragraph_start..];
        let mut search_from = 0;
        for site in self
            .math
            .iter_mut()
            .filter(|site| site.range.start >= paragraph_start)
        {
            let call = &body[site.range.start - paragraph_start..site.range.end - paragraph_start];
            if let Some(found) = rewritten[search_from..].find(call) {
                let start = paragraph_start + search_from + found;
                search_from += found + call.len();
                site.range = start..start + call.len();
            }
        }
    }
}

pub(super) fn math_error_markup(latex: &str) -> String {
    format!("#math-error({});", escape::string(latex))
}

fn drop_cap_markup(active: &ActiveDropCap, letter: &str, body: &str) -> String {
    let mut boundaries = vec![(0, Vec::new())];
    boundaries.extend(
        active
            .splits
            .iter()
            .map(|(offset, openers)| (offset - active.start, openers.clone())),
    );
    boundaries.push((body.len(), Vec::new()));
    let mut chunks = String::new();
    for pair in boundaries.windows(2) {
        let ((from, reopen), (to, still_open)) = (&pair[0], &pair[1]);
        if from == to {
            continue;
        }
        chunks.push('[');
        chunks.extend(reopen.iter().map(String::as_str));
        chunks.push_str(&body[*from..*to]);
        chunks.push_str(&"]".repeat(still_open.len()));
        chunks.push_str("], ");
    }
    format!(
        "#drop-cap(lines: {}, [{}], ({chunks}));",
        active.lines,
        escape::markup(letter)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_letter_after_punctuation() {
        assert_eq!(split_letter("Hello"), Some(("H", "ello")));
        assert_eq!(split_letter("“Quoted"), Some(("“Q", "uoted")));
        assert_eq!(split_letter(" space"), None);
        assert_eq!(split_letter("—dash"), None);
    }
}
