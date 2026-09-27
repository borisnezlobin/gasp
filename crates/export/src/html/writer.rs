//! Markdown (with Obsidian extensions) to article HTML.
//!
//! Like the PDF converter, every start event pushes an [`Open`] entry whose
//! closing markup the matching end event writes, and constructs that need
//! their whole content (code, images, callout titles) consume their events
//! directly. Equations are collected as they are met and typeset together
//! at the end, then spliced in at the offsets they were met at.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use base64::Engine as _;
use pulldown_cmark::{Alignment, CodeBlockKind, Event, LinkType, Tag, TagEnd};

use super::math::{Equation, to_mathml};
use super::raw::{self, Piece};
use super::{HtmlOptions, code, escape_attribute, escape_text, slugify};
use crate::pdf::convert::blocks::code_info;
use crate::pdf::convert::callout::{default_title, header_line_end, is_blank_span, parse_header};
use crate::pdf::convert::{extract_footnotes, opens_with_heading, parse};
use crate::pdf::images::{ImageResolver, Located, embed_width, image_extension};

/// What [`write`] produces.
pub(super) struct Output {
    pub html: String,
    pub math_css: Option<String>,
    pub failed_math: Vec<String>,
    pub missing_images: Vec<String>,
}

/// Converts a cleaned note body (no frontmatter or comments) to an
/// `<article>` element.
pub(super) fn write(
    body: &str,
    note_path: Option<&Path>,
    vault_root: Option<&Path>,
    options: &HtmlOptions,
    title: &str,
) -> Output {
    let (mut events, footnotes) = extract_footnotes(parse(body));
    // The website prints the title above the article, so a first heading
    // that repeats it would show the title twice.
    let repeats_title = opens_with_heading(&events, title);
    if repeats_title && !options.include_title {
        let heading_end = events
            .iter()
            .position(|event| matches!(event, Event::End(TagEnd::Heading(_))))
            .map_or(0, |end| end + 1);
        events.drain(..heading_end);
    }
    let resolver = ImageResolver::new(
        note_path.and_then(Path::parent).map(Path::to_path_buf),
        vault_root.map(Path::to_path_buf),
    );
    let mut writer = Writer::new(events, footnotes, resolver, options.inline_images);
    writer.out.push_str("<article>\n");
    if options.include_title && !repeats_title {
        writer
            .out
            .push_str(&format!("<h1>{}</h1>\n", escape_text(title)));
    }
    writer.run();
    writer.write_footnotes();
    writer.out.push_str("</article>\n");
    writer.finish()
}

/// How an open construct is closed.
#[derive(Debug, Clone, PartialEq, Eq)]
enum OpenKind {
    /// Closed by the Markdown end event.
    Markdown,
    /// `==highlight==`, closed by the next `==` or the end of the block.
    Mark,
    /// A raw HTML element, closed by its end tag or the enclosing construct.
    Html(String),
}

#[derive(Debug, Clone)]
struct Open {
    kind: OpenKind,
    closer: String,
}

/// Numbers and back-references of the footnotes cited so far.
#[derive(Debug, Default)]
struct Footnotes<'a> {
    definitions: HashMap<String, Vec<Event<'a>>>,
    /// Cited names in order of first citation; a footnote's number is its
    /// position here plus one.
    order: Vec<String>,
    /// How often each footnote has been cited.
    citations: HashMap<String, usize>,
}

struct Writer<'a> {
    events: Vec<Event<'a>>,
    pos: usize,
    out: String,
    open: Vec<Open>,
    footnotes: Footnotes<'a>,
    in_footnote: bool,
    images: ImageResolver,
    inline_images: bool,
    /// Encoded images by file, so an image shown twice is read once.
    data_uris: HashMap<PathBuf, String>,
    missing_images: Vec<String>,
    equations: Vec<Equation>,
    /// Output offsets where each equation goes.
    equation_at: Vec<usize>,
    heading_ids: HashSet<String>,
    /// A script or style element being skipped, until its end tag.
    hiding: Option<String>,
    alignments: Vec<Alignment>,
    cell: usize,
    in_table_head: bool,
}

