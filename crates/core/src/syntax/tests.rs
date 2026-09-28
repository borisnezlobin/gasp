//! Snapshot-style tests of the parser: each dumps the tree as indented text.

use std::fmt::Write;

use super::*;

/// One line per node: kind, quoted source, then markup tokens.
pub(crate) fn dump(text: &str) -> String {
    let tree = parse(text);
    assert_well_formed(text, &tree);
    let mut out = String::new();
    for &block in tree.blocks() {
        dump_node(&tree, text, block, 0, &mut out);
    }
    out.trim_end().to_owned()
}

/// Checks that ranges nest, sit on char boundaries inside the text, that
/// siblings are ordered without overlap, and that parent links agree.
pub(crate) fn assert_well_formed(text: &str, tree: &SyntaxTree) {
    assert_eq!(tree.root().range, 0..text.len());
    for (index, node) in tree.nodes().iter().enumerate() {
        let context = || format!("node {index} {:?} {:?}", node.kind, node.range);
        assert!(
            node.range.start <= node.range.end && node.range.end <= text.len(),
            "{}",
            context()
        );
        assert!(
            text.is_char_boundary(node.range.start) && text.is_char_boundary(node.range.end),
            "{}",
            context()
        );
        for token in node.markup.iter().map(|m| &m.range).chain(&node.content) {
            assert!(
                node.range.start <= token.start && token.end <= node.range.end,
                "{}",
                context()
            );
            assert!(
                text.is_char_boundary(token.start) && text.is_char_boundary(token.end),
                "{}",
                context()
            );
        }
        let mut previous_end = node.range.start;
        for &child in &node.children {
            let child_node = tree.node(child);
            assert_eq!(child_node.parent, Some(NodeId(index)), "{}", context());
            assert!(
                child_node.range.start >= previous_end,
                "{} child {:?}",
                context(),
                child_node.range
            );
            assert!(
                child_node.range.end <= node.range.end,
                "{} child {:?}",
                context(),
                child_node.range
            );
            previous_end = child_node.range.end;
        }
    }
}

#[track_caller]
fn check(text: &str, expected: &[&str]) {
    let actual = dump(text);
    if expected.is_empty() {
        println!("SNAP\n{actual}\nENDSNAP");
    }
    let actual_lines: Vec<&str> = actual.split('\n').collect();
    assert_eq!(actual_lines, expected, "\n--- actual ---\n{actual}\n");
}

fn kind_label(kind: &NodeKind) -> String {
    let debug = format!("{kind:?}");
    match kind {
        NodeKind::Heading { level, .. } => format!("Heading{level}"),
        NodeKind::Callout(info) => format!("Callout({:?},{:?})", info.kind, info.fold),
        NodeKind::CodeBlock(info) => format!("CodeBlock({:?},{:?})", info.language, info.title),
        NodeKind::Link(info) | NodeKind::Image(info) => {
            let name = debug.split('(').next().unwrap_or_default().to_owned();
            format!("{name}({:?},{})", info.kind, info.destination)
        }
        _ => debug
            .replace(" {", "{")
            .replace(", ", ",")
            .replace("{ ", "{")
            .replace(" }", "}"),
    }
}

fn dump_node(tree: &SyntaxTree, text: &str, id: NodeId, depth: usize, out: &mut String) {
    let node = tree.node(id);
    let markup: Vec<String> = node
        .markup
        .iter()
        .map(|m| format!("{:?}{:?}", m.kind, &text[m.range.clone()]))
        .collect();
    let _ = write!(
        out,
        "{}{} {:?}",
        "  ".repeat(depth),
        kind_label(&node.kind),
        &text[node.range.clone()]
    );
    if !markup.is_empty() {
        let _ = write!(out, " [{}]", markup.join(" "));
    }
    out.push('\n');
    for &child in &node.children {
        dump_node(tree, text, child, depth + 1, out);
    }
}

#[test]
fn atx_and_setext_headings() {
    check(
        "# One #\n## Two *em*\n\nThree\n---\n#notheading",
        &[
            "Heading1 \"# One #\" [HeadingMarker\"# \" HeadingMarker\" #\"]",
            "  Text \"One\"",
            "Heading2 \"## Two *em*\" [HeadingMarker\"## \"]",
            "  Text \"Two \"",
            "  Emphasis \"*em*\" [EmphasisDelimiter\"*\" EmphasisDelimiter\"*\"]",
            "    Text \"em\"",
            "Heading2 \"Three\\n---\" [HeadingMarker\"---\"]",
            "  Text \"Three\"",
            "Paragraph \"#notheading\"",
            "  Tag{name: \"notheading\"} \"#notheading\"",
        ],
    );
}

#[test]
fn paragraph_with_hard_and_soft_breaks() {
    check(
        "a\\\nb  \nc\nd",
        &[
            "Paragraph \"a\\\\\\nb  \\nc\\nd\"",
            "  Text \"a\"",
            "  HardBreak \"\\\\\\n\" [HardBreakMarker\"\\\\\"]",
            "  Text \"b\"",
            "  HardBreak \"  \\n\" [HardBreakMarker\"  \"]",
            "  Text \"c\"",
            "  SoftBreak \"\\n\"",
            "  Text \"d\"",
        ],
    );
}

#[test]
fn nested_lists_and_markers() {
    check(
        "- a\n  - b\n    continued\n1. one\n2) two",
        &[
            "List{ordered: false,start: None} \"- a\\n  - b\\n    continued\"",
            "  ListItem{task: None} \"- a\\n  - b\\n    continued\" [ListMarker\"- \"]",
            "    Text \"a\"",
            "    List{ordered: false,start: None} \"- b\\n    continued\"",
            "      ListItem{task: None} \"- b\\n    continued\" [ListMarker\"- \"]",
            "        Text \"b\"",
            "        SoftBreak \"\\n\"",
            "        Text \"continued\"",
            "List{ordered: true,start: Some(1)} \"1. one\"",
            "  ListItem{task: None} \"1. one\" [ListMarker\"1. \"]",
            "    Text \"one\"",
            "List{ordered: true,start: Some(2)} \"2) two\"",
            "  ListItem{task: None} \"2) two\" [ListMarker\"2) \"]",
            "    Text \"two\"",
        ],
    );
}

