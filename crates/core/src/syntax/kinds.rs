//! Node kinds, markup token kinds and the per-kind details the parser records.

/// What a node in the syntax tree is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Document,
    Frontmatter,
    Paragraph,
    Heading {
        level: u8,
        setext: bool,
    },
    BlockQuote,
    Callout(Box<CalloutInfo>),
    /// The title text on a callout's header line.
    CalloutTitle,
    List {
        ordered: bool,
        start: Option<u64>,
    },
    ListItem {
        task: Option<bool>,
    },
    CodeBlock(Box<CodeBlockInfo>),
    MathBlock,
    HtmlBlock(HtmlKind),
    Table {
        alignments: Vec<Alignment>,
    },
    TableHead,
    TableRow,
    TableCell,
    ThematicBreak,
    FootnoteDefinition {
        label: String,
    },
    LinkDefinition {
        label: String,
    },
    /// A `%%` comment that spans whole lines.
    CommentBlock,
    Text,
    SoftBreak,
    HardBreak,
    Emphasis,
    Strong,
    Strikethrough,
    Highlight,
    /// An inline `%%…%%` comment.
    Comment,
    Code,
    Math {
        display: bool,
    },
    Link(Box<LinkInfo>),
    Image(Box<LinkInfo>),
    WikiLink(Box<WikiInfo>),
    Embed(Box<WikiInfo>),
    FootnoteReference {
        label: String,
    },
    Tag {
        name: String,
    },
    /// An inline HTML tag, or a paired element such as `<u>…</u>`.
    Html(HtmlKind),
}

impl NodeKind {
    /// Whether this is a block-level node.
    pub fn is_block(&self) -> bool {
        matches!(
            self,
            Self::Document
                | Self::Frontmatter
                | Self::Paragraph
                | Self::Heading { .. }
                | Self::BlockQuote
                | Self::Callout(_)
                | Self::List { .. }
                | Self::ListItem { .. }
                | Self::CodeBlock(_)
                | Self::MathBlock
                | Self::HtmlBlock(_)
                | Self::Table { .. }
                | Self::TableHead
                | Self::TableRow
                | Self::ThematicBreak
                | Self::FootnoteDefinition { .. }
                | Self::LinkDefinition { .. }
                | Self::CommentBlock
        )
    }

    /// Whether the children of this node are inline content that the
    /// Obsidian inline extensions (highlights, comments, tags) apply to.
    pub fn holds_inlines(&self) -> bool {
        matches!(
            self,
            Self::Paragraph
                | Self::Heading { .. }
                | Self::CalloutTitle
                | Self::ListItem { .. }
                | Self::TableCell
                | Self::Emphasis
                | Self::Strong
                | Self::Strikethrough
                | Self::Highlight
                | Self::Html(_)
        )
    }

    /// Whether this node is some kind of link, so its text is not scanned
    /// for tags or bare URLs.
    pub fn is_link_like(&self) -> bool {
        matches!(
            self,
            Self::Link(_) | Self::Image(_) | Self::WikiLink(_) | Self::Embed(_)
        )
    }
}

/// Column alignment of a GFM table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alignment {
    None,
    Left,
    Center,
    Right,
}

/// How a Markdown link or image was written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkKind {
    Inline,
    Reference,
    Collapsed,
    Shortcut,
    Autolink,
    Email,
    /// A plain `https://…` URL in text.
    BareUrl,
}

/// Details of a Markdown link or image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkInfo {
    pub kind: LinkKind,
    pub destination: String,
    pub title: String,
}

/// Details of a wikilink `[[target#heading|alias]]` or embed `![[file|300]]`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WikiInfo {
    pub target: String,
    /// The part after `#`, a heading or `^block` reference.
    pub subpath: Option<String>,
    /// The display text after `|`, when it isn't a size.
    pub alias: Option<String>,
    /// For embeds, `|300` or `|300x200`.
    pub size: Option<(u32, Option<u32>)>,
}

/// Fold state written after a callout type: `+` open, `-` closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fold {
    Open,
    Closed,
}

/// The canonical Obsidian callout types, with aliases folded in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CalloutKind {
    Note,
    Abstract,
    Info,
    Todo,
    Tip,
    Success,
    Question,
    Warning,
    Failure,
    Danger,
    Bug,
    Example,
    Quote,
    /// Any type Obsidian doesn't know; drawn like a note.
    Custom,
}

