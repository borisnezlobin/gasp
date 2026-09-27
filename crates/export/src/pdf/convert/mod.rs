//! Markdown (with Obsidian extensions) to Typst markup.
//!
//! The note is parsed with pulldown-cmark into an event list. Every start
//! event pushes an [`Open`] entry whose closing text is written by the
//! matching end event, so the Typst output nests exactly like the Markdown.
//! Constructs that need their whole content at once (code, HTML blocks,
//! images) consume their events directly.

mod blocks;
mod callout;
mod html;
mod inline;

use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use super::images::ImageResolver;
use super::preprocess;

/// What to convert and how.
#[derive(Debug, Clone, Default)]
pub struct ConvertOptions {
    /// Folder of the note, for resolving embedded images.
    pub note_dir: Option<PathBuf>,
    /// Vault root, the last place images are looked up.
    pub vault_root: Option<PathBuf>,
    /// Adds a title heading with this text unless the frontmatter has a
    /// `title`, which wins. `None` leaves the title out.
    pub title: Option<String>,
    /// Lines spanned by the drop cap on the first paragraph; `None` or `0`
    /// turns it off.
    pub drop_cap_lines: Option<u32>,
}

/// A LaTeX equation in the generated markup, so compile errors inside it can
/// be traced back and the equation replaced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MathSite {
    /// Byte range of the `#m(...)` call in [`TypstBody::markup`].
    pub range: Range<usize>,
    pub latex: String,
    pub display: bool,
}

/// The Typst body of a note plus what is needed to compile it.
#[derive(Debug, Clone, Default)]
pub struct TypstBody {
    pub markup: String,
    /// Virtual image paths used in `markup` and the files they come from.
    pub images: Vec<(String, PathBuf)>,
    pub math: Vec<MathSite>,
    /// LaTeX that mitex could not convert, shown as source.
    pub unconverted_math: Vec<String>,
}

/// Converts a note's Markdown to the Typst markup of its body.
pub fn markdown_to_typst(markdown: &str, options: &ConvertOptions) -> TypstBody {
    let note = preprocess::clean(markdown);
    let mut converter = Converter::new(
        parse(&note.body),
        ImageResolver::new(options.note_dir.clone(), options.vault_root.clone()),
        options.drop_cap_lines.filter(|lines| *lines > 0),
    );
    let title = options.title.as_ref().map(|fallback| {
        note.frontmatter_title
            .clone()
            .unwrap_or_else(|| fallback.clone())
    });
    if let Some(title) = title.filter(|title| !title.trim().is_empty()) {
        converter.write_title(&title);
    }
    converter.run();
    TypstBody {
        markup: converter.out,
        images: converter.images.into_assets(),
        math: converter.math,
        unconverted_math: converter.unconverted_math,
    }
}

fn parse(source: &str) -> Vec<Event<'_>> {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_MATH
        | Options::ENABLE_WIKILINKS;
    merge_text(Parser::new_ext(source, options).collect())
}

/// Joins adjacent text events so patterns like `[!note]` or `==` are never
/// split across events.
fn merge_text(events: Vec<Event<'_>>) -> Vec<Event<'_>> {
    let mut merged: Vec<Event<'_>> = Vec::with_capacity(events.len());
    for event in events {
        if let (Some(Event::Text(previous)), Event::Text(next)) = (merged.last_mut(), &event) {
            *previous = format!("{previous}{next}").into();
            continue;
        }
        merged.push(event);
    }
    merged
}

/// How an open construct behaves when content is split or closed early.
#[derive(Debug, Clone, PartialEq, Eq)]
enum OpenKind {
    /// A Markdown block or inline construct closed by its end event.
    Markdown,
    /// An inline wrapper (`#emph[` …) that can be reopened when a paragraph
    /// is split for the drop cap.
    Inline(String),
    /// `==highlight==`, closed by the next `==`.
    Highlight,
    /// An HTML element, closed by its end tag or by the enclosing construct.
    Html { tag: String, opener: String },
}

#[derive(Debug, Clone)]
struct Open {
    kind: OpenKind,
    closer: String,
}

/// Drop cap bookkeeping for the first top-level paragraph.
#[derive(Debug, Default)]
enum DropCap {
    #[default]
    Off,
    Pending(u32),
    Active(ActiveDropCap),
}

#[derive(Debug)]
struct ActiveDropCap {
    lines: u32,
    /// Where the paragraph starts in the output.
    start: usize,
    /// Depth of the open stack inside the paragraph.
    base: usize,
    letter: Option<String>,
    aborted: bool,
    /// Word boundaries: output offset and the inline openers active there.
    splits: Vec<(usize, Vec<String>)>,
}

struct Converter<'a> {
    events: Vec<Event<'a>>,
    pos: usize,
    out: String,
    open: Vec<Open>,
    footnotes: HashMap<String, Vec<Event<'a>>>,
    footnote_depth: usize,
    list_counters: Vec<Option<u64>>,
    images: ImageResolver,
    math: Vec<MathSite>,
    unconverted_math: Vec<String>,
    drop_cap: DropCap,
}

impl<'a> Converter<'a> {
    fn new(events: Vec<Event<'a>>, images: ImageResolver, drop_cap: Option<u32>) -> Self {
        let (events, footnotes) = extract_footnotes(events);
        Self {
            events,
            pos: 0,
            out: String::new(),
            open: Vec::new(),
            footnotes,
            footnote_depth: 0,
            list_counters: Vec::new(),
            images,
            math: Vec::new(),
            unconverted_math: Vec::new(),
            drop_cap: drop_cap.map_or(DropCap::Off, DropCap::Pending),
        }
    }