#[test]
fn task_items() {
    check(
        "- [ ] open\n- [x] done\n\n  loose para",
        &[
            "List{ordered: false,start: None} \"- [ ] open\\n- [x] done\\n\\n  loose para\"",
            "  ListItem{task: Some(false)} \"- [ ] open\" [ListMarker\"- \" TaskMarker\"[ ] \"]",
            "    Paragraph \"open\"",
            "      Text \"open\"",
            "  ListItem{task: Some(true)} \"- [x] done\\n\\n  loose para\" [ListMarker\"- \" TaskMarker\"[x] \"]",
            "    Paragraph \"done\"",
            "      Text \"done\"",
            "    Paragraph \"loose para\"",
            "      Text \"loose para\"",
        ],
    );
}

#[test]
fn nested_blockquotes() {
    check(
        "> a\n> > b\n> > c\nlazy",
        &[
            "BlockQuote \"> a\\n> > b\\n> > c\\nlazy\" [QuoteMarker\"> \" QuoteMarker\"> \" QuoteMarker\"> \"]",
            "  Paragraph \"a\"",
            "    Text \"a\"",
            "  BlockQuote \"> b\\n> > c\\nlazy\" [QuoteMarker\"> \" QuoteMarker\"> \"]",
            "    Paragraph \"b\\n> > c\\nlazy\"",
            "      Text \"b\"",
            "      SoftBreak \"\\n\"",
            "      Text \"c\"",
            "      SoftBreak \"\\n\"",
            "      Text \"lazy\"",
        ],
    );
}

#[test]
fn callout_with_fold_and_inline_title() {
    check(
        "> [!NOTE]+ The *title*\n> body line\n> more",
        &[
            "Callout(Note,Some(Open)) \"> [!NOTE]+ The *title*\\n> body line\\n> more\" [QuoteMarker\"> \" CalloutHeader\"[!NOTE]+ \" QuoteMarker\"> \" QuoteMarker\"> \"]",
            "  CalloutTitle \"The *title*\"",
            "    Text \"The \"",
            "    Emphasis \"*title*\" [EmphasisDelimiter\"*\" EmphasisDelimiter\"*\"]",
            "      Text \"title\"",
            "  Paragraph \"body line\\n> more\"",
            "    Text \"body line\"",
            "    SoftBreak \"\\n\"",
            "    Text \"more\"",
        ],
    );
}

#[test]
fn callout_custom_type_and_no_title() {
    check(
        "> [!my-type]\n> body",
        &[
            "Callout(Custom,None) \"> [!my-type]\\n> body\" [QuoteMarker\"> \" CalloutHeader\"[!my-type]\" QuoteMarker\"> \"]",
            "  Paragraph \"body\"",
            "    Text \"body\"",
        ],
    );
}

#[test]
fn callout_title_only() {
    check(
        "> [!warning] Careful",
        &[
            "Callout(Warning,None) \"> [!warning] Careful\" [QuoteMarker\"> \" CalloutHeader\"[!warning] \"]",
            "  CalloutTitle \"Careful\"",
            "    Text \"Careful\"",
        ],
    );
}

#[test]
fn nested_callout_in_callout() {
    check(
        "> [!info] Outer\n> > [!danger]- Inner\n> > text",
        &[
            "Callout(Info,None) \"> [!info] Outer\\n> > [!danger]- Inner\\n> > text\" [QuoteMarker\"> \" CalloutHeader\"[!info] \" QuoteMarker\"> \" QuoteMarker\"> \"]",
            "  CalloutTitle \"Outer\"",
            "    Text \"Outer\"",
            "  Callout(Danger,Some(Closed)) \"> [!danger]- Inner\\n> > text\" [QuoteMarker\"> \" CalloutHeader\"[!danger]- \" QuoteMarker\"> \"]",
            "    CalloutTitle \"Inner\"",
            "      Text \"Inner\"",
            "    Paragraph \"text\"",
            "      Text \"text\"",
        ],
    );
}

#[test]
fn callout_with_list_body() {
    check(
        "> [!todo]\n> - [ ] item",
        &[
            "Callout(Todo,None) \"> [!todo]\\n> - [ ] item\" [QuoteMarker\"> \" CalloutHeader\"[!todo]\" QuoteMarker\"> \"]",
            "  List{ordered: false,start: None} \"- [ ] item\"",
            "    ListItem{task: Some(false)} \"- [ ] item\" [ListMarker\"- \" TaskMarker\"[ ] \"]",
            "      Text \"item\"",
        ],
    );
}

#[test]
fn quote_that_is_not_a_callout() {
    check(
        "> [link] text\n> [!note] later",
        &[
            "BlockQuote \"> [link] text\\n> [!note] later\" [QuoteMarker\"> \" QuoteMarker\"> \"]",
            "  Paragraph \"[link] text\\n> [!note] later\"",
            "    Text \"[link] text\"",
            "    SoftBreak \"\\n\"",
            "    Text \"[!note] later\"",
        ],
    );
}

#[test]
fn fenced_code_with_title() {
    check(
        "```python title:\"a b.py\" hl:2\nprint(1)\n\nx\n```",
        &[
            "CodeBlock(Some(\"python\"),Some(\"a b.py\")) \"```python title:\\\"a b.py\\\" hl:2\\nprint(1)\\n\\nx\\n```\" [CodeFence\"```python title:\\\"a b.py\\\" hl:2\" CodeFence\"```\"]",
            "  Text \"print(1)\\n\\nx\"",
        ],
    );
}

#[test]
fn tilde_fence_and_unclosed_fence() {
    check(
        "~~~\ncode\n~~~\n\n```js\nopen",
        &[
            "CodeBlock(None,None) \"~~~\\ncode\\n~~~\" [CodeFence\"~~~\" CodeFence\"~~~\"]",
            "  Text \"code\"",
            "CodeBlock(Some(\"js\"),None) \"```js\\nopen\" [CodeFence\"```js\"]",
            "  Text \"open\"",
        ],
    );
}

#[test]
fn indented_code() {
    check(
        "para\n\n    code line\n    more",
        &[
            "Paragraph \"para\"",
            "  Text \"para\"",
            "CodeBlock(None,None) \"code line\\n    more\"",
            "  Text \"code line\"",
            "  Text \"more\"",
        ],
    );
}

