//! Block constructs: quotes and callouts, code, lists, tables and HTML blocks.

use pulldown_cmark::{Alignment, CodeBlockKind, Event, Tag, TagEnd};

use super::Converter;
use super::callout::{default_title, header_line_end, is_blank_span, parse_header};
use crate::pdf::escape;

/// Splits a code fence info string into its language and an
/// `title:"…"` title (the Embedded Code Title plugin's syntax).
pub(crate) fn code_info(info: &str) -> (Option<String>, Option<String>) {
    let info = info.trim();
    let (lang, rest) = match info.split_once(char::is_whitespace) {
        Some((lang, rest)) => (lang, rest.trim()),
        None => (info, ""),
    };
    let (lang, rest) = if lang.starts_with("title:") {
        ("", info)
    } else {
        (lang, rest)
    };
    let title = rest.find("title:").map(|at| title_value(&rest[at + 6..]));
    let lang = (!lang.is_empty()).then(|| lang.to_owned());
    (lang, title.filter(|title| !title.is_empty()))
}

fn title_value(text: &str) -> String {
    let text = text.trim_start();
    for quote in ['"', '\''] {
        if let Some(rest) = text.strip_prefix(quote) {
            return rest.split(quote).next().unwrap_or_default().to_owned();
        }
    }
    text.split_whitespace()
        .next()
        .unwrap_or_default()
        .to_owned()
}

fn alignment_name(alignment: Alignment) -> &'static str {
    match alignment {
        Alignment::Center => "center",
        Alignment::Right => "right",
        Alignment::None | Alignment::Left => "left",
    }
}

impl<'a> Converter<'a> {
    pub(super) fn block_quote(&mut self) {
        let header = match (self.peek(0), self.peek(1)) {
            (Some(Event::Start(Tag::Paragraph)), Some(Event::Text(text))) => parse_header(text),
            _ => None,
        };
        let Some(header) = header else {
            self.push_open("#quote-block[", "];\n\n");
            return;
        };
        let title_start = self.pos + 1;
        if let Event::Text(text) = &mut self.events[title_start] {
            *text = text[header.rest_offset..].to_owned().into();
        }
        let title_end = header_line_end(&self.events, title_start);
        self.out.push_str("#callout(title: [");
        if is_blank_span(&self.events, title_start, title_end) {
            self.out
                .push_str(&escape::markup(&default_title(&header.kind)));
        } else {
            self.pos = title_start;
            while self.pos < title_end {
                let Some(event) = self.next_event() else {
                    break;
                };
                self.event(event);
            }
        }
        self.out.push_str("])[");
        self.push_open("", "];\n\n");
        let paragraph_ended = matches!(
            self.events.get(title_end),
            Some(Event::End(TagEnd::Paragraph))
        );
        self.pos = title_end + 1;
        if !paragraph_ended {
            self.paragraph();
        }
    }

    pub(super) fn code_block(&mut self, kind: &CodeBlockKind<'_>) {
        let mut source = String::new();
        self.collect_until_end(|event| {
            if let Event::Text(text) = event {
                source.push_str(&text);
            }
        });
        let (lang, title) = match kind {
            CodeBlockKind::Fenced(info) => code_info(info),
            CodeBlockKind::Indented => (None, None),
        };
        let mut call = String::from("#code-block(");
        if let Some(lang) = lang {
            call.push_str(&format!("lang: {}, ", escape::string(&lang)));
        }
        if let Some(title) = title {
            call.push_str(&format!("title: {}, ", escape::string(&title)));
        }
        call.push_str(&escape::string(
            source.strip_suffix('\n').unwrap_or(&source),
        ));
        call.push_str(");");
        self.block_markup(&call);
    }

    pub(super) fn html_block(&mut self) {
        let mut html = String::new();
        self.collect_until_end(|event| {
            if let Event::Html(text) | Event::Text(text) = event {
                html.push_str(&text);
            }
        });
        let depth = self.open.len();
        self.html(&html);
        while self.open.len() > depth {
            if let Some(open) = self.open.pop() {
                self.out.push_str(&open.closer);
            }
        }
        self.out.push_str("\n\n");
    }

    pub(super) fn list(&mut self, start: Option<u64>) {
        let depth = self.list_counters.len();
        self.list_counters.push(start);
        self.push_open(&format!("#md-list(depth: {depth},\n"), ");\n\n");
    }

    pub(super) fn item(&mut self) {
        let marker = self.take_task_marker().map_or_else(
            || self.next_list_marker(),
            |done| if done { "\"done\"" } else { "\"open\"" }.to_owned(),
        );
        self.push_open(&format!("({marker}, ["), "]),\n");
    }

    fn next_list_marker(&mut self) -> String {
        match self.list_counters.last_mut() {
            Some(Some(number)) => {
                *number += 1;
                (*number - 1).to_string()
            }
            _ => "\"bullet\"".to_owned(),
        }
    }

    /// Consumes the task marker of the item just started, in tight or loose
    /// lists.
    fn take_task_marker(&mut self) -> Option<bool> {
        let offset = match (self.peek(0), self.peek(1)) {
            (Some(Event::TaskListMarker(_)), _) => 0,
            (Some(Event::Start(Tag::Paragraph)), Some(Event::TaskListMarker(_))) => 1,
            _ => return None,
        };
        let index = self.pos + offset;
        match std::mem::replace(&mut self.events[index], Event::Text("".into())) {
            Event::TaskListMarker(done) => Some(done),
            _ => None,
        }
    }

    pub(super) fn table(&mut self, alignments: &[Alignment]) {
        let aligns: Vec<&str> = alignments.iter().copied().map(alignment_name).collect();
        let opener = format!("#md-table(aligns: ({},), ", aligns.join(", "));
        self.push_open(&opener, ");\n\n");
    }

    pub(super) fn table_part(&mut self, tag: &Tag<'_>) {
        match tag {
            Tag::TableHead => self.push_open("header: (", "), "),
            Tag::TableCell => self.push_open("[", "], "),
            _ => self.push_open("", ""),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_code_info() {
        assert_eq!(code_info("rust"), (Some("rust".into()), None));
        assert_eq!(
            code_info("bash title:\"count words.sh\""),
            (Some("bash".into()), Some("count words.sh".into()))
        );
        assert_eq!(code_info("title:'x.py'"), (None, Some("x.py".into())));
        assert_eq!(code_info(""), (None, None));
    }
}