const FIGURE_OPENER: &str = "<figure>";
const FIGURE_CLOSER: &str = "</figure>\n";

/// The MIME type of an image file extension.
fn mime_type(extension: &str) -> &'static str {
    match extension {
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        _ => "image/png",
    }
}

fn alignment_style(alignment: Alignment) -> &'static str {
    match alignment {
        Alignment::Center => " style=\"text-align: center\"",
        Alignment::Right => " style=\"text-align: right\"",
        Alignment::Left | Alignment::None => "",
    }
}

/// The inline Markdown elements and their tags.
fn inline_tag(tag: &Tag<'_>) -> Option<&'static str> {
    Some(match tag {
        Tag::Emphasis => "em",
        Tag::Strong => "strong",
        Tag::Strikethrough => "del",
        Tag::Superscript => "sup",
        Tag::Subscript => "sub",
        _ => return None,
    })
}

/// The `href` a link keeps on the website. Links to other notes have no
/// page there, so only web addresses and links within the note survive.
fn link_target(link_type: LinkType, destination: &str) -> Option<String> {
    if link_type == LinkType::Email {
        return Some(format!("mailto:{destination}"));
    }
    if destination.contains("://") || destination.starts_with("mailto:") {
        return Some(destination.to_owned());
    }
    let anchor = destination.strip_prefix('#')?;
    Some(format!("#{}", slugify(anchor)))
}

/// The id of a footnote's `count`th citation.
fn reference_id(number: usize, count: usize) -> String {
    if count == 1 {
        format!("fnref-{number}")
    } else {
        format!("fnref-{number}-{count}")
    }
}

impl<'a> Writer<'a> {
    fn new(
        events: Vec<Event<'a>>,
        definitions: HashMap<String, Vec<Event<'a>>>,
        images: ImageResolver,
        inline_images: bool,
    ) -> Self {
        Self {
            events,
            pos: 0,
            out: String::new(),
            open: Vec::new(),
            footnotes: Footnotes {
                definitions,
                ..Footnotes::default()
            },
            in_footnote: false,
            images,
            inline_images,
            data_uris: HashMap::new(),
            missing_images: Vec::new(),
            equations: Vec::new(),
            equation_at: Vec::new(),
            heading_ids: HashSet::new(),
            hiding: None,
            alignments: Vec::new(),
            cell: 0,
            in_table_head: false,
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
        let is_content = !matches!(
            event,
            Event::Start(_) | Event::End(_) | Event::Html(_) | Event::InlineHtml(_)
        );
        if self.hiding.is_some() && is_content {
            return;
        }
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => self.text(&text),
            Event::Code(code) => {
                self.out
                    .push_str(&format!("<code>{}</code>", escape_text(&code)));
            }
            Event::InlineMath(latex) => self.math(&latex, false),
            Event::DisplayMath(latex) => self.math(&latex, true),
            Event::Html(html) | Event::InlineHtml(html) => self.raw_html(&html),
            Event::FootnoteReference(name) => self.footnote_reference(&name),
            Event::SoftBreak | Event::HardBreak => self.out.push_str("<br>\n"),
            Event::Rule => self.out.push_str("<hr>\n"),
            Event::TaskListMarker(done) => self.task_box(done),
        }
    }

    fn start(&mut self, tag: Tag<'a>) {
        if let Some(name) = inline_tag(&tag) {
            self.push_open(&format!("<{name}>"), &format!("</{name}>"));
            return;
        }
        match tag {
            Tag::Paragraph => self.paragraph(),
            Tag::Heading { level, .. } => self.heading(level as u8),
            Tag::BlockQuote(_) => self.block_quote(),
            Tag::CodeBlock(kind) => self.code_block(&kind),
            Tag::HtmlBlock => self.html_block(),
            Tag::List(start) => self.list(start),
            Tag::Item => self.item(),
            Tag::Link {
                link_type,
                dest_url,
                ..
            } => self.link(link_type, &dest_url),
            other => self.start_other(other),
        }
    }

    fn start_other(&mut self, tag: Tag<'a>) {
        match tag {
            Tag::Table(alignments) => {
                self.alignments = alignments;
                self.push_open("<table>\n", "</tbody>\n</table>\n");
            }
            Tag::TableHead => {
                self.in_table_head = true;
                self.cell = 0;
                self.push_open("<thead>\n<tr>", "</tr>\n</thead>\n<tbody>\n");
            }
            Tag::TableRow => {
                self.cell = 0;
                self.push_open("<tr>", "</tr>\n");
            }
            Tag::TableCell => self.table_cell(),
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
        if tag == TagEnd::TableHead {
            self.in_table_head = false;
        }
        let Some(open) = self.open.pop() else {
            return;
        };
        // An image paragraph whose image was left out leaves nothing.
        if open.closer == FIGURE_CLOSER
            && let Some(empty) = self.out.strip_suffix(FIGURE_OPENER)
        {
            self.out.truncate(empty.len());
            return;
        }
        self.out.push_str(&open.closer);
    }

    /// Closes highlights and HTML elements left open inside the construct
    /// that is ending.
    fn close_dangling(&mut self) {
        while let Some(open) = self.open.last() {
            if open.kind == OpenKind::Markdown {
                return;
            }
            let closer = open.closer.clone();
            self.open.pop();
            self.out.push_str(&closer);
        }
    }

    /// Closes everything opened above `depth`.
    fn close_to(&mut self, depth: usize) {
        while self.open.len() > depth {
            if let Some(open) = self.open.pop() {
                self.out.push_str(&open.closer);
            }
        }
    }

    fn push_open(&mut self, opener: &str, closer: &str) {
        self.out.push_str(opener);
        self.open.push(Open {
            kind: OpenKind::Markdown,
            closer: closer.to_owned(),
        });
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

    /// Index just past the end of the construct starting at `from`.
    fn construct_end(&self, from: usize) -> usize {
        let mut depth = 0usize;
        for (index, event) in self.events.iter().enumerate().skip(from) {
            match event {
                Event::Start(_) => depth += 1,
                Event::End(_) if depth <= 1 => return index + 1,
                Event::End(_) => depth -= 1,
                _ => {}
            }
        }
        self.events.len()
    }

    fn text(&mut self, text: &str) {
        let mut pieces = text.split("==");
        if let Some(first) = pieces.next() {
            self.out.push_str(&escape_text(first));
        }
        for piece in pieces {
            self.toggle_mark();
            self.out.push_str(&escape_text(piece));
        }
    }

    fn toggle_mark(&mut self) {
        if self
            .open
            .last()
            .is_some_and(|open| open.kind == OpenKind::Mark)
        {
            self.open.pop();
            self.out.push_str("</mark>");
            return;
        }
        self.out.push_str("<mark>");
        self.open.push(Open {
            kind: OpenKind::Mark,
            closer: "</mark>".to_owned(),
        });
    }

    /// A paragraph, or a `<figure>` when it holds nothing but an image, or
    /// nothing at all around an equation that stands on its own.
    fn paragraph(&mut self) {
        match self.peek(0) {
            Some(Event::Start(Tag::Image { .. })) => {
                let after = self.construct_end(self.pos);
                if matches!(self.events.get(after), Some(Event::End(TagEnd::Paragraph))) {
                    self.push_open(FIGURE_OPENER, FIGURE_CLOSER);
                    return;
                }
            }
            Some(Event::DisplayMath(_))
                if matches!(self.peek(1), Some(Event::End(TagEnd::Paragraph))) =>
            {
                self.push_open("", "\n");
                return;
            }
            _ => {}
        }
        self.push_open("<p>", "</p>\n");
    }

    fn heading(&mut self, level: u8) {
        let mut text = String::new();
        for event in &self.events[self.pos..] {
            match event {
                Event::Text(part) | Event::Code(part) => text.push_str(part),
                Event::End(TagEnd::Heading(_)) => break,
                _ => {}
            }
        }
        let id = self.unique_id(&text);
        self.push_open(
            &format!("<h{level} id=\"{id}\">"),
            &format!("</h{level}>\n"),
        );
    }

    /// A heading id that is unique in the article.
    fn unique_id(&mut self, text: &str) -> String {
        let base = Some(slugify(text))
            .filter(|slug| !slug.is_empty())
            .unwrap_or_else(|| "section".to_owned());
        let mut id = base.clone();
        let mut count = 1;
        while !self.heading_ids.insert(id.clone()) {
            count += 1;
            id = format!("{base}-{count}");
        }
        id
    }

    fn block_quote(&mut self) {
        let header = match (self.peek(0), self.peek(1)) {
            (Some(Event::Start(Tag::Paragraph)), Some(Event::Text(text))) => parse_header(text),
            _ => None,
        };
        let Some(header) = header else {
            self.push_open("<blockquote>\n", "</blockquote>\n");
            return;
        };
        let title_start = self.pos + 1;
        if let Event::Text(text) = &mut self.events[title_start] {
            *text = text[header.rest_offset..].to_owned().into();
        }
        let title_end = header_line_end(&self.events, title_start);
        self.out.push_str(&format!(
            "<aside class=\"callout\" data-callout=\"{}\">\n<p class=\"callout-title\">",
            escape_attribute(&header.kind)
        ));
        if is_blank_span(&self.events, title_start, title_end) {
            self.out
                .push_str(&escape_text(&default_title(&header.kind)));
        } else {
            self.write_span(title_start, title_end);
        }
        self.out.push_str("</p>\n");
        self.push_open("", "</aside>\n");
        let paragraph_ended = matches!(
            self.events.get(title_end),
            Some(Event::End(TagEnd::Paragraph))
        );
        self.pos = title_end + 1;
        if !paragraph_ended {
            self.push_open("<p>", "</p>\n");
        }
    }

    /// Writes the events in `from..to`, closing anything they leave open.
    fn write_span(&mut self, from: usize, to: usize) {
        let depth = self.open.len();
        self.pos = from;
        while self.pos < to {
            let Some(event) = self.next_event() else {
                break;
            };
            self.event(event);
        }
        self.close_to(depth);
    }

    fn code_block(&mut self, kind: &CodeBlockKind<'_>) {
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
        let class = lang.as_ref().map_or_else(String::new, |lang| {
            format!(" class=\"language-{}\"", escape_attribute(lang))
        });
        let body = code::highlight(
            source.strip_suffix('\n').unwrap_or(&source),
            lang.as_deref(),
        );
        let pre = format!("<pre><code{class}>{body}</code></pre>");
        match title {
            Some(title) => self.out.push_str(&format!(
                "<figure class=\"code\">\n<figcaption>{}</figcaption>\n{pre}\n</figure>\n",
                escape_text(&title)
            )),
            None => {
                self.out.push_str(&pre);
                self.out.push('\n');
            }
        }
    }

    fn html_block(&mut self) {
        let mut html = String::new();
        self.collect_until_end(|event| {
            if let Event::Html(text) | Event::Text(text) = event {
                html.push_str(&text);
            }
        });
        let depth = self.open.len();
        let start = self.out.len();
        self.raw_html(html.trim_end());
        self.close_to(depth);
        if self.out.len() > start {
            self.out.push('\n');
        }
    }

    fn raw_html(&mut self, html: &str) {
        for piece in raw::filter(html) {
            if let Some(hidden) = &self.hiding {
                if piece == Piece::Unhide(hidden.clone()) {
                    self.hiding = None;
                }
                continue;
            }
            self.html_piece(piece);
        }
    }

    fn html_piece(&mut self, piece: Piece) {
        match piece {
            Piece::Hide(tag) => self.hiding = Some(tag),
            Piece::Unhide(_) => {}
            Piece::Markup(markup) => self.out.push_str(&markup),
            Piece::Text(text) if text == "<" => self.out.push_str("&lt;"),
            Piece::Text(text) => self.out.push_str(&text),
            Piece::Image { src, width } => {
                let width = width.and_then(|width| width.parse::<f64>().ok());
                self.image_tag(&src, width, "");
            }
            Piece::Open { tag, markup } => {
                self.out.push_str(&markup);
                self.open.push(Open {
                    closer: format!("</{tag}>"),
                    kind: OpenKind::Html(tag),
                });
            }
            Piece::Close { tag } => self.close_html(&tag),
        }
    }

    /// Closes HTML element `tag` and anything opened inside it, if it is
    /// open in the current Markdown construct.
    fn close_html(&mut self, tag: &str) {
        let mut depth = None;
        for (index, open) in self.open.iter().enumerate().rev() {
            match &open.kind {
                OpenKind::Html(name) if name == tag => {
                    depth = Some(index);
                    break;
                }
                OpenKind::Markdown => break,
                _ => {}
            }
        }
        if let Some(depth) = depth {
            self.close_to(depth);
        }
    }

    fn list(&mut self, start: Option<u64>) {
        match start {
            Some(1) => self.push_open("<ol>\n", "</ol>\n"),
            Some(start) => self.push_open(&format!("<ol start=\"{start}\">\n"), "</ol>\n"),
            None => self.push_open("<ul>\n", "</ul>\n"),
        }
    }

    fn item(&mut self) {
        let is_task = matches!(
            (self.peek(0), self.peek(1)),
            (Some(Event::TaskListMarker(_)), _)
                | (
                    Some(Event::Start(Tag::Paragraph)),
                    Some(Event::TaskListMarker(_))
                )
        );
        let opener = if is_task {
            "<li class=\"task\">"
        } else {
            "<li>"
        };
        self.push_open(opener, "</li>\n");
    }

    fn task_box(&mut self, done: bool) {
        let checked = if done { " checked" } else { "" };
        self.out
            .push_str(&format!("<input type=\"checkbox\" disabled{checked}> "));
    }

    fn table_cell(&mut self) {
        let tag = if self.in_table_head { "th" } else { "td" };
        let style = self
            .alignments
            .get(self.cell)
            .copied()
            .map_or("", alignment_style);
        self.cell += 1;
        self.push_open(&format!("<{tag}{style}>"), &format!("</{tag}>"));
    }

    fn link(&mut self, link_type: LinkType, destination: &str) {
        match link_target(link_type, destination) {
            Some(href) => {
                self.push_open(&format!("<a href=\"{}\">", escape_attribute(&href)), "</a>")
            }
            None => self.push_open("", ""),
        }
    }

    fn image(&mut self, link_type: LinkType, destination: &str) {
        let mut alt = String::new();
        self.collect_until_end(|event| {
            if let Event::Text(text) = event {
                alt.push_str(&text);
            }
        });
        let (alt, size) = match link_type {
            LinkType::WikiLink { has_pothole: true } => (String::new(), Some(alt.as_str())),
            LinkType::WikiLink { .. } => (String::new(), None),
            _ => match alt.rsplit_once('|') {
                Some((text, size)) => (text.to_owned(), Some(size)),
                None => (alt.clone(), None),
            },
        };
        let width = size.and_then(embed_width);
        self.image_tag(destination, width, &alt);
    }

    /// Writes an `<img>` for `destination`. Missing images and embeds of
    /// other notes are left out, since the website can't show them.
    fn image_tag(&mut self, destination: &str, width: Option<f64>, alt: &str) {
        let src = match self.images.locate(destination) {
            Located::File(path) => self.local_source(destination, path),
            Located::Remote(url) => Some(url),
            Located::Missing => {
                self.missing_images.push(destination.to_owned());
                None
            }
            Located::NotAnImage => None,
        };
        let Some(src) = src else {
            return;
        };
        let width = width.map_or_else(String::new, |width| format!(" width=\"{width}\""));
        self.out.push_str(&format!(
            "<img src=\"{}\" alt=\"{}\"{width}>",
            escape_attribute(&src),
            escape_attribute(alt)
        ));
    }

    /// A local image as a `data:` URI, or its path when images aren't
    /// inlined. An unreadable file counts as missing.
    fn local_source(&mut self, destination: &str, path: PathBuf) -> Option<String> {
        if !self.inline_images {
            return Some(destination.to_owned());
        }
        if let Some(uri) = self.data_uris.get(&path) {
            return Some(uri.clone());
        }
        let Ok(bytes) = std::fs::read(&path) else {
            self.missing_images.push(destination.to_owned());
            return None;
        };
        let extension = image_extension(&path.to_string_lossy()).unwrap_or_default();
        let uri = format!(
            "data:{};base64,{}",
            mime_type(&extension),
            base64::engine::general_purpose::STANDARD.encode(bytes)
        );
        self.data_uris.insert(path, uri.clone());
        Some(uri)
    }

    fn math(&mut self, latex: &str, display: bool) {
        self.equation_at.push(self.out.len());
        self.equations.push(Equation {
            latex: latex.to_owned(),
            display,
        });
    }

    fn footnote_reference(&mut self, name: &str) {
        if self.in_footnote || !self.footnotes.definitions.contains_key(name) {
            self.out.push_str(&escape_text(&format!("[^{name}]")));
            return;
        }
        let number = match self.footnotes.order.iter().position(|cited| cited == name) {
            Some(index) => index + 1,
            None => {
                self.footnotes.order.push(name.to_owned());
                self.footnotes.order.len()
            }
        };
        let count = self.footnotes.citations.entry(name.to_owned()).or_default();
        *count += 1;
        let id = reference_id(number, *count);
        self.out.push_str(&format!(
            "<sup class=\"footnote-ref\"><a href=\"#fn-{number}\" id=\"{id}\">{number}</a></sup>"
        ));
    }

    /// The footnote list, in order of first citation, each with links back
    /// to every place that cites it.
    fn write_footnotes(&mut self) {
        if self.footnotes.order.is_empty() {
            return;
        }
        self.out.push_str("<section class=\"footnotes\">\n<ol>\n");
        self.in_footnote = true;
        for (index, name) in self.footnotes.order.clone().iter().enumerate() {
            let number = index + 1;
            let events = self
                .footnotes
                .definitions
                .get(name)
                .cloned()
                .unwrap_or_default();
            self.out.push_str(&format!("<li id=\"fn-{number}\">\n"));
            self.render_nested(events);
            let citations = self.footnotes.citations.get(name).copied().unwrap_or(1);
            self.write_back_links(number, citations);
            self.out.push_str("</li>\n");
        }
        self.in_footnote = false;
        self.out.push_str("</ol>\n</section>\n");
    }

    /// Links back to each citation, inside the footnote's last paragraph.
    fn write_back_links(&mut self, number: usize, citations: usize) {
        let links: Vec<String> = (1..=citations)
            .map(|count| {
                format!(
                    "<a href=\"#{}\" class=\"footnote-back\" aria-label=\"Back to the text\">↩\u{fe0e}</a>",
                    reference_id(number, count)
                )
            })
            .collect();
        let links = format!(" {}", links.join(" "));
        match self.out.strip_suffix("</p>\n") {
            Some(before) => {
                let at = before.len();
                self.out.insert_str(at, &links);
            }
            None => self.out.push_str(&links),
        }
        if !self.out.ends_with('\n') {
            self.out.push('\n');
        }
    }

    /// Writes `events` in place, then resumes the main stream.
    fn render_nested(&mut self, events: Vec<Event<'a>>) {
        let saved_events = std::mem::replace(&mut self.events, events);
        let saved_pos = std::mem::replace(&mut self.pos, 0);
        self.run();
        self.events = saved_events;
        self.pos = saved_pos;
    }

    /// Typesets the equations and splices them in.
    fn finish(self) -> Output {
        let (mathml, math_css) = to_mathml(&self.equations);
        let mut html = String::with_capacity(self.out.len() + mathml.len() * 256);
        let mut failed_math = Vec::new();
        let mut copied = 0;
        let sites = self.equation_at.iter().zip(&self.equations).zip(mathml);
        for ((at, equation), rendered) in sites {
            html.push_str(&self.out[copied..*at]);
            copied = *at;
            match rendered {
                Some(rendered) => html.push_str(&rendered),
                None => {
                    html.push_str(&format!(
                        "<code class=\"math-error\">{}</code>",
                        escape_text(&equation.latex)
                    ));
                    failed_math.push(equation.latex.clone());
                }
            }
        }
        html.push_str(&self.out[copied..]);
        Output {
            html,
            math_css,
            failed_math,
            missing_images: self.missing_images,
        }
    }
}