#[test]
fn code_in_list_and_quote() {
    check(
        "- item\n\n  ```\n  x\n  ```\n> ```\n> y\n> ```",
        &[
            "List{ordered: false,start: None} \"- item\\n\\n  ```\\n  x\\n  ```\"",
            "  ListItem{task: None} \"- item\\n\\n  ```\\n  x\\n  ```\" [ListMarker\"- \"]",
            "    Paragraph \"item\"",
            "      Text \"item\"",
            "    CodeBlock(None,None) \"```\\n  x\\n  ```\" [CodeFence\"```\" CodeFence\"```\"]",
            "      Text \"x\"",
            "BlockQuote \"> ```\\n> y\\n> ```\" [QuoteMarker\"> \" QuoteMarker\"> \" QuoteMarker\"> \"]",
            "  CodeBlock(None,None) \"```\\n> y\\n> ```\" [CodeFence\"```\" CodeFence\"```\"]",
            "    Text \"y\"",
        ],
    );
}

#[test]
fn table_with_alignment_and_escaped_pipe() {
    check(
        "| a | b \\| c | d |\n|:--|:-:|--:|\n| 1 | **2** | $x$ |",
        &[
            "Table{alignments: [Left,Center,Right]} \"| a | b \\\\| c | d |\\n|:--|:-:|--:|\\n| 1 | **2** | $x$ |\" [TableDelimiterRow\"|:--|:-:|--:|\"]",
            "  TableHead \"| a | b \\\\| c | d |\" [TablePipe\"|\" TablePipe\"|\" TablePipe\"|\" TablePipe\"|\"]",
            "    TableCell \" a \"",
            "      Text \"a\"",
            "    TableCell \" b \\\\| c \"",
            "      Text \"b \"",
            "      Text \"| c\"",
            "    TableCell \" d \"",
            "      Text \"d\"",
            "  TableRow \"| 1 | **2** | $x$ |\" [TablePipe\"|\" TablePipe\"|\" TablePipe\"|\" TablePipe\"|\"]",
            "    TableCell \" 1 \"",
            "      Text \"1\"",
            "    TableCell \" **2** \"",
            "      Strong \"**2**\" [StrongDelimiter\"**\" StrongDelimiter\"**\"]",
            "        Text \"2\"",
            "    TableCell \" $x$ \"",
            "      Math{display: false} \"$x$\" [MathDelimiter\"$\" MathDelimiter\"$\"]",
        ],
    );
}

#[test]
fn footnotes() {
    check(
        "Text[^1] and[^note].\n\n[^1]: First.\n[^note]: Second\n    continued.",
        &[
            "Paragraph \"Text[^1] and[^note].\"",
            "  Text \"Text\"",
            "  FootnoteReference{label: \"1\"} \"[^1]\" [FootnoteMarker\"[^\" FootnoteMarker\"]\"]",
            "  Text \" and\"",
            "  FootnoteReference{label: \"note\"} \"[^note]\" [FootnoteMarker\"[^\" FootnoteMarker\"]\"]",
            "  Text \".\"",
            "FootnoteDefinition{label: \"1\"} \"[^1]: First.\" [FootnoteMarker\"[^1]: \"]",
            "  Paragraph \"First.\"",
            "    Text \"First.\"",
            "FootnoteDefinition{label: \"note\"} \"[^note]: Second\\n    continued.\" [FootnoteMarker\"[^note]: \"]",
            "  Paragraph \"Second\\n    continued.\"",
            "    Text \"Second\"",
            "    SoftBreak \"\\n\"",
            "    Text \"continued.\"",
        ],
    );
}

#[test]
fn inline_and_display_math() {
    check(
        "a $x+1$ b $$\\sum$$ c $5 and $6 d \\$e$",
        &[
            "Paragraph \"a $x+1$ b $$\\\\sum$$ c $5 and $6 d \\\\$e$\"",
            "  Text \"a \"",
            "  Math{display: false} \"$x+1$\" [MathDelimiter\"$\" MathDelimiter\"$\"]",
            "  Text \" b \"",
            "  Math{display: true} \"$$\\\\sum$$\" [MathDelimiter\"$$\" MathDelimiter\"$$\"]",
            "  Text \" c $5 and \"",
            "  Math{display: false} \"$6 d \\\\$e$\" [MathDelimiter\"$\" MathDelimiter\"$\"]",
        ],
    );
}

#[test]
fn block_math_in_list() {
    check(
        "- item\n  $$\n  x^2\n  $$",
        &[
            "List{ordered: false,start: None} \"- item\\n  $$\\n  x^2\\n  $$\"",
            "  ListItem{task: None} \"- item\\n  $$\\n  x^2\\n  $$\" [ListMarker\"- \"]",
            "    Text \"item\"",
            "    SoftBreak \"\\n\"",
            "    Math{display: true} \"$$\\n  x^2\\n  $$\" [MathDelimiter\"$$\" MathDelimiter\"$$\"]",
        ],
    );
}

#[test]
fn empty_double_dollar_is_math() {
    check(
        "mk $$ here",
        &[
            "Paragraph \"mk $$ here\"",
            "  Text \"mk \"",
            "  Math{display: false} \"$$\" [MathDelimiter\"$\" MathDelimiter\"$\"]",
            "  Text \" here\"",
        ],
    );
}

#[test]
fn highlights() {
    check(
        "==one== and ==two *em*== and a == b and ==c==",
        &[
            "Paragraph \"==one== and ==two *em*== and a == b and ==c==\"",
            "  Highlight \"==one==\" [HighlightDelimiter\"==\" HighlightDelimiter\"==\"]",
            "    Text \"one\"",
            "  Text \" and \"",
            "  Highlight \"==two *em*==\" [HighlightDelimiter\"==\" HighlightDelimiter\"==\"]",
            "    Text \"two \"",
            "    Emphasis \"*em*\" [EmphasisDelimiter\"*\" EmphasisDelimiter\"*\"]",
            "      Text \"em\"",
            "  Text \" and a == b and \"",
            "  Highlight \"==c==\" [HighlightDelimiter\"==\" HighlightDelimiter\"==\"]",
            "    Text \"c\"",
        ],
    );
}

#[test]
fn highlight_across_emphasis_boundaries() {
    check(
        "*a ==b* c==",
        &[
            "Paragraph \"*a ==b* c==\"",
            "  Emphasis \"*a ==b*\" [EmphasisDelimiter\"*\" EmphasisDelimiter\"*\"]",
            "    Text \"a ==b\"",
            "  Text \" c==\"",
        ],
    );
}

