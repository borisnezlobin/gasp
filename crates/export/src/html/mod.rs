//! HTML export for the owner's website.
//!
//! A note becomes the body of an article: clean semantic HTML with no
//! Obsidian class names, no scripts and no styles. The website's article
//! stylesheet ([`ARTICLE_CSS`]) styles it. The pieces are:
//!
//! - text as `<p>`, `<h2 id="…">`, `<ul>`/`<ol>`, `<blockquote>`, `<table>`
//!   and `<pre><code>`, with code highlighted into classed spans;
//! - callouts as `<aside class="callout" data-callout="note">`;
//! - footnotes as numbered references and a `<section class="footnotes">`
//!   list at the end;
//! - math as MathML, which browsers render natively;
//! - images inlined as `data:` URIs, which the website's publish endpoint
//!   uploads to its image store and replaces with links.
//!
//! The title and description come from the frontmatter (or the file name)
//! and are returned beside the body, because the website stores them as the
//! article's metadata and prints the title itself.

mod code;
mod math;
mod raw;
mod writer;

use std::path::Path;

use crate::pdf::preprocess;

/// The website's article stylesheet. It is the contract between this export
/// and the site: the site serves it for every article, and the standalone
/// preview page embeds it.
pub const ARTICLE_CSS: &str = include_str!("../../assets/article.css");

/// Export settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlOptions {
    /// Starts the body with the title as an `<h1>`. The website prints the
    /// title from the article's metadata, so this is off by default.
    pub include_title: bool,
    /// Inlines local images as `data:` URIs; when off, they keep their
    /// path relative to the note.
    pub inline_images: bool,
}

impl Default for HtmlOptions {
    fn default() -> Self {
        Self {
            include_title: false,
            inline_images: true,
        }
    }
}

/// An exported note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlExport {
    /// The article body, an `<article>` element.
    pub html: String,
    /// The frontmatter `title`, or the note's file name.
    pub title: String,
    /// The frontmatter `description`, when there is one.
    pub description: Option<String>,
    /// The URL slug the website files the article under.
    pub slug: String,
    /// Rules for the MathML Typst writes. They are part of [`ARTICLE_CSS`]
    /// already; they are returned for pages that don't use it.
    pub math_css: Option<String>,
    /// Equations that could not be typeset and are shown as LaTeX source.
    pub failed_math: Vec<String>,
    /// Embedded images that could not be found.
    pub missing_images: Vec<String>,
}

/// Converts a note to an article. `note_path` gives the title and locates
/// images; `vault_root` is the last place images are looked up.
pub fn export_html(
    markdown: &str,
    note_path: Option<&Path>,
    vault_root: Option<&Path>,
    options: &HtmlOptions,
) -> HtmlExport {
    let note = preprocess::clean(markdown);
    let title = note.frontmatter_title.clone().unwrap_or_else(|| {
        note_path.and_then(Path::file_stem).map_or_else(
            || "Untitled".to_owned(),
            |stem| stem.to_string_lossy().into_owned(),
        )
    });
    let output = writer::write(&note.body, note_path, vault_root, options, &title);
    HtmlExport {
        slug: slugify(&title),
        title,
        description: note.frontmatter_description,
        html: output.html,
        math_css: output.math_css,
        failed_math: output.failed_math,
        missing_images: output.missing_images,
    }
}

/// A complete page showing the article as the website will: the title,
/// the description and the body, styled by [`ARTICLE_CSS`].
pub fn standalone_page(export: &HtmlExport) -> String {
    let description = export
        .description
        .as_ref()
        .map_or_else(String::new, |text| {
            format!("<p class=\"description\">{}</p>\n", escape_text(text))
        });
    format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{title}</title>\n<script>{DARK_SCRIPT}</script>\n\
         <style>\n{css}{PAGE_CSS}</style>\n</head>\n<body>\n<main>\n\
         <header>\n<h1>{title}</h1>\n{description}</header>\n{body}</main>\n</body>\n</html>\n",
        title = escape_text(&export.title),
        css = ARTICLE_CSS,
        body = export.html,
    )
}

/// Follows the system's dark mode the way the website's `.dark` class does.
const DARK_SCRIPT: &str = "if (matchMedia('(prefers-color-scheme: dark)').matches) \
                           document.documentElement.classList.add('dark');";

/// The preview page around the article: the website's column and title.
const PAGE_CSS: &str = "
body { margin: 0; background: #ffffff; color: #1c1c1c; }
.dark body { background: #141414; color: #e8e6e3; }
main { max-width: 38.5rem; margin: 0 auto; padding: 4rem 1.25rem 6rem; }
header { margin-bottom: 3rem; text-align: center; font-family: \"Iowan Old Style\", Palatino, serif; }
header h1 { margin: 0; font-size: 2rem; font-weight: 400; line-height: 1.2; text-wrap: balance; }
header .description { margin: 1rem 0 0; font-size: 1.1rem; opacity: 0.7; text-wrap: pretty; }
";

/// The slug the website derives from a title: lowercase letters and digits
/// with single dashes between words, as `publish-article.mjs` makes it.
pub fn slugify(text: &str) -> String {
    let mut slug = String::with_capacity(text.len());
    for character in text.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_end_matches('-').to_owned()
}

/// Escapes text for an element's content.
pub(crate) fn escape_text(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            other => escaped.push(other),
        }
    }
    escaped
}

/// Escapes text for a double-quoted attribute value.
pub(crate) fn escape_attribute(text: &str) -> String {
    escape_text(text).replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_match_the_website() {
        assert_eq!(
            slugify("Normal Modes of a Chain"),
            "normal-modes-of-a-chain"
        );
        assert_eq!(slugify("  C++ & Rust: 2024!  "), "c-rust-2024");
        assert_eq!(slugify("Élan"), "lan");
    }

    #[test]
    fn escapes() {
        assert_eq!(escape_text("a < b & c"), "a &lt; b &amp; c");
        assert_eq!(escape_attribute("say \"hi\""), "say &quot;hi&quot;");
    }
}
