//! Conversion snapshots for each construct, and layout checks on compiled
//! documents: footnote placement, the drop cap and PDF validity.

use std::path::{Path, PathBuf};

use editor_export::pdf::{
    ConvertOptions, MathSite, PdfOptions, TypstBody, TypstNote, compile_note, export_pdf,
    markdown_to_typst, typst_preamble, typst_source, write_pdf,
};
use typst::layout::{Frame, FrameItem, Point};
use typst_layout::PagedDocument;

fn body(markdown: &str) -> String {
    markdown_to_typst(markdown, &ConvertOptions::default()).markup
}

fn assert_converts(markdown: &str, expected: &str) {
    assert_eq!(
        body(markdown).trim_end(),
        expected.trim_end(),
        "for {markdown:?}"
    );
}

#[test]
fn headings() {
    assert_converts(
        "# Title\n\n## Sub *it*",
        "#heading(level: 1)[Title];\n\n#heading(level: 2)[Sub #emph[it];];",
    );
}

#[test]
fn inline_styles() {
    assert_converts(
        "Some *em* **strong** ~~gone~~ ==mark== <u>under</u> text.",
        "Some #emph[em]; #strong[strong]; #strike[gone]; #mark[mark]; #underline[under]; text\\.",
    );
}

#[test]
fn unclosed_highlight_and_html_close_with_paragraph() {
    assert_converts("a ==b <u>c", "a #mark[b #underline[c];];");
}

#[test]
fn soft_breaks_and_br_are_line_breaks() {
    assert_converts(
        "Line one\nline two<br>three",
        "Line one#linebreak();line two#linebreak();three",
    );
}

#[test]
fn links() {
    assert_converts(
        "[site](https://example.com/a_b) and [[Note|alias]] and [local](Other%20Note.md)",
        "#link(\"https://example.com/a_b\")[site]; and #underline[alias]; and #underline[local];",
    );
}

#[test]
fn lists() {
    assert_converts(
        "- a\n    - c\n\n3) x",
        "#md-list(depth: 0,\n(\"bullet\", [a#md-list(depth: 1,\n(\"bullet\", [c]),\n);\n\n]),\n);\n\n\
         #md-list(depth: 0,\n(3, [x]),\n);",
    );
}

#[test]
fn task_lists() {
    assert_converts(
        "- [ ] open\n- [x] done",
        "#md-list(depth: 0,\n(\"open\", [open]),\n(\"done\", [done]),\n);",
    );
}

#[test]
fn block_quote() {
    assert_converts(
        "> quoted\n> more",
        "#quote-block[quoted#linebreak();more\n\n];",
    );
}

#[test]
fn callout_with_default_title() {
    assert_converts("> [!note]\n> Body", "#callout(title: [Note])[Body\n\n];");
}

#[test]
fn callout_with_formatted_title_and_fold_marker() {
    assert_converts(
        "> [!tip]- Folded **bold** title\n> Body line\n>\n> Second para",
        "#callout(title: [Folded #strong[bold]; title])[Body line\n\nSecond para\n\n];",
    );
}

#[test]
fn table() {
    assert_converts(
        "| a | b |\n|:-|-:|\n| 1 | $x$ |",
        "#md-table(aligns: (left, right,), header: ([a], [b], ), [1], [#m(\"$x$\");], );",
    );
}

#[test]
fn code_blocks() {
    assert_converts(
        "```rust title:\"main.rs\"\nfn main() {}\n```\n\n    indented",
        "#code-block(lang: \"rust\", title: \"main.rs\", \"fn main() {}\");\n\n#code-block(\"indented\");",
    );
}

#[test]
fn missing_images_and_note_embeds() {
    assert_converts(
        "![[gone.png|300]] and ![alt](missing.jpg) and ![[Some Note]]",
        "#missing-image(\"gone.png\"); and #missing-image(\"missing.jpg\"); and #note-embed(\"Some Note\");",
    );
}

#[test]
fn rules() {
    assert_converts("<hr>\n\ntext<hr/>more", "#hrule(); \n\ntext#hrule();more");
}