#[test]
fn inline_comments() {
    check(
        "a %%one%% b %%two\nlines%% c",
        &[
            "Paragraph \"a %%one%% b %%two\\nlines%% c\"",
            "  Text \"a \"",
            "  Comment \"%%one%%\" [CommentDelimiter\"%%\" CommentDelimiter\"%%\"]",
            "    Text \"one\"",
            "  Text \" b \"",
            "  Comment \"%%two\\nlines%%\" [CommentDelimiter\"%%\" CommentDelimiter\"%%\"]",
            "    Text \"two\"",
            "    SoftBreak \"\\n\"",
            "    Text \"lines\"",
            "  Text \" c\"",
        ],
    );
}

#[test]
fn block_comment_and_unclosed_comment() {
    check(
        "para\n%%\n# not heading\n%%\nafter\n\n%% open\nrest",
        &[
            "Paragraph \"para\"",
            "  Text \"para\"",
            "CommentBlock \"%%\\n# not heading\\n%%\" [CommentDelimiter\"%%\" CommentDelimiter\"%%\"]",
            "Paragraph \"after\"",
            "  Text \"after\"",
            "CommentBlock \"%% open\\nrest\" [CommentDelimiter\"%%\"]",
        ],
    );
}

#[test]
fn tags() {
    check(
        "#tag text #nested/tag, #123 #a1 x#no `#code` [#link](u) https://x.y/#frag",
        &[
            "Paragraph \"#tag text #nested/tag, #123 #a1 x#no `#code` [#link](u) https://x.y/#frag\"",
            "  Tag{name: \"tag\"} \"#tag\"",
            "  Text \" text \"",
            "  Tag{name: \"nested/tag\"} \"#nested/tag\"",
            "  Text \", #123 \"",
            "  Tag{name: \"a1\"} \"#a1\"",
            "  Text \" x#no \"",
            "  Code \"`#code`\" [CodeDelimiter\"`\" CodeDelimiter\"`\"]",
            "  Text \" \"",
            "  Link(Inline,u) \"[#link](u)\" [LinkBracket\"[\" LinkBracket\"]\" LinkDestination\"(u)\"]",
            "    Text \"#link\"",
            "  Text \" \"",
            "  Link(BareUrl,https://x.y/#frag) \"https://x.y/#frag\"",
        ],
    );
}

#[test]
fn tag_in_heading() {
    check(
        "# Title #tag",
        &[
            "Heading1 \"# Title #tag\" [HeadingMarker\"# \"]",
            "  Text \"Title \"",
            "  Tag{name: \"tag\"} \"#tag\"",
        ],
    );
}

#[test]
fn wikilinks_and_embeds() {
    check(
        "[[note]] [[note#Head|Alias]] [[#local]] ![[img.png|300x200]] ![[doc.pdf#page=2]] ![[a.png|caption]]",
        &[
            "Paragraph \"[[note]] [[note#Head|Alias]] [[#local]] ![[img.png|300x200]] ![[doc.pdf#page=2]] ![[a.png|caption]]\"",
            "  WikiLink(WikiInfo{target: \"note\",subpath: None,alias: None,size: None}) \"[[note]]\" [WikiBracket\"[[\" WikiBracket\"]]\"]",
            "    Text \"note\"",
            "  Text \" \"",
            "  WikiLink(WikiInfo{target: \"note\",subpath: Some(\"Head\"),alias: Some(\"Alias\"),size: None}) \"[[note#Head|Alias]]\" [WikiBracket\"[[\" WikiTarget\"note#Head|\" WikiBracket\"]]\"]",
            "    Text \"Alias\"",
            "  Text \" \"",
            "  WikiLink(WikiInfo{target: \"\",subpath: Some(\"local\"),alias: None,size: None}) \"[[#local]]\" [WikiBracket\"[[\" WikiSubpath\"#\" WikiBracket\"]]\"]",
            "    Text \"local\"",
            "  Text \" \"",
            "  Embed(WikiInfo{target: \"img.png\",subpath: None,alias: None,size: Some((300,Some(200)))}) \"![[img.png|300x200]]\" [WikiBracket\"![[\" EmbedSize\"|300x200\" WikiBracket\"]]\"]",
            "  Text \" \"",
            "  Embed(WikiInfo{target: \"doc.pdf\",subpath: Some(\"page=2\"),alias: None,size: None}) \"![[doc.pdf#page=2]]\" [WikiBracket\"![[\" WikiBracket\"]]\"]",
            "    Text \"doc.pdf#page=2\"",
            "  Text \" \"",
            "  Embed(WikiInfo{target: \"a.png\",subpath: None,alias: Some(\"caption\"),size: None}) \"![[a.png|caption]]\" [WikiBracket\"![[\" WikiTarget\"a.png|\" WikiBracket\"]]\"]",
            "    Text \"caption\"",
        ],
    );
}

#[test]
fn markdown_links() {
    check(
        "[a](http://x \"t\") [b][ref] [c][] [ref] <http://auto> <me@x.y> ![img](p.png)\n\n[ref]: http://r",
        &[
            "Paragraph \"[a](http://x \\\"t\\\") [b][ref] [c][] [ref] <http://auto> <me@x.y> ![img](p.png)\"",
            "  Link(Inline,http://x) \"[a](http://x \\\"t\\\")\" [LinkBracket\"[\" LinkBracket\"]\" LinkDestination\"(http://x \\\"t\\\")\"]",
            "    Text \"a\"",
            "  Text \" \"",
            "  Link(Reference,http://r) \"[b][ref]\" [LinkBracket\"[\" LinkBracket\"]\" LinkDestination\"[ref]\"]",
            "    Text \"b\"",
            "  Text \" [c][] \"",
            "  Link(Shortcut,http://r) \"[ref]\" [LinkBracket\"[\" LinkBracket\"]\"]",
            "    Text \"ref\"",
            "  Text \" \"",
            "  Link(Autolink,http://auto) \"<http://auto>\" [LinkBracket\"<\" LinkBracket\">\"]",
            "    Text \"http://auto\"",
            "  Text \" \"",
            "  Link(Email,me@x.y) \"<me@x.y>\" [LinkBracket\"<\" LinkBracket\">\"]",
            "    Text \"me@x.y\"",
            "  Text \" \"",
            "  Image(Inline,p.png) \"![img](p.png)\" [LinkBracket\"![\" LinkBracket\"]\" LinkDestination\"(p.png)\"]",
            "    Text \"img\"",
            "LinkDefinition{label: \"ref\"} \"[ref]: http://r\" [LinkBracket\"[ref]: \" LinkDestination\"http://r\"]",
        ],
    );
}

