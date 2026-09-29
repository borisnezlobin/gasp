//! Snapshot-style tests: each plan is printed as one line per source line,
//! with hidden text removed, styled runs as `{style,style:text}`, widgets as
//! `⟦…⟧` and line decorations in a `[…]` prefix.

use std::fmt::Write;
use std::ops::Range;

use super::*;
use crate::syntax::{self, SyntaxKind};

fn element() -> RevealSettings {
    RevealSettings::default()
}

fn around(scope: RevealScope) -> RevealSettings {
    RevealSettings::new(RevealMode::AroundCursor { scope })
}

fn shown() -> RevealSettings {
    RevealSettings::new(RevealMode::AlwaysShown)
}

fn hidden() -> RevealSettings {
    RevealSettings::new(RevealMode::AlwaysHidden)
}

/// Renders `text` with the cursor at the `‸` in it, if there is one.
fn render(marked: &str, settings: &RevealSettings) -> String {
    let cursor = marked.find('‸');
    let text = marked.replacen('‸', "", 1);
    let selections: Vec<Range<usize>> = cursor.map(|at| at..at).into_iter().collect();
    render_with(&text, &selections, settings)
}

fn render_with(text: &str, selections: &[Range<usize>], settings: &RevealSettings) -> String {
    let tree = syntax::parse(text);
    let plan = plan(&RenderInput {
        text,
        tree: &tree,
        selections,
        settings,
    });
    check_plan_invariants(text, &plan);
    plan.lines
        .iter()
        .map(|line| show_line(text, line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn check_plan_invariants(text: &str, plan: &RenderPlan) {
    for line in &plan.lines {
        let mut at = line.range.start;
        for run in &line.runs {
            assert_eq!(run.range.start, at, "runs must tile the line");
            at = run.range.end;
        }
        if !line.runs.is_empty() {
            assert_eq!(at, line.range.end);
        }
        for hidden in &line.hidden {
            assert!(line.range.start <= hidden.start && hidden.end <= line.range.end);
            assert!(text.is_char_boundary(hidden.start) && text.is_char_boundary(hidden.end));
        }
    }
}

fn line_style_label(style: &LineStyle) -> String {
    match style {
        LineStyle::Heading(level) => format!("h{level}"),
        LineStyle::Quote { depth } => format!("quote{depth}"),
        LineStyle::Callout { kind, depth } => format!("callout-{kind:?}{depth}").to_lowercase(),
        LineStyle::CalloutHeader { .. } => "callout-header".into(),
        LineStyle::CodeBlock { index } => format!("code{index}"),
        LineStyle::Conflict { side } => format!("{side:?}").to_lowercase(),
        other => format!("{other:?}").to_lowercase(),
    }
}

fn widget_label(text: &str, widget: &Widget) -> String {
    match &widget.kind {
        WidgetKind::InlineMath {
            tex,
            display: false,
        } => format!("math:{tex}"),
        WidgetKind::InlineMath { tex, display: true } => format!("math-display:{tex}"),
        WidgetKind::MathBlock { tex } => format!("math-block:{}", tex.replace('\n', "⏎")),
        WidgetKind::MathPreview { tex, .. } => format!("preview:{}", tex.replace('\n', "⏎")),
        WidgetKind::Image {
            target,
            width,
            embed,
            ..
        } => {
            let kind = if *embed { "embed" } else { "img" };
            let width = width.map(|w| format!("@{w}")).unwrap_or_default();
            format!("{kind}:{target}{width}")
        }
        WidgetKind::CalloutHeader { .. } => callout_label(text, &widget.kind),
        other => simple_widget_label(other),
    }
}

fn callout_label(text: &str, kind: &WidgetKind) -> String {
    let WidgetKind::CalloutHeader {
        kind,
        title,
        default_title,
        folded,
        ..
    } = kind
    else {
        return String::new();
    };
    let title = title
        .clone()
        .map_or(default_title.clone(), |r| text[r].to_owned());
    let folded = if *folded { ":folded" } else { "" };
    format!("callout:{kind:?}:{title}{folded}")
}

fn simple_widget_label(kind: &WidgetKind) -> String {
    match kind {
        WidgetKind::Checkbox { checked: true } => "[x]".into(),
        WidgetKind::Checkbox { checked: false } => "[ ]".into(),
        WidgetKind::ListBullet {
            number: Some(n), ..
        } => format!("{n}."),
        WidgetKind::ListBullet { depth, .. } => format!("•{depth}"),
        WidgetKind::CodeBlock {
            language, title, ..
        } => format!(
            "code:{}:{}",
            language.clone().unwrap_or_default(),
            title.clone().unwrap_or_default()
        ),
        WidgetKind::FootnoteSuperscript { label } => format!("^{label}"),
        WidgetKind::ConflictLabel { side } => format!("{side:?}").to_lowercase(),
        other => format!("{other:?}").to_lowercase(),
    }
}

fn show_line(text: &str, line: &LinePlan) -> String {
    let mut out = String::new();
    if !line.line_styles.is_empty() {
        let labels: Vec<String> = line.line_styles.iter().map(line_style_label).collect();
        let _ = write!(out, "[{}] ", labels.join(" "));
    }
    if line.collapsed {
        out.push_str("~collapsed~");
        return out;
    }
    if let Some(row) = &line.table_row {
        let cells: Vec<&str> = row.cells.iter().map(|cell| &text[cell.clone()]).collect();
        let _ = write!(
            out,
            "⟨row {}/{}: {}⟩ ",
            row.index,
            row.count,
            cells.join("¦")
        );
    }
    for widget in line
        .widgets
        .iter()
        .filter(|w| w.placement == Placement::Above)
    {
        let _ = write!(out, "^⟦{}⟧ ", widget_label(text, widget));
    }
    let mut pieces: Vec<(usize, u8, String)> = Vec::new();
    for widget in line
        .widgets
        .iter()
        .filter(|w| w.placement == Placement::Replace)
    {
        pieces.push((
            widget.range.start,
            0,
            format!("⟦{}⟧", widget_label(text, widget)),
        ));
    }
    for run in &line.runs {
        for visible in visible_parts(&run.range, &line.hidden) {
            let source = &text[visible.clone()];
            let shown = if run.styles.is_empty() {
                source.to_owned()
            } else {
                let names: Vec<&str> = run.styles.iter().map(|s| s.name()).collect();
                format!("{{{}:{source}}}", names.join(","))
            };
            pieces.push((visible.start, 1, shown));
        }
    }
    pieces.sort_by_key(|(at, order, _)| (*at, *order));
    out.extend(pieces.into_iter().map(|(_, _, piece)| piece));
    for widget in line
        .widgets
        .iter()
        .filter(|w| w.placement == Placement::Below)
    {
        let _ = write!(out, " v⟦{}⟧", widget_label(text, widget));
    }
    out
}

fn visible_parts(range: &Range<usize>, hidden: &[Range<usize>]) -> Vec<Range<usize>> {
    let mut parts = Vec::new();
    let mut at = range.start;
    for gap in hidden
        .iter()
        .filter(|h| h.end > range.start && h.start < range.end)
    {
        if gap.start > at {
            parts.push(at..gap.start);
        }
        at = at.max(gap.end);
    }
    if at < range.end {
        parts.push(at..range.end);
    }
    parts
}

#[track_caller]
fn check(marked: &str, settings: &RevealSettings, expected: &[&str]) {
    let actual = render(marked, settings);
    if expected.is_empty() {
        println!("SNAP\n{actual}\nENDSNAP");
    }
    let actual_lines: Vec<&str> = actual.split('\n').collect();
    assert_eq!(actual_lines, expected, "\n--- actual ---\n{actual}\n");
}

#[test]
fn plain_paragraph_has_one_plain_run() {
    check("just text", &element(), &["just text"]);
}

#[test]
fn emphasis_hidden_away_from_cursor() {
    check(
        "a **bold** and *em* ~~gone~~\n\n‸",
        &element(),
        &[
            "a {strong:bold} and {emphasis:em} {strikethrough:gone}",
            "",
            "",
        ],
    );
}

#[test]
fn emphasis_revealed_with_cursor_inside() {
    check(
        "a **bo‸ld** and *em*",
        &element(),
        &["a {strong,markup-dimmed:**}{strong:bold}{strong,markup-dimmed:**} and {emphasis:em}"],
    );
}

#[test]
fn cursor_touching_start_of_element_reveals_it() {
    check(
        "a ‸**bold** b",
        &element(),
        &["a {strong,markup-dimmed:**}{strong:bold}{strong,markup-dimmed:**} b"],
    );
}

#[test]
fn cursor_touching_end_of_element_reveals_it() {
    check(
        "a **bold**‸ b",
        &element(),
        &["a {strong,markup-dimmed:**}{strong:bold}{strong,markup-dimmed:**} b"],
    );
}

#[test]
fn cursor_one_past_the_element_hides_it() {
    check("a **bold** ‸b", &element(), &["a {strong:bold} b"]);
}

#[test]
fn nested_emphasis_reveals_both_levels() {
    check(
        "***bo‸th***",
        &element(),
        &[
            "{emphasis,markup-dimmed:*}{strong,emphasis,markup-dimmed:**}{strong,emphasis:both}{strong,emphasis,markup-dimmed:**}{emphasis,markup-dimmed:*}",
        ],
    );
}

#[test]
fn atx_heading_levels() {
    check(
        "# One\n## Two ##\n###### Six\n\n‸",
        &element(),
        &[
            "[h1] {heading-1:One}",
            "[h2] {heading-2:Two}",
            "[h6] {heading-6:Six}",
            "",
            "",
        ],
    );
}

#[test]
fn heading_marker_shows_on_cursor() {
    check(
        "## Tw‸o",
        &element(),
        &["[h2] {heading-2,markup-dimmed:## }{heading-2:Two}"],
    );
}

#[test]
fn setext_underline_collapses() {
    check(
        "Title\n===\n\npara‸",
        &element(),
        &["[h1] {heading-1:Title}", "[h1] ~collapsed~", "", "para"],
    );
}

#[test]
fn setext_underline_shows_with_cursor() {
    check(
        "Title‸\n---",
        &element(),
        &[
            "[h2] {heading-2:Title}",
            "[h2] {heading-2,markup-dimmed:---}",
        ],
    );
}

#[test]
fn bullets_replace_list_markers() {
    check(
        "- one\n- two\n  - nested\n\n‸",
        &element(),
        &["⟦•1⟧one", "⟦•1⟧two", "  ⟦•2⟧nested", "", ""],
    );
}

#[test]
fn list_marker_shows_on_cursor_line() {
    check(
        "- one\n- tw‸o",
        &element(),
        &["⟦•1⟧one", "{markup-dimmed:- }two"],
    );
}

#[test]
fn ordered_lists_keep_their_numbers() {
    check(
        "3. three\n4) four\n\n‸",
        &element(),
        &["⟦3.⟧three", "⟦4.⟧four", "", ""],
    );
}

#[test]
fn tasks_become_checkboxes() {
    check(
        "- [ ] todo\n- [x] done\n\n‸",
        &element(),
        &["⟦•1⟧⟦[ ]⟧todo", "⟦•1⟧⟦[x]⟧{task-done:done}", "", ""],
    );
}

#[test]
fn task_marker_shows_on_cursor_line() {
    check(
        "- [x] do‸ne",
        &element(),
        &["{markup-dimmed:- [x] }{task-done:done}"],
    );
}

#[test]
fn a_shown_list_marker_is_named_so_the_text_after_it_can_stay_put() {
    let text = "- [x] done\n\t1. one\n- plain";
    let tree = syntax::parse(text);
    let selections = [text.find("done").unwrap()..text.find("done").unwrap(); 1];
    let settings = element();
    let marker_of = |selections: &[Range<usize>]| {
        plan(&RenderInput {
            text,
            tree: &tree,
            selections,
            settings: &settings,
        })
        .lines
        .iter()
        .map(|line| line.shown_marker.clone().map(|range| &text[range]))
        .collect::<Vec<_>>()
    };
    assert_eq!(marker_of(&selections), [Some("- [x] "), None, None]);
    let on_number = text.find("one").unwrap();
    assert_eq!(
        marker_of(&[on_number..on_number]),
        [None, Some("1. "), None]
    );
}

#[test]
fn quote_markers_hide_off_cursor_lines() {
    check(
        "> first **b**\n> sec‸ond\n>> deep",
        &element(),
        &[
            "[quote1] first {strong:b}",
            "[quote1] {markup-dimmed:> }second",
            "[quote1 quote2] deep",
        ],
    );
}

#[test]
fn callout_header_becomes_widget() {
    check(
        "> [!note] Read *this*\n> body\n\n‸",
        &element(),
        &[
            "[callout-note1 callout-header] ⟦callout:Note:Read *this*⟧{callout-title:Read }{emphasis,callout-title:this}",
            "[callout-note1] body",
            "",
            "",
        ],
    );
}

#[test]
fn callout_without_title_uses_type_name() {
    check(
        "> [!faq]\n> body\n\n‸",
        &element(),
        &[
            "[callout-question1 callout-header] ⟦callout:Question:Faq⟧",
            "[callout-question1] body",
            "",
            "",
        ],
    );
}

#[test]
fn folded_callout_collapses_body() {
    check(
        "> [!tip]- Hidden\n> body\n> more\n\n‸",
        &element(),
        &[
            "[callout-tip1 callout-header] ⟦callout:Tip:Hidden:folded⟧{callout-title:Hidden}",
            "[callout-tip1] ~collapsed~",
            "[callout-tip1] ~collapsed~",
            "",
            "",
        ],
    );
}

#[test]
fn folded_callout_opens_with_cursor_in_body() {
    check(
        "> [!tip]- Hidden\n> bo‸dy",
        &element(),
        &[
            "[callout-tip1 callout-header] ⟦callout:Tip:Hidden⟧{callout-title:Hidden}",
            "[callout-tip1] {markup-dimmed:> }body",
        ],
    );
}

#[test]
fn callout_header_shows_on_its_line() {
    check(
        "> [!tip]+ Ti‸tle\n> body",
        &element(),
        &[
            "[callout-tip1 callout-header] {markup-dimmed:> [!tip]+ }{callout-title:Title}",
            "[callout-tip1] body",
        ],
    );
}

#[test]
fn nested_callouts() {
    check(
        "> [!info] Outer\n> > [!bug] Inner\n> > body\n\n‸",
        &element(),
        &[
            "[callout-info1 callout-header] ⟦callout:Info:Outer⟧{callout-title:Outer}",
            "[callout-info1 callout-bug2 callout-header] ⟦callout:Bug:Inner⟧{callout-title:Inner}",
            "[callout-info1 callout-bug2] body",
            "",
            "",
        ],
    );
}

#[test]
fn code_block_header_widget() {
    check(
        "```rust title:\"main.rs\" ln:true\nfn main() {}\n```\n\n‸",
        &element(),
        &[
            "[code0] ⟦code:rust:main.rs⟧",
            "[code1] {code-block:fn main() {}}",
            "[code2] ~collapsed~",
            "",
            "",
        ],
    );
}

#[test]
fn code_block_fences_show_with_cursor_inside() {
    check(
        "```rust\nfn ‸main() {}\n```",
        &element(),
        &[
            "[code0] {markup-dimmed:```rust}",
            "[code1] {code-block:fn main() {}}",
            "[code2] {markup-dimmed:```}",
        ],
    );
}

#[test]
fn indented_code_block() {
    check(
        "    let x = 1;\n\n‸",
        &element(),
        &["[code0]     {code-block:let x = 1;}", "", ""],
    );
}

#[test]
fn code_in_quote_collapses_closing_fence() {
    check(
        "> ```\n> code\n> ```\n\nz‸",
        &element(),
        &[
            "[quote1 code0] ⟦code::⟧",
            "[quote1 code1] {code-block:code}",
            "[quote1 code2] ~collapsed~",
            "",
            "z",
        ],
    );
}

#[test]
fn inline_code() {
    check(
        "use `x` and ``a`b``\n\n‸",
        &element(),
        &["use {code:x} and {code:a`b}", "", ""],
    );
}

#[test]
fn table_rows_become_grid_rows() {
    check(
        "| a | **b** |\n|---|:-:|\n| 1 | $x$ |\n\n‸",
        &element(),
        &[
            "[table] ⟨row 0/2: a¦**b**⟩  a  {strong:b} ",
            "[table] ~collapsed~",
            "[table] ⟨row 1/2: 1¦$x$⟩  1  ⟦math:x⟧ ",
            "",
            "",
        ],
    );
}

#[test]
fn a_table_stays_a_grid_with_the_cursor_inside() {
    // The cursor's cell reaches to it, and its bold reveals as a
    // paragraph's does; the pipes and delimiter row stay hidden.
    check(
        "| a | b |\n|---|---|\n| **1** | 2 ‸ |",
        &element(),
        &[
            "[table] ⟨row 0/2: a¦b⟩  a  b ",
            "[table] ~collapsed~",
            "[table] ⟨row 1/2: **1**¦2 ⟩  {strong:1}  2  ",
        ],
    );
    check(
        "| a | b |\n|---|---|\n| **‸1** | 2 |",
        &element(),
        &[
            "[table] ⟨row 0/2: a¦b⟩  a  b ",
            "[table] ~collapsed~",
            "[table] ⟨row 1/2: **1**¦2⟩  {strong,markup-dimmed:**}{strong:1}{strong,markup-dimmed:**}  2 ",
        ],
    );
}

#[test]
fn escaped_pipes_hide_their_backslash_in_the_grid() {
    check(
        "| a \\| b |\n|---|\n\n‸",
        &element(),
        &[
            "[table] ⟨row 0/1: a \\| b⟩  a | b ",
            "[table] ~collapsed~",
            "",
            "",
        ],
    );
}

#[test]
fn a_table_edited_as_markdown_shows_its_source() {
    let mut settings = element();
    settings.source_table = Some(3);
    check(
        "| a | b |\n|---|---|\n| 1 | ‸2 |",
        &settings,
        &[
            "[table] {markup-dimmed:|} a {markup-dimmed:|} b {markup-dimmed:|}",
            "[table] {markup-dimmed:|---|---|}",
            "[table] {markup-dimmed:|} 1 {markup-dimmed:|} 2 {markup-dimmed:|}",
        ],
    );
}

#[test]
fn inline_math_becomes_widget() {
    check(
        "Inline $x^2$ and $$y$$\n\n‸",
        &element(),
        &["Inline ⟦math:x^2⟧ and ⟦math-display:y⟧", "", ""],
    );
}

#[test]
fn inline_math_shows_source_and_preview_with_cursor() {
    check(
        "Inline $x‸^2$ end",
        &element(),
        &[
            "^⟦preview:x^2⟧ Inline {math-source,markup-dimmed:$}{math-source:x^2}{math-source,markup-dimmed:$} end",
        ],
    );
}

#[test]
fn cursor_after_math_reveals_it() {
    check(
        "a $x$‸ b",
        &element(),
        &[
            "^⟦preview:x⟧ a {math-source,markup-dimmed:$}{math-source:x}{math-source,markup-dimmed:$} b",
        ],
    );
}

#[test]
fn block_math_becomes_widget() {
    check(
        "$$\n\\int x\n$$\n\n‸",
        &element(),
        &[
            "[mathblock] ⟦math-block:\\int x⟧",
            "[mathblock] ~collapsed~",
            "[mathblock] ~collapsed~",
            "",
            "",
        ],
    );
}

#[test]
fn block_math_shows_source_with_cursor() {
    check(
        "$$\n\\int‸ x\n$$",
        &element(),
        &[
            "[mathblock] {math-source,markup-dimmed:$$}",
            "[mathblock] {math-source:\\int x}",
            "[mathblock] {math-source,markup-dimmed:$$} v⟦preview:\\int x⟧",
        ],
    );
}

#[test]
fn block_math_in_quote_strips_markers() {
    check(
        "> $$\n> a +\n> b\n> $$\n\nz‸",
        &element(),
        &[
            "[quote1 mathblock] ⟦math-block:a +⏎b⟧",
            "[quote1 mathblock] ~collapsed~",
            "[quote1 mathblock] ~collapsed~",
            "[quote1 mathblock] ~collapsed~",
            "",
            "z",
        ],
    );
}

#[test]
fn empty_double_dollar_stays_literal() {
    check("type $$ here‸", &element(), &["type $$ here"]);
}

#[test]
fn images_and_embeds_become_widgets() {
    check(
        "![alt](img.png) ![[pic.png|300]] ![c|120x80](c.png)\n\n‸",
        &element(),
        &["⟦img:img.png⟧ ⟦embed:pic.png@300⟧ ⟦img:c.png@120⟧", "", ""],
    );
}

#[test]
fn image_source_shows_with_preview_below() {
    check(
        "![al‸t](img.png)",
        &element(),
        &["{link,markup-dimmed:![}{link:alt}{link,markup-dimmed:](img.png)} v⟦img:img.png⟧"],
    );
}

#[test]
fn html_img_becomes_widget() {
    check(
        "<img src=\"a.png\" width=\"50\">\n\n‸",
        &element(),
        &["⟦img:a.png@50⟧", "", ""],
    );
}

#[test]
fn links_show_only_their_text() {
    check(
        "[link](http://x.y \"t\") [[note#h|alias]] [[plain]] <http://a.b> [r][ref]\n\n[ref]: http://r\n\n‸",
        &element(),
        &[
            "{link:link} {link:alias} {link:plain} {link:http://a.b} {link:r}",
            "",
            "{markup-dimmed:[ref]: http://r}",
            "",
            "",
        ],
    );
}

#[test]
fn link_source_shows_with_cursor() {
    check(
        "[li‸nk](http://x.y)",
        &element(),
        &["{link,markup-dimmed:[}{link:link}{link,markup-dimmed:](http://x.y)}"],
    );
}

#[test]
fn wikilink_target_shows_with_cursor() {
    check(
        "[[note#h|al‸ias]]",
        &element(),
        &["{link,markup-dimmed:[[note#h|}{link:alias}{link,markup-dimmed:]]}"],
    );
}

#[test]
fn wikilink_headings_read_as_note_then_heading() {
    check(
        "[[Waves#Questions]] [[Waves#^ab12]] [[#Local]] [[Waves#Q|shown]]\n‸",
        &element(),
        &[
            "{link:Waves}⟦subpathseparator⟧{link:Questions} {link:Waves}⟦subpathseparator⟧{link:^ab12} {link:Local} {link:shown}",
            "",
        ],
    );
    check(
        "[[Waves#Ques‸tions]]",
        &element(),
        &[
            "{link,markup-dimmed:[[}{link:Waves}{link,markup-dimmed:#}{link:Questions}{link,markup-dimmed:]]}",
        ],
    );
}

#[test]
fn footnote_reference_becomes_superscript() {
    check(
        "text[^1] more\n\n[^1]: the note‸",
        &element(),
        &[
            "text⟦^1⟧ more",
            "",
            "[footnotedefinition] {markup-dimmed:[^1]: }the note",
        ],
    );
}

#[test]
fn footnote_reference_shows_with_cursor() {
    check(
        "text[^1‸] more\n\n[^1]: note",
        &element(),
        &[
            "text{footnote-ref,markup-dimmed:[^}{footnote-ref:1}{footnote-ref,markup-dimmed:]} more",
            "",
            "[footnotedefinition] ⟦^1⟧note",
        ],
    );
}

#[test]
fn highlight_comment_tag_and_url() {
    check(
        "a ==hi== b %%secret%% c #tag/sub https://x.y.\n\n‸",
        &element(),
        &[
            "a {highlight:hi} b  c {tag:#tag/sub} {link:https://x.y}.",
            "",
            "",
        ],
    );
}

#[test]
fn comment_shows_with_cursor_inside() {
    check(
        "a %%sec‸ret%% c",
        &element(),
        &["a {comment,markup-dimmed:%%}{comment:secret}{comment,markup-dimmed:%%} c"],
    );
}

#[test]
fn block_comment_collapses() {
    check(
        "%%\nblock\ncomment\n%%\nafter‸",
        &element(),
        &[
            "[comment] ~collapsed~",
            "[comment] ~collapsed~",
            "[comment] ~collapsed~",
            "[comment] ~collapsed~",
            "after",
        ],
    );
}

#[test]
fn block_comment_shows_with_cursor() {
    check(
        "%%\nblo‸ck\n%%",
        &element(),
        &[
            "[comment] {comment,markup-dimmed:%%}",
            "[comment] {comment:block}",
            "[comment] {comment,markup-dimmed:%%}",
        ],
    );
}

#[test]
fn frontmatter_fences_collapse() {
    check(
        "---\ntitle: x\n---\nbody‸",
        &element(),
        &[
            "[frontmatter] ~collapsed~",
            "[frontmatter property { keyed: true }] {frontmatter,frontmatter-key:title}{frontmatter:x}",
            "[frontmatter] ~collapsed~",
            "body",
        ],
    );
}

#[test]
fn frontmatter_fences_show_with_cursor() {
    check(
        "---\nti‸tle: x\n---\nbody",
        &element(),
        &[
            "[frontmatter] {frontmatter,markup-dimmed:---}",
            "[frontmatter] {frontmatter,frontmatter-key:title}{frontmatter:: x}",
            "[frontmatter] {frontmatter,markup-dimmed:---}",
            "body",
        ],
    );
}

#[test]
fn html_elements() {
    check(
        "line<br>next <u>under</u> <hr> <span>x</span>\n\n‸",
        &element(),
        &[
            "line⟦linebreak⟧next {underline:under} ⟦horizontalrule⟧ x",
            "",
            "",
        ],
    );
}

#[test]
fn html_block_tags_hide() {
    check(
        "<div align=\"center\">\ncentered\n</div>\n\nz‸",
        &element(),
        &[
            "[align(center)] ~collapsed~",
            "[align(center)] centered",
            "[align(center)] ~collapsed~",
            "",
            "z",
        ],
    );
}

#[test]
fn thematic_break_becomes_rule() {
    check(
        "a\n\n***\n\nb‸",
        &element(),
        &["a", "", "⟦horizontalrule⟧", "", "b"],
    );
}

#[test]
fn thematic_break_shows_with_cursor() {
    check("a\n\n-‸--", &element(), &["a", "", "{markup-dimmed:---}"]);
}

#[test]
fn hard_break_backslash_hides() {
    check("a\\\nb\n\n‸", &element(), &["a", "b", "", ""]);
}

#[test]
fn always_shown_mode_dims_everything() {
    check(
        "# H *e* [l](u) $x$\n- [ ] t\n> q\n\n| a |\n|---|\n\n```\nc\n```",
        &shown(),
        &[
            "[h1] {heading-1,markup-dimmed:# }{heading-1:H }{emphasis,heading-1,markup-dimmed:*}{emphasis,heading-1:e}{emphasis,heading-1,markup-dimmed:*}{heading-1: }{heading-1,link,markup-dimmed:[}{heading-1,link:l}{heading-1,link,markup-dimmed:](u)}{heading-1: }{heading-1,math-source,markup-dimmed:$}{heading-1,math-source:x}{heading-1,math-source,markup-dimmed:$}",
            "{markup-dimmed:- [ ] }t",
            "[quote1] {markup-dimmed:> }q",
            "",
            "[table] {markup-dimmed:|} a {markup-dimmed:|}",
            "[table] {markup-dimmed:|---|}",
            "",
            "[code0] {markup-dimmed:```}",
            "[code1] {code-block:c}",
            "[code2] {markup-dimmed:```}",
        ],
    );
}

#[test]
fn always_hidden_mode_ignores_cursor() {
    check(
        "# H‸ *e* [l](u) $x$\n- [ ] t",
        &hidden(),
        &[
            "[h1] {heading-1:H }{emphasis,heading-1:e}{heading-1: }{heading-1,link:l}{heading-1: }⟦math:x⟧",
            "⟦•1⟧⟦[ ]⟧t",
        ],
    );
}

#[test]
fn line_scope_reveals_everything_on_the_line() {
    check(
        "**a** *b* ‸c\n**d**",
        &around(RevealScope::Line),
        &[
            "{strong,markup-dimmed:**}{strong:a}{strong,markup-dimmed:**} {emphasis,markup-dimmed:*}{emphasis:b}{emphasis,markup-dimmed:*} c",
            "{strong:d}",
        ],
    );
}

#[test]
fn block_scope_reveals_the_whole_paragraph() {
    check(
        "**a**\n*b* ‸c\n\n**d**",
        &around(RevealScope::Block),
        &[
            "{strong,markup-dimmed:**}{strong:a}{strong,markup-dimmed:**}",
            "{emphasis,markup-dimmed:*}{emphasis:b}{emphasis,markup-dimmed:*} c",
            "",
            "{strong:d}",
        ],
    );
}

#[test]
fn block_scope_reveals_the_whole_quote_markers() {
    check(
        "> a\n> b‸\n> c",
        &around(RevealScope::Block),
        &[
            "[quote1] {markup-dimmed:> }a",
            "[quote1] {markup-dimmed:> }b",
            "[quote1] {markup-dimmed:> }c",
        ],
    );
}

#[test]
fn element_scope_on_multiline_element() {
    check(
        "**a\nb‸**",
        &element(),
        &[
            "{strong,markup-dimmed:**}{strong:a}",
            "{strong:b}{strong,markup-dimmed:**}",
        ],
    );
}

#[test]
fn link_urls_stay_hidden_while_emphasis_follows_cursor() {
    check(
        "[*l‸ink*](http://x.y)",
        &element().with_override(SyntaxKind::LinkUrl, RevealMode::AlwaysHidden),
        &[
            "{link,markup-dimmed:[}{emphasis,link,markup-dimmed:*}{emphasis,link:link}{emphasis,link,markup-dimmed:*}{link,markup-dimmed:]}",
        ],
    );
}

#[test]
fn math_always_shown_override() {
    check(
        "a $x$ **b**\n\n‸",
        &element().with_override(SyntaxKind::Math, RevealMode::AlwaysShown),
        &[
            "a {math-source,markup-dimmed:$}{math-source:x}{math-source,markup-dimmed:$} {strong:b}",
            "",
            "",
        ],
    );
}

#[test]
fn headings_always_hidden_override() {
    check(
        "# Ti‸tle",
        &element().with_override(SyntaxKind::Heading, RevealMode::AlwaysHidden),
        &["[h1] {heading-1:Title}"],
    );
}

#[test]
fn unicode_text_keeps_char_boundaries() {
    check(
        "héllo **wörld** ==日本== #タグ\n\n‸",
        &element(),
        &["héllo {strong:wörld} {highlight:日本} {tag:#タグ}", "", ""],
    );
}

#[test]
fn every_cursor_reveals_its_own_element() {
    let text = "**a** *b* ~~c~~";
    let actual = render_with(text, &[1..1, 12..12], &element());
    assert_eq!(
        actual,
        "{strong,markup-dimmed:**}{strong:a}{strong,markup-dimmed:**} {emphasis:b} \
         {strikethrough,markup-dimmed:~~}{strikethrough:c}{strikethrough,markup-dimmed:~~}"
    );
}

#[test]
fn a_selection_reveals_everything_it_covers() {
    let text = "**a** *b* ~~c~~\n\n**d**";
    let backwards = Range { start: 8, end: 3 };
    let actual = render_with(text, std::slice::from_ref(&backwards), &element());
    let first_line = actual.lines().next().unwrap_or_default();
    assert!(first_line.contains("{strong,markup-dimmed:**}"), "{actual}");
    assert!(
        first_line.contains("{emphasis,markup-dimmed:*}"),
        "{actual}"
    );
    assert!(
        !first_line.contains("strikethrough,markup-dimmed"),
        "{actual}"
    );
    assert!(actual.ends_with("{strong:d}"), "{actual}");
}

#[test]
fn viewport_plans_match_the_whole_plan() {
    let text =
        "# T\n\n| a |\n|---|\n| 1 |\n\n$$\nx\n$$\n\n> [!note]- F\n> body\n\n- [ ] t\n\n**b** $y$";
    let tree = syntax::parse(text);
    let settings = element();
    let input = RenderInput {
        text,
        tree: &tree,
        selections: &[],
        settings: &settings,
    };
    let whole = plan(&input);
    for start in 0..whole.lines.len() {
        for end in start..=whole.lines.len() + 1 {
            let part = plan_lines(&input, start..end);
            let expected = &whole.lines[start..end.min(whole.lines.len())];
            assert_eq!(part.lines, expected, "lines {start}..{end}");
        }
    }
}

#[test]
fn crlf_lines_exclude_the_carriage_return() {
    check(
        "# Title\r\n**b**\r\n",
        &element(),
        &["[h1] {heading-1:Title}", "{strong:b}", ""],
    );
}

#[test]
fn style_keys_are_sorted_and_runs_merge() {
    let text = "***x***";
    let tree = syntax::parse(text);
    let settings = element();
    let plan = plan(&RenderInput {
        text,
        tree: &tree,
        selections: &[],
        settings: &settings,
    });
    let runs = &plan.lines[0].runs;
    assert_eq!(runs.len(), 3);
    assert_eq!(runs[1].styles, vec![StyleKey::Strong, StyleKey::Emphasis]);
    assert_eq!(plan.lines[0].hidden, vec![0..3, 4..7]);
}

const CONFLICT: &str =
    "Before\n<<<<<<< this device\nMilk and **eggs**\n=======\nCheese\n>>>>>>> other device\nAfter";

#[test]
fn sync_conflicts_label_each_version_and_hide_their_markers() {
    check(
        &format!("{CONFLICT}\n\n‸"),
        &element(),
        &[
            "Before",
            "[thisdevice] ⟦thisdevice⟧",
            "[thisdevice] Milk and {strong:eggs}",
            "[otherdevice] ⟦otherdevice⟧",
            "[otherdevice] Cheese",
            "[otherdevice] ~collapsed~",
            "After",
            "",
            "",
        ],
    );
}

#[test]
fn a_conflict_marker_shows_while_the_cursor_is_on_its_line() {
    let at = CONFLICT.find("=======").unwrap();
    let marked = format!("{}‸{}", &CONFLICT[..at], &CONFLICT[at..]);
    check(
        &marked,
        &element(),
        &[
            "Before",
            "[thisdevice] ⟦thisdevice⟧",
            "[thisdevice] Milk and {strong:eggs}",
            "[otherdevice] {markup-dimmed:=======}",
            "[otherdevice] Cheese",
            "[otherdevice] ~collapsed~",
            "After",
        ],
    );
}

#[test]
fn the_separator_never_makes_a_heading() {
    let tree = syntax::parse(CONFLICT);
    assert!(
        tree.nodes()
            .iter()
            .all(|node| !matches!(node.kind, syntax::NodeKind::Heading { .. })),
        "the line above ======= stays a paragraph"
    );
}

#[test]
fn frontmatter_reads_as_properties_away_from_the_cursor() {
    check(
        "---\ntitle: Waves\ntags:\n  - physics\n---\n\n‸",
        &element(),
        &[
            "[frontmatter] ~collapsed~",
            "[frontmatter property { keyed: true }] {frontmatter,frontmatter-key:title}{frontmatter:Waves}",
            "[frontmatter property { keyed: true }] {frontmatter,frontmatter-key:tags}⟦propertylist { items: [\"physics\"], tags: true }⟧",
            "[frontmatter] ~collapsed~",
            "[frontmatter] ~collapsed~",
            "",
            "",
        ],
    );
    check(
        "---\ntitle: ‸Waves\n---",
        &element(),
        &[
            "[frontmatter] {frontmatter,markup-dimmed:---}",
            "[frontmatter] {frontmatter,frontmatter-key:title}{frontmatter:: Waves}",
            "[frontmatter] {frontmatter,markup-dimmed:---}",
        ],
    );
}

#[test]
fn list_properties_read_as_chips() {
    check(
        "---\naliases: [Waves, \"Wave packets\"]\ntags: [physics]\nnote: [not closed\n---\n\n‸",
        &element(),
        &[
            "[frontmatter] ~collapsed~",
            "[frontmatter property { keyed: true }] {frontmatter,frontmatter-key:aliases}{frontmatter,property-chip:Waves}{frontmatter,property-chip:Wave packets}",
            "[frontmatter property { keyed: true }] {frontmatter,frontmatter-key:tags}{tag,frontmatter,property-chip:physics}",
            "[frontmatter property { keyed: true }] {frontmatter,frontmatter-key:note}{frontmatter:[not closed}",
            "[frontmatter] ~collapsed~",
            "",
            "",
        ],
    );
    check(
        "---\naliases:\n  - Waves\n  - \"\"\n---\n\n‸",
        &element(),
        &[
            "[frontmatter] ~collapsed~",
            "[frontmatter property { keyed: true }] {frontmatter,frontmatter-key:aliases}⟦propertylist { items: [\"waves\"], tags: false }⟧",
            "[frontmatter] ~collapsed~",
            "[frontmatter property { keyed: false }] {frontmatter:  - \"\"}",
            "[frontmatter] ~collapsed~",
            "",
            "",
        ],
    );
}

#[test]
fn the_empty_document_has_one_empty_line() {
    check("", &element(), &[""]);
}

#[test]
fn link_embed_blocks_become_cards_away_from_the_cursor() {
    let block = "```embed\ntitle: \"Rust\"\nurl: \"https://rust-lang.org\"\n```\n\n";
    let away = render(&format!("{block}‸"), &element());
    let lines: Vec<&str> = away.lines().collect();
    assert!(lines[0].starts_with("⟦linkcard("), "{away}");
    assert!(lines[0].contains("https://rust-lang.org"), "{away}");
    assert_eq!(lines[1..4], ["~collapsed~", "~collapsed~", "~collapsed~"]);
    let inside = render(&block.replacen("Rust", "R‸ust", 1), &element());
    assert!(!inside.contains("linkcard"), "the source shows: {inside}");
}

#[test]
fn embed_blocks_without_a_url_stay_code() {
    let away = render("```embed\ntitle: \"x\"\n```\n\n‸", &element());
    assert!(!away.contains("linkcard"), "{away}");
}

#[test]
fn styled_html_hides_its_tags_and_styles_its_text() {
    check(
        "<span style=\"color:red;\">abc</span> <b>b</b> <i>i</i> <a href=\"https://example.com\">site</a>\n\n<span style=”color:red;”>curly</span>\n\nz‸",
        &element(),
        &[
            "{html-color:abc} {strong:b} {emphasis:i} {link:site}",
            "",
            "{html-color:curly}",
            "",
            "z",
        ],
    );
}

#[test]
fn html_tags_show_around_the_cursor() {
    check(
        "<span style=\"color:red;\">a‸bc</span> <b>b</b>",
        &element(),
        &[
            "{html,markup-dimmed:<span style=\"color:red;\">}{html-color:abc}{html,markup-dimmed:</span>} {strong:b}",
        ],
    );
}

#[test]
fn html_blocks_align_their_lines() {
    check(
        "<p style=\"text-align: center;\">Mid</p>\n\n<center>Also</center>\n\n<div style=\"text-align: right\">\nRight\n</div>\n\nz‸",
        &element(),
        &[
            "[align(center)] Mid",
            "",
            "[align(center)] Also",
            "",
            "[align(right)] ~collapsed~",
            "[align(right)] Right",
            "[align(right)] ~collapsed~",
            "",
            "z",
        ],
    );
}

#[test]
fn inline_html_elements_and_nesting() {
    check(
        "<sup>up</sup><sub>down</sub> <kbd>Ctrl</kbd> <mark>m</mark> <s>s</s> <span style=\"color:red\"><b>x</b></span> <b>unclosed\n\nz‸",
        &element(),
        &[
            "{superscript:up}{subscript:down} {kbd:Ctrl} {highlight:m} {strikethrough:s} {strong,html-color:x} unclosed",
            "",
            "z",
        ],
    );
}

#[test]
fn html_style_keys_carry_their_values() {
    let text = "<span style=\"font-size:2em; background-color: #ff0\"><span style=\"font-size:50%; color: rgb(0,0,255)\">x</span></span> <span style=\"font-size:99px\">y</span>";
    let tree = syntax::parse(text);
    let plan = plan(&RenderInput {
        text,
        tree: &tree,
        selections: &[],
        settings: &element(),
    });
    let styles_at = |needle: &str| {
        let at = text.find(needle).unwrap();
        let run = plan.lines[0]
            .runs
            .iter()
            .find(|run| run.range.contains(&at))
            .unwrap();
        run.styles.clone()
    };
    let x = styles_at("x<");
    assert!(x.contains(&StyleKey::FontScale {
        depth: 1,
        percent: 100
    }));
    assert!(x.contains(&StyleKey::TextColor {
        depth: 1,
        rgba: 0x0000ffff
    }));
    assert!(x.contains(&StyleKey::TextBackground {
        depth: 0,
        rgba: 0xffff00ff
    }));
    assert!(styles_at("y<").contains(&StyleKey::FontScale {
        depth: 0,
        percent: 300
    }));
}

#[test]
fn the_styled_html_fixture_renders_its_subset() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/html/Styled HTML.md");
    let text = std::fs::read_to_string(path).unwrap();
    let tree = syntax::parse(&text);
    let plan = plan(&RenderInput {
        text: &text,
        tree: &tree,
        selections: &[],
        settings: &element(),
    });
    check_plan_invariants(&text, &plan);
    let styles_at = |needle: &str| -> Vec<StyleKey> {
        let at = text.find(needle).unwrap();
        plan.lines
            .iter()
            .flat_map(|line| &line.runs)
            .find(|run| run.range.contains(&at))
            .map(|run| run.styles.clone())
            .unwrap_or_default()
    };
    let has_color = |needle: &str| {
        styles_at(needle)
            .iter()
            .any(|style| matches!(style, StyleKey::TextColor { .. }))
    };
    assert!(has_color("red word"));
    assert!(has_color("still red"));
    assert!(has_color("navy and larger"));
    assert!(has_color("only\nthe colour"));
    assert!(styles_at("bold on a").contains(&StyleKey::Strong));
    assert!(styles_at("Ctrl").contains(&StyleKey::Kbd));
    assert!(styles_at("a link").contains(&StyleKey::Link));
    assert!(!styles_at("not a link").contains(&StyleKey::Link));
    assert!(styles_at("After the unclosed").is_empty());
    let aligned = |needle: &str| {
        let line = tree.lines().line_of(text.find(needle).unwrap());
        plan.lines[line]
            .line_styles
            .iter()
            .find_map(|style| match style {
                LineStyle::Align(align) => Some(*align),
                _ => None,
            })
    };
    assert_eq!(
        aligned("A centred paragraph"),
        Some(syntax::Alignment::Center)
    );
    assert_eq!(aligned("A centre element"), Some(syntax::Alignment::Center));
    assert_eq!(aligned("Right-aligned"), Some(syntax::Alignment::Right));
    assert_eq!(aligned("After the unclosed"), None);
}