#[test]
fn footnotes_render_at_each_reference() {
    assert_converts(
        "Cite[^a] twice[^a] and missing[^zz].\n\n[^a]: Note *one*.",
        "Cite#footnote[Note #emph[one];\\.]; twice#footnote[Note #emph[one];\\.]; and missing\\[^zz\\]\\.",
    );
}

#[test]
fn math() {
    let converted = markdown_to_typst(
        "Inline $a^2$ and\n\n$$\n\\frac{1}{2}\n$$\n\nbad $\\undefinedmacro$",
        &ConvertOptions::default(),
    );
    assert_eq!(
        converted.markup.trim_end(),
        "Inline #m(\"$a ^(2 )$\"); and\n\n#m(\"$ frac(1 ,2 ) $\");\n\nbad #math-error(\"\\\\undefinedmacro\");"
    );
    assert_eq!(converted.math.len(), 2);
    assert!(converted.math[1].display);
    assert_eq!(
        converted.unconverted_math,
        vec!["\\undefinedmacro".to_owned()]
    );
    let first = &converted.math[0];
    assert_eq!(&converted.markup[first.range.clone()], "#m(\"$a ^(2 )$\");");
}

#[test]
fn frontmatter_and_comments_are_dropped() {
    assert_converts(
        "---\ntitle: Front\n---\nBody %%hidden%% end\n\n%%\nblock\n%%\n\nafter",
        "Body  end\n\nafter",
    );
}

#[test]
fn frontmatter_title_replaces_file_title() {
    let options = ConvertOptions {
        title: Some("file name".to_owned()),
        ..ConvertOptions::default()
    };
    let converted = markdown_to_typst("---\ntitle: Front\n---\nBody", &options);
    assert!(
        converted.markup.starts_with("#note-title[Front];"),
        "{}",
        converted.markup
    );
    let converted = markdown_to_typst("Body", &options);
    assert!(
        converted.markup.starts_with("#note-title[file name];"),
        "{}",
        converted.markup
    );
}

#[test]
fn page_break_markup() {
    assert_converts(
        "<div class=\"page-break\"></div>\n\nNext",
        "#page-break(); \n\nNext",
    );
    assert_converts(
        "<div style=\"page-break-after: always;\"></div>",
        "#page-break();",
    );
}

#[test]
fn special_characters_are_escaped() {
    assert_converts(
        "Chars: * _ # $ [ ] < > @ = - + / ~ ' \" \\ 1. // x",
        "Chars\\: \\* \\_ \\# \\$ \\[ \\] \\< \\> \\@ \\= \\- \\+ \\/ \\~ \\' \\\" \\\\ 1\\. \\/\\/ x",
    );
}

#[test]
fn inline_html() {
    assert_converts(
        "<span style=\"color: #b5452c\">red</span> <sup>up</sup> <kbd>K</kbd> <div style=\"text-align: right\">R</div>",
        "#text(fill: rgb(\"#b5452c\"))[red]; #super[up]; #kbd[K]; #align(right)[R];",
    );
}

#[test]
fn drop_cap_splits_first_paragraph_into_chunks() {
    let options = ConvertOptions {
        drop_cap_lines: Some(3),
        ..ConvertOptions::default()
    };
    let converted = markdown_to_typst("## Head\n\nOnce *upon a* time.\n\nNext.", &options);
    assert_eq!(
        converted.markup.trim_end(),
        "#heading(level: 2)[Head];\n\n\
         #drop-cap(lines: 3, [O], ([nce ], [#emph[upon ]], [#emph[a]; ], [time\\.], ));\n\nNext\\."
    );
}

#[test]
fn drop_cap_needs_text_at_paragraph_start() {
    let options = ConvertOptions {
        drop_cap_lines: Some(3),
        ..ConvertOptions::default()
    };
    let converted = markdown_to_typst("*Once* upon.\n\nLater.", &options);
    assert!(
        !converted.markup.contains("drop-cap"),
        "{}",
        converted.markup
    );
}

/// A temporary note folder holding one PNG in `images/`.
struct TempNote {
    dir: PathBuf,
}