#[test]
fn bare_urls() {
    check(
        "see https://example.com/path?q=1. or www.example.org, not xhttp://no",
        &[
            "Paragraph \"see https://example.com/path?q=1. or www.example.org, not xhttp://no\"",
            "  Text \"see \"",
            "  Link(BareUrl,https://example.com/path?q=1) \"https://example.com/path?q=1\"",
            "  Text \". or \"",
            "  Link(BareUrl,http://www.example.org) \"www.example.org\"",
            "  Text \", not xhttp://no\"",
        ],
    );
}

#[test]
fn inline_html() {
    check(
        "a<br>b <u>under *em*</u> <span class=\"x\">s</span> <b>unclosed",
        &[
            "Paragraph \"a<br>b <u>under *em*</u> <span class=\\\"x\\\">s</span> <b>unclosed\"",
            "  Text \"a\"",
            "  Html(LineBreak) \"<br>\" [HtmlTag\"<br>\"]",
            "  Text \"b \"",
            "  Html(Underline) \"<u>under *em*</u>\" [HtmlTag\"<u>\" HtmlTag\"</u>\"]",
            "    Text \"under \"",
            "    Emphasis \"*em*\" [EmphasisDelimiter\"*\" EmphasisDelimiter\"*\"]",
            "      Text \"em\"",
            "  Text \" \"",
            "  Html(Span({})) \"<span class=\\\"x\\\">s</span>\" [HtmlTag\"<span class=\\\"x\\\">\" HtmlTag\"</span>\"]",
            "    Text \"s\"",
            "  Text \" \"",
            "  Html(Bold) \"<b>\" [HtmlTag\"<b>\"]",
            "  Text \"unclosed\"",
        ],
    );
}

#[test]
fn html_blocks() {
    check(
        "<div align=\"center\">\n<img src=\"a.png\">\n</div>\n\n<hr>\n\n<br>\n\n<!-- note -->",
        &[
            "HtmlBlock(Div({text-align: center})) \"<div align=\\\"center\\\">\\n<img src=\\\"a.png\\\">\\n</div>\"",
            "  Html(Div({text-align: center})) \"<div align=\\\"center\\\">\\n<img src=\\\"a.png\\\">\\n</div>\" [HtmlTag\"<div align=\\\"center\\\">\" HtmlTag\"</div>\"]",
            "    Html(Image) \"<img src=\\\"a.png\\\">\"",
            "HtmlBlock(HorizontalRule) \"<hr>\"",
            "  Html(HorizontalRule) \"<hr>\"",
            "HtmlBlock(LineBreak) \"<br>\"",
            "  Html(LineBreak) \"<br>\"",
            "HtmlBlock(Comment) \"<!-- note -->\"",
            "  Html(Comment) \"<!-- note -->\"",
        ],
    );
}

#[test]
fn frontmatter() {
    check(
        "---\ntitle: x\ntags: [a]\n---\n# Body",
        &[
            "Frontmatter \"---\\ntitle: x\\ntags: [a]\\n---\" [FrontmatterFence\"---\" FrontmatterFence\"---\"]",
            "Heading1 \"# Body\" [HeadingMarker\"# \"]",
            "  Text \"Body\"",
        ],
    );
}

#[test]
fn frontmatter_needs_to_close() {
    check(
        "---\ntitle: x\n\nbody",
        &[
            "ThematicBreak \"---\" [ThematicBreak\"---\"]",
            "Paragraph \"title: x\"",
            "  Text \"title: x\"",
            "Paragraph \"body\"",
            "  Text \"body\"",
        ],
    );
}

#[test]
fn thematic_breaks() {
    check(
        "***\n\n---\n\n_ _ _",
        &[
            "ThematicBreak \"***\" [ThematicBreak\"***\"]",
            "ThematicBreak \"---\" [ThematicBreak\"---\"]",
            "ThematicBreak \"_ _ _\" [ThematicBreak\"_ _ _\"]",
        ],
    );
}

#[test]
fn crlf_line_endings() {
    check(
        "# Head\r\n\r\n- a\r\n- b\r\n",
        &[
            "Heading1 \"# Head\" [HeadingMarker\"# \"]",
            "  Text \"Head\"",
            "List{ordered: false,start: None} \"- a\\r\\n- b\"",
            "  ListItem{task: None} \"- a\" [ListMarker\"- \"]",
            "    Text \"a\"",
            "  ListItem{task: None} \"- b\" [ListMarker\"- \"]",
            "    Text \"b\"",
        ],
    );
}

#[test]
fn unicode_content() {
    check(
        "# Ünïcödé ==日本語== #タグ\n\n> [!note] Tïtle\n> ✓ $α$",
        &[
            "Heading1 \"# Ünïcödé ==日本語== #タグ\" [HeadingMarker\"# \"]",
            "  Text \"Ünïcödé \"",
            "  Highlight \"==日本語==\" [HighlightDelimiter\"==\" HighlightDelimiter\"==\"]",
            "    Text \"日本語\"",
            "  Text \" \"",
            "  Tag{name: \"タグ\"} \"#タグ\"",
            "Callout(Note,None) \"> [!note] Tïtle\\n> ✓ $α$\" [QuoteMarker\"> \" CalloutHeader\"[!note] \" QuoteMarker\"> \"]",
            "  CalloutTitle \"Tïtle\"",
            "    Text \"Tïtle\"",
            "  Paragraph \"✓ $α$\"",
            "    Text \"✓ \"",
            "    Math{display: false} \"$α$\" [MathDelimiter\"$\" MathDelimiter\"$\"]",
        ],
    );
}

#[test]
fn context_at_reports_the_innermost_construct() {
    let text =
        "a $x$ `c` [t](http://u) [[w]] %%c%% <b>h</b>\n\n| $m$ | t |\n|---|---|\n\n```\ncode\n```";
    let tree = parse(text);
    let at = |needle: &str, offset: usize| tree.context_at(text.find(needle).unwrap() + offset);
    assert_eq!(at("a $", 0), InputContext::Text);
    assert_eq!(
        at("$x$", 0),
        InputContext::Text,
        "before the opening delimiter"
    );
    assert_eq!(at("$x$", 1), InputContext::Math);
    assert_eq!(at("$x$", 2), InputContext::Math);
    assert_eq!(
        at("$x$", 3),
        InputContext::Text,
        "after the closing delimiter"
    );
}