    fn run(&mut self) {
        while let Some(event) = self.next_event() {
            self.event(event);
        }
        while let Some(open) = self.open.pop() {
            self.out.push_str(&open.closer);
        }
    }

    fn next_event(&mut self) -> Option<Event<'a>> {
        let event = self.events.get_mut(self.pos)?;
        self.pos += 1;
        Some(std::mem::replace(event, Event::SoftBreak))
    }

    fn peek(&self, offset: usize) -> Option<&Event<'a>> {
        self.events.get(self.pos + offset)
    }

    fn event(&mut self, event: Event<'a>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => self.text(&text),
            Event::Code(code) => self.inline_code(&code),
            Event::InlineMath(latex) => self.math(&latex, false),
            Event::DisplayMath(latex) => self.math(&latex, true),
            Event::Html(html) | Event::InlineHtml(html) => self.html(&html),
            Event::FootnoteReference(name) => self.footnote_reference(&name),
            Event::SoftBreak | Event::HardBreak => self.inline_markup("#linebreak();"),
            Event::Rule => self.block_markup("#hrule();"),
            Event::TaskListMarker(_) => {}
        }
    }

    fn start(&mut self, tag: Tag<'a>) {
        if let Some(opener) = inline::opener(&tag) {
            self.open_inline(opener);
            return;
        }
        match tag {
            Tag::Paragraph => self.paragraph(),
            Tag::Heading { level, .. } => {
                self.push_open(&format!("#heading(level: {})[", level as u8), "];\n\n");
            }
            Tag::BlockQuote(_) => self.block_quote(),
            Tag::CodeBlock(kind) => self.code_block(&kind),
            Tag::HtmlBlock => self.html_block(),
            Tag::List(start) => self.list(start),
            Tag::Item => self.item(),
            other => self.start_other(other),
        }
    }

    fn start_other(&mut self, tag: Tag<'a>) {
        match tag {
            Tag::Table(alignments) => self.table(&alignments),
            Tag::TableHead | Tag::TableRow | Tag::TableCell => self.table_part(&tag),
            Tag::Image {
                link_type,
                dest_url,
                ..
            } => self.image(link_type, &dest_url),
            Tag::MetadataBlock(_) => self.skip_to_end(),
            _ => self.push_open("", ""),
        }
    }

    fn end(&mut self, tag: TagEnd) {
        self.close_dangling();
        let Some(open) = self.open.pop() else {
            return;
        };
        match tag {
            TagEnd::Paragraph => self.end_paragraph(&open.closer),
            TagEnd::List(_) => {
                self.list_counters.pop();
                self.out.push_str(&open.closer);
            }
            _ => self.out.push_str(&open.closer),
        }
    }

    /// Closes highlights and HTML elements left open inside the construct
    /// that is ending.
    fn close_dangling(&mut self) {
        while let Some(open) = self.open.last() {
            if !matches!(open.kind, OpenKind::Highlight | OpenKind::Html { .. }) {
                return;
            }
            let closer = open.closer.clone();
            self.open.pop();
            self.out.push_str(&closer);
        }
    }

    fn push_open(&mut self, opener: &str, closer: &str) {
        self.out.push_str(opener);
        self.open.push(Open {
            kind: OpenKind::Markdown,
            closer: closer.to_owned(),
        });
    }

    /// Writes a block-level construct on its own.
    fn block_markup(&mut self, markup: &str) {
        self.out.push_str(markup);
        self.out.push_str("\n\n");
    }

    /// Consumes events up to the end of the construct just started.
    fn skip_to_end(&mut self) {
        self.collect_until_end(|_| {});
    }

    /// Feeds every event up to the matching end to `visit`, consuming them.
    fn collect_until_end(&mut self, mut visit: impl FnMut(Event<'a>)) {
        let mut depth = 0usize;
        while let Some(event) = self.next_event() {
            match &event {
                Event::Start(_) => depth += 1,
                Event::End(_) if depth == 0 => return,
                Event::End(_) => depth -= 1,
                _ => {}
            }
            visit(event);
        }
    }

    fn write_title(&mut self, title: &str) {
        self.out.push_str("#note-title[");
        self.out.push_str(&super::escape::markup(title));
        self.out.push_str("];\n\n");
    }
}

/// Removes footnote definitions from the event stream, keyed by name.
#[allow(clippy::type_complexity)]
fn extract_footnotes(events: Vec<Event<'_>>) -> (Vec<Event<'_>>, HashMap<String, Vec<Event<'_>>>) {
    let mut body = Vec::with_capacity(events.len());
    let mut footnotes = HashMap::new();
    let mut current: Option<(String, Vec<Event<'_>>, usize)> = None;
    for event in events {
        let Some((name, content, depth)) = current.as_mut() else {
            match event {
                Event::Start(Tag::FootnoteDefinition(name)) => {
                    current = Some((name.to_string(), Vec::new(), 0));
                }
                other => body.push(other),
            }
            continue;
        };
        match &event {
            Event::Start(_) => *depth += 1,
            Event::End(_) if *depth == 0 => {
                footnotes
                    .entry(std::mem::take(name))
                    .or_insert_with(|| std::mem::take(content));
                current = None;
                continue;
            }
            Event::End(_) => *depth -= 1,
            _ => {}
        }
        content.push(event);
    }
    (body, footnotes)
}