impl TempNote {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("editor-export-{name}-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("images")).unwrap();
        let corpus_image = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/corpus/Course Notes/Classical Mechanics/images/vector-159.png");
        std::fs::copy(corpus_image, dir.join("images/pic.png")).unwrap();
        Self { dir }
    }

    fn note_path(&self) -> PathBuf {
        self.dir.join("Note.md")
    }
}

impl Drop for TempNote {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn images_resolve_next_to_the_note() {
    let temp = TempNote::new("images");
    let note_path = temp.note_path();
    let markdown = "![[pic.png|300]]\n\n<img src=\"images/pic.png\" width=\"120\">";
    let note = typst_source(markdown, Some(&note_path), None, &PdfOptions::default());
    assert!(
        note.body
            .markup
            .contains("#note-image(\"/assets/0.png\", width: 225pt);"),
        "{}",
        note.body.markup
    );
    assert!(
        note.body
            .markup
            .contains("#note-image(\"/assets/0.png\", width: 90pt);")
    );
    assert_eq!(note.body.images.len(), 1);
    let compiled = compile_note(&note, &[]).unwrap();
    let images = count_items(&compiled.document, |item| {
        matches!(item, FrameItem::Image(..))
    });
    assert_eq!(images, 2);
}

fn count_items(document: &PagedDocument, predicate: impl Fn(&FrameItem) -> bool + Copy) -> usize {
    fn walk(frame: &Frame, predicate: impl Fn(&FrameItem) -> bool + Copy) -> usize {
        frame
            .items()
            .map(|(_, item)| match item {
                FrameItem::Group(group) => walk(&group.frame, predicate),
                other => usize::from(predicate(other)),
            })
            .sum()
    }
    document
        .pages()
        .iter()
        .map(|page| walk(&page.frame, predicate))
        .sum()
}

/// A run of text on a page with its absolute position and size.
struct PlacedText {
    text: String,
    y: f64,
    size: f64,
}

fn page_texts(frame: &Frame, offset: Point, out: &mut Vec<PlacedText>) {
    for (position, item) in frame.items() {
        let at = offset + *position;
        match item {
            FrameItem::Group(group) => page_texts(&group.frame, at, out),
            FrameItem::Text(text) => out.push(PlacedText {
                text: text.text.to_string(),
                y: at.y.to_pt(),
                size: text.size.to_pt(),
            }),
            _ => {}
        }
    }
}

fn texts_by_page(document: &PagedDocument) -> Vec<Vec<PlacedText>> {
    document
        .pages()
        .iter()
        .map(|page| {
            let mut texts = Vec::new();
            page_texts(&page.frame, Point::zero(), &mut texts);
            texts
        })
        .collect()
}

fn find(pages: &[Vec<PlacedText>], needle: &str) -> Option<(usize, f64)> {
    pages.iter().enumerate().find_map(|(page, texts)| {
        texts
            .iter()
            .find(|placed| placed.text.contains(needle))
            .map(|placed| (page, placed.y))
    })
}

fn compile_markdown(markdown: &str, options: &PdfOptions) -> PagedDocument {
    let note = typst_source(markdown, Some(Path::new("Test.md")), None, options);
    compile_note(&note, &[]).unwrap().document
}

#[test]
fn footnotes_land_on_the_citing_page() {
    let mut markdown = String::new();
    let mut definitions = String::new();
    for index in 0..40 {
        markdown.push_str(&format!(
            "Paragraph {index} has words that fill a line or two of the page before \
             it reaches anchor{index}z[^{index}] and then continues with more words.\n\n"
        ));
        definitions.push_str(&format!("[^{index}]: Footnote body{index}z text.\n"));
    }
    markdown.push('\n');
    markdown.push_str(&definitions);

    let document = compile_markdown(&markdown, &PdfOptions::default());
    let pages = texts_by_page(&document);
    assert!(
        pages.len() >= 4,
        "expected several pages, got {}",
        pages.len()
    );
    for index in 0..40 {
        let (anchor_page, anchor_y) = find(&pages, &format!("anchor{index}z")).unwrap();
        let (note_page, note_y) = find(&pages, &format!("body{index}z")).unwrap();
        assert_eq!(anchor_page, note_page, "footnote {index} left its page");
        assert!(note_y > anchor_y, "footnote {index} is above its reference");
    }
}

#[test]
fn footnote_text_uses_footnote_size() {
    let document = compile_markdown("Text[^1].\n\n[^1]: Small words.", &PdfOptions::default());
    let pages = texts_by_page(&document);
    let small = pages[0]
        .iter()
        .find(|placed| placed.text.contains("Small"))
        .unwrap();
    assert!((small.size - 8.5).abs() < 0.01, "size {}", small.size);
}

#[test]
fn drop_cap_is_off_by_default_and_renders_when_asked_for() {
    let markdown = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod \
                    tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, \
                    quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo.\n\nNext.";
    let pages = texts_by_page(&compile_markdown(markdown, &PdfOptions::default()));
    assert!(pages[0].iter().all(|placed| placed.size < 30.0));

    let with_drop_cap = PdfOptions {
        drop_cap_lines: 3,
        ..PdfOptions::default()
    };
    let pages = texts_by_page(&compile_markdown(markdown, &with_drop_cap));
    let letter = pages[0]
        .iter()
        .find(|placed| placed.text == "L")
        .expect("drop cap letter");
    assert!(letter.size > 36.0, "drop cap is only {} pt", letter.size);
    assert!(find(&pages, "orem").is_some());
}

#[test]
fn page_break_starts_a_new_page() {
    let document = compile_markdown(
        "Before\n\n<div class=\"page-break\"></div>\n\nAfter",
        &PdfOptions::default(),
    );
    let pages = texts_by_page(&document);
    assert_eq!(pages.len(), 2);
    assert_eq!(find(&pages, "After").map(|(page, _)| page), Some(1));
}

#[test]
fn pdf_is_valid_and_non_empty() {
    let temp = TempNote::new("pdf");
    let markdown = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/constructs.md"),
    )
    .unwrap();
    let export = export_pdf(
        &markdown,
        Some(&temp.note_path()),
        None,
        &PdfOptions::default(),
        &[],
    )
    .unwrap();
    assert!(export.pdf.starts_with(b"%PDF-"));
    assert!(export.pdf.len() > 1000);
    assert!(export.replaced_math.is_empty());
    let parsed = lopdf::Document::load_mem(&export.pdf).expect("PDF parses");
    assert_eq!(parsed.get_pages().len(), export.pages);
    assert!(export.pages >= 2, "the page break adds a page");
}