#[test]
fn math_at_finds_the_source_between_the_delimiters() {
    let text = "a $x+1$ b\n\n$$\n\\frac{1}{2}\n$$\n\n$y";
    let tree = parse(text);
    let inline = tree.math_at(text.find('x').unwrap()).unwrap();
    assert_eq!(&text[inline.inner.clone()], "x+1");
    assert!(!inline.block);
    let block = tree.math_at(text.find("frac").unwrap()).unwrap();
    assert_eq!(&text[block.inner.clone()], "\n\\frac{1}{2}\n");
    assert!(block.block);
    assert!(tree.math_at(text.find('b').unwrap()).is_none());
}

#[test]
fn context_at_in_code_links_comments_and_tables() {
    let text =
        "a $x$ `c` [t](http://u) [[w]] %%c%% <b>h</b>\n\n| $m$ | t |\n|---|---|\n\n```\ncode\n```";
    let tree = parse(text);
    let at = |needle: &str, offset: usize| tree.context_at(text.find(needle).unwrap() + offset);
    assert_eq!(at("`c`", 1), InputContext::Code);
    assert_eq!(at("[t]", 1), InputContext::Text, "link text is text");
    assert_eq!(at("http://u", 2), InputContext::Link);
    assert_eq!(at("[[w]]", 2), InputContext::Link);
    assert_eq!(at("%%c", 2), InputContext::Comment);
    assert_eq!(at("<b>", 1), InputContext::Html);
    assert_eq!(at(">h<", 1), InputContext::Text, "inside a paired element");
    assert_eq!(at("$m$", 1), InputContext::Math, "math inside a table");
    assert_eq!(at(" t |", 1), InputContext::Table);
    assert_eq!(at("code", 1), InputContext::Code);
}

#[test]
fn context_at_in_frontmatter_blocks_and_empty_math() {
    let text = "---\na: 1\n---\n%%\nhidden\n%%\n\nmk $$ \n\n<div>\nraw\n</div>";
    let tree = parse(text);
    let at = |needle: &str, offset: usize| tree.context_at(text.find(needle).unwrap() + offset);
    assert_eq!(at("a: 1", 1), InputContext::Frontmatter);
    assert_eq!(at("hidden", 1), InputContext::Comment);
    assert_eq!(
        at("$$ ", 1),
        InputContext::Math,
        "between a freshly typed pair"
    );
    assert_eq!(at("raw", 1), InputContext::Text, "a div's text is prose");
    assert_eq!(at("<div>", 2), InputContext::Html);
}

#[test]
fn syntax_tree_is_a_context_provider() {
    use crate::pipeline::ContextProvider;
    let text = "x $y$";
    let doc = crate::document::Document::from(text);
    let provider: &dyn ContextProvider = &parse(text);
    assert_eq!(provider.context_at(&doc, 3), InputContext::Math);
}

#[test]
fn path_at_prefers_the_node_starting_at_the_offset() {
    let text = "*a*b";
    let tree = parse(text);
    let path = tree.path_at(3);
    let innermost = tree.node(*path.last().unwrap());
    assert_eq!(innermost.kind, NodeKind::Text);
    assert_eq!(innermost.range, 3..4);
}

#[test]
fn content_is_the_range_minus_markup() {
    let text = "**bold** [t](u)";
    let tree = parse(text);
    let strong = tree
        .nodes()
        .iter()
        .find(|n| n.kind == NodeKind::Strong)
        .unwrap();
    assert_eq!(strong.content, vec![2..6]);
    let link = tree
        .nodes()
        .iter()
        .find(|n| matches!(n.kind, NodeKind::Link(_)))
        .unwrap();
    assert_eq!(link.content, vec![10..11]);
}

/// A tiny deterministic generator so the randomized tests need no crates.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) as usize) % bound.max(1)
    }
}

const FRAGMENTS: &[&str] = &[
    "# ",
    "## ",
    "\n",
    "\n\n",
    "- ",
    "  ",
    "1. ",
    "- [ ] ",
    "- [x] ",
    "> ",
    "> [!note]- T\n",
    "> > ",
    "**",
    "*",
    "_",
    "~~",
    "==",
    "%%",
    "`",
    "```",
    "~~~",
    "$",
    "$$",
    "\\",
    "[",
    "]",
    "(",
    ")",
    "[[",
    "]]",
    "![[",
    "|",
    "#",
    "#tag",
    "[^1]",
    "[^1]: ",
    "<br>",
    "<u>",
    "</u>",
    "<div>",
    "</div>",
    "<!--",
    "-->",
    "---",
    "***",
    "| a | b |\n|---|---|\n",
    "http://x.y",
    "word",
    " ",
    "é",
    "日本",
    "\t",
    "&amp;",
    "[x]: u",
];

fn random_doc(rng: &mut Lcg, pieces: usize) -> String {
    (0..pieces)
        .map(|_| FRAGMENTS[rng.next(FRAGMENTS.len())])
        .collect()
}

#[test]
fn random_documents_parse_into_well_formed_trees() {
    let mut rng = Lcg(7);
    for _ in 0..600 {
        let pieces = 1 + rng.next(60);
        let text = random_doc(&mut rng, pieces);
        let tree = parse(&text);
        assert_well_formed(&text, &tree);
        for offset in (0..=text.len()).filter(|&o| text.is_char_boundary(o)) {
            let _ = tree.context_at(offset);
        }
    }
}