const CALLOUT_ALIASES: &[(&str, CalloutKind)] = &[
    ("note", CalloutKind::Note),
    ("abstract", CalloutKind::Abstract),
    ("summary", CalloutKind::Abstract),
    ("tldr", CalloutKind::Abstract),
    ("info", CalloutKind::Info),
    ("todo", CalloutKind::Todo),
    ("tip", CalloutKind::Tip),
    ("hint", CalloutKind::Tip),
    ("important", CalloutKind::Tip),
    ("success", CalloutKind::Success),
    ("check", CalloutKind::Success),
    ("done", CalloutKind::Success),
    ("question", CalloutKind::Question),
    ("help", CalloutKind::Question),
    ("faq", CalloutKind::Question),
    ("warning", CalloutKind::Warning),
    ("caution", CalloutKind::Warning),
    ("attention", CalloutKind::Warning),
    ("failure", CalloutKind::Failure),
    ("fail", CalloutKind::Failure),
    ("missing", CalloutKind::Failure),
    ("danger", CalloutKind::Danger),
    ("error", CalloutKind::Danger),
    ("bug", CalloutKind::Bug),
    ("example", CalloutKind::Example),
    ("quote", CalloutKind::Quote),
    ("cite", CalloutKind::Quote),
];

impl CalloutKind {
    /// Resolves a callout type name, case-insensitively.
    pub fn from_name(name: &str) -> Self {
        let lower = name.to_ascii_lowercase();
        CALLOUT_ALIASES
            .iter()
            .find(|(alias, _)| *alias == lower)
            .map_or(Self::Custom, |(_, kind)| *kind)
    }
}

/// Details of an Obsidian callout `> [!type]± Title`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalloutInfo {
    pub kind: CalloutKind,
    /// The type as written, e.g. `NOTE` or `my-type`.
    pub type_name: String,
    pub fold: Option<Fold>,
}

impl CalloutInfo {
    /// The title Obsidian shows when the header has none: the type name
    /// with its first letter capitalised.
    pub fn default_title(&self) -> String {
        let lower = self.type_name.to_lowercase();
        let mut chars = lower.chars();
        chars.next().map_or_else(String::new, |first| {
            first.to_uppercase().chain(chars).collect()
        })
    }
}

/// Details of a code block.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CodeBlockInfo {
    pub fenced: bool,
    /// The whole info string after the fence.
    pub info: String,
    pub language: Option<String>,
    /// From `title:"…"` in the fence line.
    pub title: Option<String>,
    /// From `ln:true` / `ln:false`, when given.
    pub line_numbers: Option<bool>,
    /// From `hl:2,4-6`, 1-based and inclusive.
    pub highlighted_lines: Vec<(u32, u32)>,
}

/// The raw HTML elements the editor treats specially.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HtmlKind {
    LineBreak,
    HorizontalRule,
    Underline,
    Div,
    Image,
    /// `<!-- … -->`.
    Comment,
    Other,
}

impl HtmlKind {
    /// Classifies an HTML tag name.
    pub fn from_tag_name(name: &str) -> Self {
        match name.to_ascii_lowercase().as_str() {
            "br" => Self::LineBreak,
            "hr" => Self::HorizontalRule,
            "u" => Self::Underline,
            "div" => Self::Div,
            "img" => Self::Image,
            _ => Self::Other,
        }
    }
}

/// What a markup token is. Each maps to a [`SyntaxKind`] for reveal settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MarkupKind {
    EmphasisDelimiter,
    StrongDelimiter,
    StrikethroughDelimiter,
    HighlightDelimiter,
    CodeDelimiter,
    MathDelimiter,
    /// `#`s and following space, a closing `#` run, or a setext underline.
    HeadingMarker,
    QuoteMarker,
    ListMarker,
    TaskMarker,
    /// `[!type]±` plus following space on a callout's first line.
    CalloutHeader,
    /// `[`, `]`, `![`, `<` and `>` around link text or an autolink.
    LinkBracket,
    /// `(url "title")` or `[ref]` after link text.
    LinkDestination,
    /// `[[`, `]]` and `![[`.
    WikiBracket,
    /// `target#heading|` when a wikilink has an alias.
    WikiTarget,
    /// `|300` on an embed.
    EmbedSize,
    /// `[^`, `]` and `]:` around footnote labels.
    FootnoteMarker,
    /// The opening or closing fence line of a code block.
    CodeFence,
    TableDelimiterRow,
    TablePipe,
    CommentDelimiter,
    HtmlTag,
    FrontmatterFence,
    ThematicBreak,
    /// A trailing backslash or the trailing spaces of a hard line break.
    HardBreakMarker,
}

