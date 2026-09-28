//! HTML export against golden files, plus the pieces the website relies on.
//!
//! Each `tests/data/html/<name>.md` (and `tests/data/constructs.md`) is
//! exported and compared with `<name>.html` next to it. After an intended
//! change, run with `UPDATE_GOLDEN=1` to rewrite the golden files, then
//! review the diff.

use std::path::{Path, PathBuf};

use editor_export::html::{ARTICLE_CSS, HtmlExport, HtmlOptions, export_html, standalone_page};

fn data() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data")
}

fn export(note: &Path) -> HtmlExport {
    let markdown = std::fs::read_to_string(note).unwrap();
    export_html(
        &markdown,
        Some(note),
        Some(&data()),
        &HtmlOptions::default(),
    )
}

fn check_golden(note: &Path, golden: &Path) {
    let html = export(note).html;
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(golden, &html).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(golden).unwrap_or_default();
    if html != expected {
        let line = html
            .lines()
            .zip(expected.lines())
            .position(|(got, want)| got != want)
            .unwrap_or_else(|| html.lines().count().min(expected.lines().count()));
        panic!(
            "{} differs from {} at line {}:\n  got:  {}\n  want: {}\nRun with UPDATE_GOLDEN=1 to accept.",
            note.display(),
            golden.display(),
            line + 1,
            html.lines().nth(line).unwrap_or("<end>"),
            expected.lines().nth(line).unwrap_or("<end>"),
        );
    }
}

#[test]
fn constructs_match_golden() {
    check_golden(
        &data().join("constructs.md"),
        &data().join("html/constructs.html"),
    );
}

#[test]
fn article_matches_golden() {
    check_golden(
        &data().join("html/article.md"),
        &data().join("html/article.html"),
    );
}

#[test]
fn metadata_comes_from_frontmatter() {
    let export = export(&data().join("html/article.md"));
    assert_eq!(export.title, "Notes on Coupled Oscillators");
    assert_eq!(export.slug, "notes-on-coupled-oscillators");
    assert_eq!(
        export.description.as_deref(),
        Some("A short walk through normal modes.")
    );
    let fallback = export_html(
        "Body",
        Some(Path::new("a/My Note.md")),
        None,
        &HtmlOptions::default(),
    );
    assert_eq!(fallback.title, "My Note");
    assert_eq!(fallback.slug, "my-note");
}

#[test]
fn the_title_heading_is_opt_in_and_never_doubled() {
    let options = HtmlOptions {
        include_title: true,
        ..HtmlOptions::default()
    };
    let path = Path::new("Lemma.md");
    let added = export_html("Body", Some(path), None, &options);
    assert!(
        added.html.starts_with("<article>\n<h1>Lemma</h1>"),
        "{}",
        added.html
    );
    let doubled = export_html("# Lemma\n\nBody", Some(path), None, &options);
    assert_eq!(doubled.html.matches("<h1").count(), 1, "{}", doubled.html);
    let default = export_html("# Lemma\n\nBody", Some(path), None, &HtmlOptions::default());
    assert!(!default.html.contains("<h1"), "{}", default.html);
}

#[test]
fn reports_what_it_could_not_export() {
    let export = export(&data().join("constructs.md"));
    assert_eq!(export.missing_images, vec!["missing.png".to_owned()]);
    let export = export_html("$\\left( x$ and $x$", None, None, &HtmlOptions::default());
    assert_eq!(export.failed_math.len(), 1, "{}", export.html);
    assert!(export.html.contains("<code class=\"math-error\">"));
}

#[test]
fn images_can_stay_as_paths() {
    let options = HtmlOptions {
        inline_images: false,
        ..HtmlOptions::default()
    };
    let export = export_html("![[pic.png]]", Some(&data().join("x.md")), None, &options);
    assert!(
        export.html.contains("<img src=\"pic.png\""),
        "{}",
        export.html
    );
}

/// The stylesheet carries Typst's MathML rules, so a Typst upgrade that
/// changes them shows up here.
#[test]
fn stylesheet_has_the_math_rules_typst_emits() {
    let export = export_html("$x$", None, None, &HtmlOptions::default());
    let emitted = export.math_css.expect("math was typeset");
    let squash = |css: &str| css.split_whitespace().collect::<String>();
    assert!(
        squash(ARTICLE_CSS).contains(&squash(&emitted)),
        "article.css is missing Typst's MathML rules:\n{emitted}"
    );
}

#[test]
fn standalone_page_wraps_the_article() {
    let export = export(&data().join("html/article.md"));
    let page = standalone_page(&export);
    assert!(page.starts_with("<!DOCTYPE html>"));
    assert!(page.contains("<h1>Notes on Coupled Oscillators</h1>"));
    assert!(page.contains("<p class=\"description\">A short walk"));
    assert!(page.contains(&export.html));
}

#[test]
fn styled_html_passes_through_sanitised() {
    let markdown = "<span style=\"color:red;\">abc</span> and <span style=”color: red; font-size: 2em”>curly</span>\n\n\
<p style=\"text-align: center;\">Centred paragraph</p>\n\n\
<center>Centred block</center>\n\n\
<b>bold</b> <i>italic</i> <a href=\"https://example.com\" onclick=\"x()\">site</a> \
<a href=\"javascript:alert(1)\">bad</a> <span onmouseover=\"x()\" style=\"position: fixed; width: 1px; color: #00f\">safe</span>\
<script>alert(1)</script>";
    let html = export_html(markdown, None, None, &HtmlOptions::default()).html;
    for expected in [
        "<span style=\"color: #ff0000\">abc</span>",
        "<span style=\"color: #ff0000; font-size: 200%\">curly</span>",
        "<p style=\"text-align: center\">Centred paragraph</p>",
        "<center>Centred block</center>",
        "<b>bold</b> <i>italic</i> <a href=\"https://example.com\">site</a>",
        "<span style=\"color: #0000ff\">safe</span>",
    ] {
        assert!(html.contains(expected), "{expected} missing from\n{html}");
    }
    for unsafe_part in [
        "onclick",
        "onmouseover",
        "javascript",
        "position",
        "width",
        "<script",
    ] {
        assert!(!html.contains(unsafe_part), "{unsafe_part} in\n{html}");
    }
}