fn random_boundary(rng: &mut Lcg, text: &str) -> usize {
    let mut at = rng.next(text.len() + 1);
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// Applies a random insertion or deletion and returns the new text and edit.
fn random_edit(rng: &mut Lcg, text: &str) -> (String, Edit) {
    let start = random_boundary(rng, text);
    let end = if rng.next(3) == 0 {
        random_boundary(rng, &text[start..]) + start
    } else {
        start
    };
    let end = end.min(start + 12).max(start);
    let end = (end..=text.len())
        .find(|&e| text.is_char_boundary(e))
        .unwrap_or(text.len());
    let inserted = if rng.next(4) == 0 {
        ""
    } else {
        FRAGMENTS[rng.next(FRAGMENTS.len())]
    };
    let new_text = format!("{}{inserted}{}", &text[..start], &text[end..]);
    (
        new_text,
        Edit {
            old: start..end,
            new_len: inserted.len(),
        },
    )
}

fn check_incremental(text: &str, rng: &mut Lcg, edits: usize) {
    let mut tree = parse(text);
    let mut current = text.to_owned();
    for _ in 0..edits {
        let (new_text, edit) = random_edit(rng, &current);
        tree.edit(&new_text, &edit);
        let expected = parse(&new_text);
        assert!(
            tree == expected,
            "incremental parse differs after {edit:?}\n--- text ---\n{new_text}\n--- got ---\n{}\n--- expected ---\n{}",
            dump_tree(&tree, &new_text),
            dump_tree(&expected, &new_text)
        );
        current = new_text;
    }
}

fn dump_tree(tree: &SyntaxTree, text: &str) -> String {
    let mut out = String::new();
    for &block in tree.blocks() {
        dump_node(tree, text, block, 0, &mut out);
    }
    out
}

const BLOCKS: &[&str] = &[
    "# Heading *em*",
    "A paragraph with $x^2$ and **bold**.\nSecond line [^1].",
    "- one\n- two\n  - nested",
    "> [!tip] Title\n> body $y$",
    "```rust\nfn x() {}\n```",
    "| a | b |\n|---|---|\n| 1 | 2 |",
    "$$\n\\int x\\,dx\n$$",
    "1. first\n2. second",
    "> quote\n> more",
    "Text with [link][ref] and [[wiki]].",
    "***",
    "<div>\nhtml\n</div>",
    "Setext\n---",
    "    indented code",
    "<<<<<<< this device\nMine **b**\n=======\nTheirs\n>>>>>>> other device",
];

fn random_block_doc(rng: &mut Lcg, blocks: usize) -> String {
    let mut text: String = (0..blocks)
        .map(|_| BLOCKS[rng.next(BLOCKS.len())])
        .collect::<Vec<_>>()
        .join("\n\n");
    text.push_str("\n\n[^1]: A footnote.\n\n[ref]: http://r\n");
    text
}

#[test]
fn incremental_reparse_matches_a_full_parse() {
    let mut rng = Lcg(42);
    for _ in 0..150 {
        let blocks = 3 + rng.next(12);
        let text = random_block_doc(&mut rng, blocks);
        check_incremental(&text, &mut rng, 12);
    }
}

#[test]
fn incremental_reparse_matches_on_random_fragments() {
    let mut rng = Lcg(99);
    for _ in 0..200 {
        let pieces = 5 + rng.next(80);
        let text = random_doc(&mut rng, pieces);
        check_incremental(&text, &mut rng, 8);
    }
}

fn corpus_notes() -> Vec<std::path::PathBuf> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/corpus");
    let mut notes = Vec::new();
    let mut folders = vec![root];
    while let Some(folder) = folders.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for path in entries.flatten().map(|entry| entry.path()) {
            if path.is_dir() {
                folders.push(path);
            } else if path.extension().is_some_and(|ext| ext == "md") {
                notes.push(path);
            }
        }
    }
    notes.sort();
    notes
}

/// Parses every note of the synthetic corpus, when it is present, and
/// checks the tree, contexts, a render plan and incremental edits.
#[test]
fn sync_conflicts_hold_each_version_as_blocks() {
    let text = "See [^1].\n<<<<<<< this device\n- mine\n=======\n```\nunclosed\n>>>>>>> other device\n\n[^1]: A note.";
    let tree = parse(text);
    assert_well_formed(text, &tree);
    let conflict = tree
        .nodes()
        .iter()
        .find(|node| node.kind == NodeKind::Conflict)
        .expect("the conflict is a node");
    let markers: Vec<&str> = conflict
        .markup
        .iter()
        .map(|m| &text[m.range.clone()])
        .collect();
    assert_eq!(
        markers,
        ["<<<<<<< this device", "=======", ">>>>>>> other device"]
    );
    let kinds: Vec<&NodeKind> = conflict
        .children
        .iter()
        .map(|&child| &tree.node(child).kind)
        .collect();
    assert!(matches!(
        kinds[..],
        [NodeKind::List { .. }, NodeKind::CodeBlock(_)]
    ));
    assert!(
        tree.nodes()
            .iter()
            .any(|node| matches!(node.kind, NodeKind::FootnoteReference { .. })),
        "a reference before the conflict finds its definition after it"
    );
}

#[test]
fn corpus_notes_parse_into_well_formed_trees() {
    let notes = corpus_notes();
    let mut rng = Lcg(3);
    for path in &notes {
        let text = std::fs::read_to_string(path).expect("corpus notes are UTF-8");
        let tree = parse(&text);
        assert_well_formed(&text, &tree);
        for offset in (0..=text.len())
            .step_by(7)
            .filter(|&o| text.is_char_boundary(o))
        {
            let _ = tree.context_at(offset);
        }
        let middle = random_boundary(&mut rng, &text);
        let cursor = middle..middle;
        let selections = std::slice::from_ref(&cursor);
        let settings = crate::render::RevealSettings::default();
        let plan = crate::render::plan(&crate::render::RenderInput {
            text: &text,
            tree: &tree,
            selections,
            settings: &settings,
        });
        assert_eq!(
            plan.lines.len(),
            tree.lines().line_count(),
            "{}",
            path.display()
        );
        check_incremental(&text, &mut rng, 3);
    }
}

fn math_heavy_document(target_len: usize) -> String {
    let note = "## Section\n\nLet $f(x) = \\int_0^x g(t)\\,dt$ and $\\alpha + \\beta = \\gamma$ where $n \\in \\mathbb{N}$.\n\
Then **bold** and *em* with $a_i^2$ and a footnote[^1].\n\n$$\n\\sum_{k=1}^{n} \\frac{1}{k^2} \\le \\frac{\\pi^2}{6}\n$$\n\n\
> [!note] Lemma\n> For all $\\epsilon > 0$ there is $\\delta$ with $|x - y| < \\delta$.\n\n\
- [ ] check $x$\n- [x] prove $y = mx + b$\n\n| $a$ | $b$ |\n|---|---|\n| $1$ | $2$ |\n\n";
    let mut text = String::from("---\nupdated: 2024-01-01\n---\n");
    while text.len() < target_len {
        text.push_str(note);
    }
    text.push_str("[^1]: The footnote.\n");
    text
}