/// Groups of syntax that reveal settings can target individually.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SyntaxKind {
    /// Emphasis, strong, strikethrough and highlight delimiters.
    Emphasis,
    Heading,
    Quote,
    List,
    Task,
    Callout,
    /// Link text brackets.
    Link,
    /// Link destinations and wikilink targets.
    LinkUrl,
    WikiLink,
    /// Images and embeds.
    Image,
    InlineCode,
    CodeBlock,
    Math,
    Table,
    Footnote,
    Comment,
    Html,
    Frontmatter,
    ThematicBreak,
    HardBreak,
}

impl MarkupKind {
    /// The reveal category of this markup.
    pub fn syntax_kind(self) -> SyntaxKind {
        MARKUP_SYNTAX
            .iter()
            .find(|(markup, _)| *markup == self)
            .map_or(SyntaxKind::Emphasis, |(_, syntax)| *syntax)
    }
}

const MARKUP_SYNTAX: &[(MarkupKind, SyntaxKind)] = &[
    (MarkupKind::EmphasisDelimiter, SyntaxKind::Emphasis),
    (MarkupKind::StrongDelimiter, SyntaxKind::Emphasis),
    (MarkupKind::StrikethroughDelimiter, SyntaxKind::Emphasis),
    (MarkupKind::HighlightDelimiter, SyntaxKind::Emphasis),
    (MarkupKind::CodeDelimiter, SyntaxKind::InlineCode),
    (MarkupKind::MathDelimiter, SyntaxKind::Math),
    (MarkupKind::HeadingMarker, SyntaxKind::Heading),
    (MarkupKind::QuoteMarker, SyntaxKind::Quote),
    (MarkupKind::ListMarker, SyntaxKind::List),
    (MarkupKind::TaskMarker, SyntaxKind::Task),
    (MarkupKind::CalloutHeader, SyntaxKind::Callout),
    (MarkupKind::LinkBracket, SyntaxKind::Link),
    (MarkupKind::LinkDestination, SyntaxKind::LinkUrl),
    (MarkupKind::WikiBracket, SyntaxKind::WikiLink),
    (MarkupKind::WikiTarget, SyntaxKind::LinkUrl),
    (MarkupKind::EmbedSize, SyntaxKind::Image),
    (MarkupKind::FootnoteMarker, SyntaxKind::Footnote),
    (MarkupKind::CodeFence, SyntaxKind::CodeBlock),
    (MarkupKind::TableDelimiterRow, SyntaxKind::Table),
    (MarkupKind::TablePipe, SyntaxKind::Table),
    (MarkupKind::CommentDelimiter, SyntaxKind::Comment),
    (MarkupKind::HtmlTag, SyntaxKind::Html),
    (MarkupKind::FrontmatterFence, SyntaxKind::Frontmatter),
    (MarkupKind::ThematicBreak, SyntaxKind::ThematicBreak),
    (MarkupKind::HardBreakMarker, SyntaxKind::HardBreak),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callout_aliases_resolve_case_insensitively() {
        assert_eq!(CalloutKind::from_name("TLDR"), CalloutKind::Abstract);
        assert_eq!(CalloutKind::from_name("Error"), CalloutKind::Danger);
        assert_eq!(CalloutKind::from_name("zzz"), CalloutKind::Custom);
    }

    #[test]
    fn default_title_capitalises_the_type() {
        let info = CalloutInfo {
            kind: CalloutKind::Note,
            type_name: "NOTE".into(),
            fold: None,
        };
        assert_eq!(info.default_title(), "Note");
    }

    #[test]
    fn every_markup_kind_has_a_syntax_kind() {
        assert_eq!(MARKUP_SYNTAX.len(), 25);
        assert_eq!(MarkupKind::WikiTarget.syntax_kind(), SyntaxKind::LinkUrl);
    }
}