#[test]
fn no_markup_leaks_into_the_text() {
    let temp = TempNote::new("leaks");
    let markdown = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/constructs.md"),
    )
    .unwrap();
    let note = typst_source(
        &markdown,
        Some(&temp.note_path()),
        None,
        &PdfOptions::default(),
    );
    let document = compile_note(&note, &[]).unwrap().document;
    for placed in texts_by_page(&document).iter().flatten() {
        assert!(
            !placed.text.contains(';')
                && !placed.text.contains('\\')
                && !placed.text.contains("%%"),
            "markup leaked: {:?}",
            placed.text
        );
    }
}

#[test]
fn equations_that_fail_to_typeset_are_replaced() {
    let broken = "#m(\"$nosuchsymbol$\");";
    let good = "#m(\"$x$\");";
    let markup = format!("A {good} and {broken} end.");
    let site = |call: &str, latex: &str| {
        let start = markup.find(call).unwrap();
        MathSite {
            range: start..start + call.len(),
            latex: latex.to_owned(),
            display: false,
        }
    };
    let note = TypstNote {
        preamble: typst_preamble(&PdfOptions::default()),
        body: TypstBody {
            math: vec![site(good, "x"), site(broken, "\\nosuch")],
            markup: markup.clone(),
            ..TypstBody::default()
        },
    };
    let compiled = compile_note(&note, &[]).unwrap();
    assert_eq!(compiled.replaced_math.len(), 1);
    assert_eq!(compiled.replaced_math[0].latex, "\\nosuch");
    assert!(!write_pdf(&compiled.document).unwrap().is_empty());
}