/// Run with `cargo test --release -p editor-core -- --ignored --nocapture parse_speed`.
#[test]
#[ignore = "benchmark"]
fn parse_speed() {
    let text = math_heavy_document(200_000);
    let runs = 20;
    let started = std::time::Instant::now();
    for _ in 0..runs {
        std::hint::black_box(parse(&text));
    }
    let full = started.elapsed() / runs;
    let tree = parse(&text);
    let middle = text.len() / 2;
    let middle = (middle..).find(|&o| text.is_char_boundary(o)).unwrap();
    let edited = format!("{}x{}", &text[..middle], &text[middle..]);
    let edit = Edit {
        old: middle..middle,
        new_len: 1,
    };
    let started = std::time::Instant::now();
    for _ in 0..runs {
        let mut copy = tree.clone();
        copy.edit(&edited, &edit);
        std::hint::black_box(copy);
    }
    let incremental = started.elapsed() / runs;
    let started = std::time::Instant::now();
    for _ in 0..runs {
        std::hint::black_box(tree.clone());
    }
    let clone = started.elapsed() / runs;
    let started = std::time::Instant::now();
    for _ in 0..runs {
        let events = pulldown_cmark::Parser::new_ext(&text, build::options()).into_offset_iter();
        std::hint::black_box(events.count());
    }
    let pulldown = started.elapsed() / runs;
    println!(
        "{} bytes, {} nodes: full parse {full:?} (pulldown-cmark alone {pulldown:?}), \
         incremental edit {incremental:?} (includes a {clone:?} tree clone)",
        text.len(),
        tree.nodes().len()
    );
    render_speed(&text, &tree);
}

fn render_speed(text: &str, tree: &SyntaxTree) {
    use crate::render::{RenderInput, RevealSettings, plan, plan_lines};
    let settings = RevealSettings::default();
    let middle = text.len() / 2;
    let cursor = middle..middle;
    let selections = std::slice::from_ref(&cursor);
    let input = RenderInput {
        text,
        tree,
        selections,
        settings: &settings,
    };
    let runs = 20;
    let started = std::time::Instant::now();
    for _ in 0..runs {
        std::hint::black_box(plan(&input));
    }
    let whole = started.elapsed() / runs;
    let first_line = tree.lines().line_of(middle);
    let started = std::time::Instant::now();
    for _ in 0..runs {
        std::hint::black_box(plan_lines(&input, first_line..first_line + 60));
    }
    let viewport = started.elapsed() / runs;
    println!("render plan: whole document {whole:?}, 60-line viewport {viewport:?}");
}

#[test]
fn styled_html_elements() {
    check(
        "<span style=\"color:red;\">abc</span> <b>b</b> <i>i</i> <a href=\"https://example.com\">site</a>",
        &[
            "Paragraph \"<span style=\\\"color:red;\\\">abc</span> <b>b</b> <i>i</i> <a href=\\\"https://example.com\\\">site</a>\"",
            "  Html(Span({color: #ff0000})) \"<span style=\\\"color:red;\\\">abc</span>\" [HtmlTag\"<span style=\\\"color:red;\\\">\" HtmlTag\"</span>\"]",
            "    Text \"abc\"",
            "  Text \" \"",
            "  Html(Bold) \"<b>b</b>\" [HtmlTag\"<b>\" HtmlTag\"</b>\"]",
            "    Text \"b\"",
            "  Text \" \"",
            "  Html(Italic) \"<i>i</i>\" [HtmlTag\"<i>\" HtmlTag\"</i>\"]",
            "    Text \"i\"",
            "  Text \" \"",
            "  Link(Html,https://example.com) \"<a href=\\\"https://example.com\\\">site</a>\" [HtmlTag\"<a href=\\\"https://example.com\\\">\" HtmlTag\"</a>\"]",
            "    Text \"site\"",
        ],
    );
}

#[test]
fn html_block_elements_carry_their_alignment() {
    check(
        "<p style=\"text-align: center;\">Mid</p>\n\n<center>Also</center>",
        &[
            "HtmlBlock(Paragraph({text-align: center})) \"<p style=\\\"text-align: center;\\\">Mid</p>\"",
            "  Html(Paragraph({text-align: center})) \"<p style=\\\"text-align: center;\\\">Mid</p>\" [HtmlTag\"<p style=\\\"text-align: center;\\\">\" HtmlTag\"</p>\"]",
            "HtmlBlock(Center({})) \"<center>Also</center>\"",
            "  Html(Center({})) \"<center>Also</center>\" [HtmlTag\"<center>\" HtmlTag\"</center>\"]",
        ],
    );
}

#[test]
fn curly_quoted_attributes_still_style() {
    check(
        "<span style=”color:red;”>one</span> <span style=”color: blue; font-size: 2em”>two</span>",
        &[
            "Paragraph \"<span style=”color:red;”>one</span> <span style=”color: blue; font-size: 2em”>two</span>\"",
            "  Html(Span({color: #ff0000})) \"<span style=”color:red;”>one</span>\" [HtmlTag\"<span style=”color:red;”>\" HtmlTag\"</span>\"]",
            "    Text \"one\"",
            "  Text \" \"",
            "  Html(Span({color: #0000ff; font-size: 200%})) \"<span style=”color: blue; font-size: 2em”>two</span>\" [HtmlTag\"<span style=”color: blue; font-size: 2em”>\" HtmlTag\"</span>\"]",
            "    Text \"two\"",
        ],
    );
}

#[test]
fn nested_mismatched_and_unsafe_html() {
    check(
        "<span style=\"color:red\"><b>x</b></span> <b><i>y</b></i> <a href=\"javascript:alert(1)\">z</a> \\<b style=”a”>",
        &[
            "Paragraph \"<span style=\\\"color:red\\\"><b>x</b></span> <b><i>y</b></i> <a href=\\\"javascript:alert(1)\\\">z</a> \\\\<b style=”a”>\"",
            "  Html(Span({color: #ff0000})) \"<span style=\\\"color:red\\\"><b>x</b></span>\" [HtmlTag\"<span style=\\\"color:red\\\">\" HtmlTag\"</span>\"]",
            "    Html(Bold) \"<b>x</b>\" [HtmlTag\"<b>\" HtmlTag\"</b>\"]",
            "      Text \"x\"",
            "  Text \" \"",
            "  Html(Bold) \"<b><i>y</b>\" [HtmlTag\"<b>\" HtmlTag\"</b>\"]",
            "    Html(Italic) \"<i>\" [HtmlTag\"<i>\"]",
            "    Text \"y\"",
            "  Html(Italic) \"</i>\" [HtmlTag\"</i>\"]",
            "  Text \" \"",
            "  Html(Anchor) \"<a href=\\\"javascript:alert(1)\\\">z</a>\" [HtmlTag\"<a href=\\\"javascript:alert(1)\\\">\" HtmlTag\"</a>\"]",
            "    Text \"z\"",
            "  Text \" \"",
            "  Text \"<b style=”a”>\"",
        ],
    );
}