#[test]
fn non_math_errors_are_reported() {
    let note = TypstNote {
        preamble: typst_preamble(&PdfOptions::default()),
        body: TypstBody {
            markup: "#nosuchfunction()".to_owned(),
            ..TypstBody::default()
        },
    };
    assert!(compile_note(&note, &[]).is_err());
}

#[test]
fn title_is_not_repeated_when_the_note_opens_with_it() {
    let options = ConvertOptions {
        title: Some("Lemma".to_owned()),
        ..ConvertOptions::default()
    };
    let converted = markdown_to_typst("# lemma\n\nBody", &options);
    assert!(
        converted.markup.starts_with("#heading(level: 1)[lemma];"),
        "{}",
        converted.markup
    );
    let converted = markdown_to_typst("# Another heading\n\nBody", &options);
    assert!(converted.markup.starts_with("#note-title[Lemma];"));
}

/// A short table moves to the next page whole instead of leaving its first
/// rows behind, wherever the page break falls.
#[test]
fn short_tables_are_not_split() {
    let table = "| a | b |\n|---|---|\n| Alpharow | 1 |\n| Betarow | 2 |\n| Gammarow | 3 |\n";
    let options = PdfOptions {
        include_title: false,
        ..PdfOptions::default()
    };
    let mut crossed_a_page = false;
    for lines in 16..40 {
        let filler = "Filler line.\n\n".repeat(lines);
        let pages = texts_by_page(&compile_markdown(&format!("{filler}{table}"), &options));
        let first = find(&pages, "Alpharow").map(|(page, _)| page);
        let last = find(&pages, "Gammarow").map(|(page, _)| page);
        assert!(first.is_some());
        assert_eq!(first, last, "table split after {lines} filler lines");
        crossed_a_page |= first == Some(1);
    }
    assert!(
        crossed_a_page,
        "the filler never pushed the table to page two"
    );
}

/// The owner's styled HTML, with synthetic text.
const STYLED_HTML: &str = "<span style=\"color:red;\">abc</span> and <span style=”color:red;”>curly</span> \
and <span style=”color: blue; font-size: 2em”>big</span>\n\n\
<p style=\"text-align: center;\">Centred paragraph</p>\n\n\
<center>Centred block</center>\n\n\
<b>bold</b> <i>italic</i> <s>gone</s> x<sup>2</sup> H<sub>2</sub>O <mark>marked</mark> \
<kbd>Ctrl</kbd> <a href=\"https://example.com\">site</a> <a href=\"javascript:alert(1)\">bad</a> \
<span onclick=\"alert(1)\" style=\"position: fixed; color: #00f\">safe</span>";

#[test]
fn styled_html_subset() {
    assert_converts(
        STYLED_HTML,
        "#text(fill: rgb(\"#ff0000\"))[abc]; and #text(fill: rgb(\"#ff0000\"))[curly]; \
         and #text(fill: rgb(\"#0000ff\"), size: 2em)[big];\n\n\
         #align(center)[Centred paragraph]; \n\n\
         #align(center)[Centred block]; \n\n\
         #strong[bold]; #emph[italic]; #strike[gone]; x#super[2]; H#sub[2];O #mark[marked]; \
         #kbd[Ctrl]; #link(\"https://example.com\")[site]; #[bad]; \
         #text(fill: rgb(\"#0000ff\"))[safe];",
    );
}

#[test]
fn styled_html_compiles_with_sizes_applied() {
    let document = compile_markdown(STYLED_HTML, &PdfOptions::default());
    let pages = texts_by_page(&document);
    let size = |needle: &str| {
        pages[0]
            .iter()
            .find(|placed| placed.text.contains(needle))
            .map(|placed| placed.size)
            .unwrap_or_else(|| panic!("{needle} is missing"))
    };
    assert!(
        size("big") > size("abc") * 1.9,
        "{} vs {}",
        size("big"),
        size("abc")
    );
    assert!(find(&pages, "Centred paragraph").is_some());
    assert!(
        find(&pages, "bad").is_some(),
        "an unsafe link keeps its text"
    );
}
