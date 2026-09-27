//! Block-level Markdown: frontmatter, tables, task lists, code, lists, quotes,
//! callouts, display math, comments and raw HTML blocks.

use crate::inline::{self, InlineEnv, Token};
use crate::math;
use crate::plan::{Budget, FrontmatterKind};
use crate::prose;
use crate::rng::Rng;

/// Every callout type Obsidian ships with.
pub const CALLOUT_TYPES: [&str; 27] = [
    "note",
    "abstract",
    "summary",
    "tldr",
    "info",
    "todo",
    "tip",
    "hint",
    "important",
    "success",
    "check",
    "done",
    "question",
    "help",
    "faq",
    "warning",
    "caution",
    "attention",
    "failure",
    "fail",
    "missing",
    "danger",
    "error",
    "bug",
    "example",
    "quote",
    "cite",
];

/// Frontmatter with the Chronotyper keys, and other keys for some notes.
pub fn frontmatter(rng: &mut Rng, kind: FrontmatterKind) -> String {
    let updated = format!(
        "updated: 2024-{:02}-{:02}T{:02}:{:02}:{:02}+0{}:00",
        rng.range(1, 12),
        rng.range(1, 28),
        rng.range(7, 23),
        rng.below(60),
        rng.below(60),
        rng.below(3)
    );
    let edited = format!("edited_seconds: {}", rng.range(30, 40_000));
    let mut lines = vec![updated, edited];
    if kind == FrontmatterKind::WithOtherKeys {
        let extra = other_frontmatter_keys(rng);
        let at = if rng.chance(0.5) { 0 } else { lines.len() };
        lines.splice(at..at, extra);
    }
    format!("---\n{}\n---\n", lines.join("\n"))
}

fn other_frontmatter_keys(rng: &mut Rng) -> Vec<String> {
    let options = [
        format!(
            "tags:\n  - {}\n  - {}",
            prose::noun(rng).replace(' ', "-"),
            prose::noun(rng).replace(' ', "-")
        ),
        format!("aliases: [{}]", prose::title_case(&prose::phrase(rng))),
        format!(
            "created: 2023-{:02}-{:02}",
            rng.range(1, 12),
            rng.range(1, 28)
        ),
        "cssclasses: [wide-page]".to_string(),
        "publish: true".to_string(),
    ];
    let mut picked: Vec<String> = options.into_iter().filter(|_| rng.chance(0.5)).collect();
    if picked.is_empty() {
        picked.push("publish: false".to_string());
    }
    picked
}

/// A display-math block, usually `$$` on their own lines.
pub fn block_math(rng: &mut Rng) -> String {
    let body = math::display(rng);
    if !body.contains('\n') && rng.chance(0.15) {
        return format!("$${body}$$");
    }
    format!("$$\n{body}\n$$")
}

/// A heading line.
pub fn heading(rng: &mut Rng, level: usize) -> String {
    let text = match rng.below(6) {
        0 => format!("Proof of the {}", prose::phrase(rng)),
        1 => format!("Example {}", rng.range(1, 12)),
        2 => format!(
            "{} and {}",
            prose::title_case(prose::noun(rng)),
            prose::noun(rng)
        ),
        3 => format!("Lecture {} recap", rng.range(1, 24)),
        4 => "Questions".to_string(),
        _ => prose::title_case(&prose::phrase(rng)),
    };
    format!("{} {text}", "#".repeat(level))
}

const ALIGNMENTS: &[&str] = &["---", ":---", ":---:", "---:"];

/// What a table contains besides plain cells.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TableSpec {
    /// Rows including the header.
    pub rows: usize,
    /// One column holds inline math in every body row.
    pub math_column: bool,
    /// One cell holds a `<br>`.
    pub br_cell: bool,
}

impl TableSpec {
    /// Decides a table's extras, taking inline math and `<br>` from the budget.
    pub fn take(rng: &mut Rng, rows: usize, budget: &mut Budget) -> Self {
        let body_rows = rows.saturating_sub(1);
        let math_column = body_rows > 0 && budget.inline_math >= body_rows && rng.chance(0.7);
        if math_column {
            budget.inline_math -= body_rows;
        }
        let br_cell = body_rows > 0 && budget.br > 0 && rng.chance(0.4);
        budget.br -= usize::from(br_cell);
        Self {
            rows,
            math_column,
            br_cell,
        }
    }
}

/// A table with alignment row, formatted cells and optional math and `<br>`.
pub fn table(rng: &mut Rng, spec: TableSpec) -> String {
    let columns = rng.range(2, 5);
    let body_rows = spec.rows.saturating_sub(1);
    let math_column = spec.math_column.then(|| rng.below(columns));
    let br_cell = spec.br_cell.then(|| {
        let column = rng.below(columns);
        let column = if math_column == Some(column) {
            (column + 1) % columns
        } else {
            column
        };
        (rng.below(body_rows.max(1)), column)
    });
    let pipe_cell = rng
        .chance(0.3)
        .then(|| (rng.below(body_rows.max(1)), rng.below(columns)));
    let header: Vec<String> = (0..columns)
        .map(|_| prose::title_case(&prose::phrase(rng)))
        .collect();
    let aligned = rng.chance(0.6);
    let align: Vec<&str> = (0..columns)
        .map(|_| {
            if aligned {
                *rng.pick(ALIGNMENTS)
            } else {
                "---"
            }
        })
        .collect();
    let mut lines = vec![row(&header), row(&align)];
    for r in 0..body_rows {
        let cells: Vec<String> = (0..columns)
            .map(|c| {
                if math_column == Some(c) {
                    return format!("${}$", math::inline_for_table(rng));
                }
                let mut text = cell(rng);
                if br_cell == Some((r, c)) {
                    text = format!("{text}<br>{}", prose::phrase(rng));
                }
                if pipe_cell == Some((r, c)) {
                    text = format!("{text} \\| {}", prose::phrase(rng));
                }
                text
            })
            .collect();
        lines.push(row(&cells));
    }
    lines.join("\n")
}

fn row<S: AsRef<str>>(cells: &[S]) -> String {
    let inner: Vec<&str> = cells.iter().map(AsRef::as_ref).collect();
    format!("| {} |", inner.join(" | "))
}

fn cell(rng: &mut Rng) -> String {
    match rng.weighted(&[40, 15, 10, 8, 8, 8, 6, 5]) {
        0 => prose::phrase(rng),
        1 => prose::decimal(rng),
        2 => rng.range(1, 500).to_string(),
        3 => format!("**{}**", prose::noun(rng)),
        4 => format!("`{}`", prose::noun(rng).replace(' ', "_")),
        5 => format!("*{}*", prose::adjective(rng)),
        6 => prose::clock_time(rng),
        _ => format!("~~{}~~ {}", prose::noun(rng), prose::noun(rng)),
    }
}

/// A task list with `items` items, some nested and some checked.
pub fn task_list(rng: &mut Rng, items: usize) -> String {
    let bullet = *rng.pick(&["-", "-", "-", "*", "+"]);
    let indent = *rng.pick(&["\t", "    ", "  "]);
    let mut lines = Vec::with_capacity(items);
    for i in 0..items {
        let nested = i > 0 && rng.chance(0.25);
        let mark = if rng.chance(0.4) { "x" } else { " " };
        let prefix = if nested { indent } else { "" };
        lines.push(format!(
            "{prefix}{bullet} [{mark}] {}",
            prose::task_text(rng)
        ));
    }
    lines.join("\n")
}

/// A plain list, bulleted or numbered, nested two or three levels deep.
pub fn nested_list(rng: &mut Rng) -> String {
    let indent = *rng.pick(&["\t", "    ", "  "]);
    let numbered = rng.chance(0.35);
    let mut lines = Vec::new();
    let mut depth = 0usize;
    let mut number = 0usize;
    for item in 0..rng.range(3, 8) {
        if item > 0 {
            depth = next_depth(rng, depth);
        }
        let marker = if depth == 0 && numbered {
            number += 1;
            format!("{number}.")
        } else {
            rng.pick(&["-", "-", "*"]).to_string()
        };
        let plain = prose::sentence(rng);
        let text = inline::decorated(rng, plain).render();
        lines.push(format!("{}{marker} {text}", indent.repeat(depth)));
    }
    lines.join("\n")
}

fn next_depth(rng: &mut Rng, depth: usize) -> usize {
    match rng.below(3) {
        0 if depth < 2 => depth + 1,
        1 if depth > 0 => depth - 1,
        _ => depth,
    }
}

/// A block quote, sometimes with an attribution line.
pub fn quote(rng: &mut Rng) -> String {
    let mut lines: Vec<String> = (0..rng.range(1, 3))
        .map(|_| prose::sentence(rng).render())
        .collect();
    if rng.chance(0.4) {
        lines.push(String::new());
        lines.push(format!("— Dr. {}", prose::name(rng)));
    }
    prefix_lines(&lines.join("\n"), "> ")
}

/// A `%%` comment spanning several lines.
pub fn comment_block(rng: &mut Rng) -> String {
    let lines: Vec<String> = (0..rng.range(1, 3))
        .map(|_| prose::sentence(rng).render())
        .collect();
    format!("%%\n{}\n%%", lines.join("\n"))
}

/// A `<hr>` line.
pub fn hr(rng: &mut Rng) -> String {
    rng.pick(&["<hr>", "<hr>", "<hr>", "<hr/>", "<hr />"])
        .to_string()
}

/// A one-line `<div>`.
pub fn div(rng: &mut Rng) -> String {
    let text = prose::sentence(rng).render();
    match rng.below(3) {
        0 => format!("<div align=\"center\">{text}</div>"),
        1 => format!("<div style=\"text-align: right\">{text}</div>"),
        _ => "<div class=\"page-break\"></div>".to_string(),
    }
}

/// What a callout contains.
pub struct CalloutSpec {
    pub kind: String,
    pub fold: &'static str,
    pub title: Option<String>,
    pub tokens: Vec<Token>,
    pub block_math: bool,
    pub tasks: usize,
    pub nested_kind: Option<String>,
}

/// A callout with a paragraph and optional list, math and nested callout.
pub fn callout(rng: &mut Rng, env: &mut InlineEnv, spec: &CalloutSpec) -> String {
    let mut parts = vec![inline::paragraph(rng, env, &spec.tokens)];
    if spec.tasks > 0 {
        parts.push(task_list(rng, spec.tasks));
    } else if rng.chance(0.25) {
        parts.push(nested_list(rng));
    }
    if spec.block_math {
        parts.push(block_math(rng));
    }
    if let Some(kind) = &spec.nested_kind {
        let inner = format!(
            "[!{kind}] {}\n{}",
            prose::title_case(&prose::phrase(rng)),
            prose::sentence(rng).render()
        );
        parts.push(prefix_lines(&inner, "> "));
    }
    let title = spec
        .title
        .as_ref()
        .map(|t| format!(" {t}"))
        .unwrap_or_default();
    let body = format!(
        "[!{}]{}{title}\n{}",
        spec.kind,
        spec.fold,
        parts.join("\n\n")
    );
    prefix_lines(&body, "> ")
}

/// Prefixes every line; empty lines get the prefix without its trailing space.
pub fn prefix_lines(text: &str, prefix: &str) -> String {
    text.lines()
        .map(|line| {
            if line.is_empty() {
                prefix.trim_end().to_string()
            } else {
                format!("{prefix}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

struct CodeSample {
    language: &'static str,
    extension: &'static str,
    body: &'static str,
}

const CODE_SAMPLES: &[CodeSample] = &[
    CodeSample {
        language: "python",
        extension: "py",
        body: "import numpy as np\n\ndef @id(x, steps=@n):\n    # simple fixed-point iteration\n    for _ in range(steps):\n        x = np.cos(x)\n    return x",
    },
    CodeSample {
        language: "python",
        extension: "py",
        body: "data = [@n, 1@n, 2@n]\nmean = sum(data) / len(data)\nprint(f\"mean = {mean:.2f}\")",
    },
    CodeSample {
        language: "rust",
        extension: "rs",
        body: "fn @id(xs: &[f64]) -> f64 {\n    xs.iter().map(|x| x * x).sum::<f64>().sqrt()\n}",
    },
    CodeSample {
        language: "js",
        extension: "js",
        body: "const @id = (items) => items\n  .filter((item) => item.done)\n  .map((item) => item.title);",
    },
    CodeSample {
        language: "bash",
        extension: "sh",
        body: "for f in *.md; do\n  echo \"$f: $(wc -w < \"$f\") words\"\ndone",
    },
    CodeSample {
        language: "bash",
        extension: "sh",
        body: "git add -A && git commit -m \"@id notes\"\necho $HOME",
    },
    CodeSample {
        language: "c",
        extension: "c",
        body: "#include <stdio.h>\n\nint main(void) {\n    printf(\"%d\\n\", @n);\n    return 0;\n}",
    },
    CodeSample {
        language: "latex",
        extension: "tex",
        body: "\\begin{theorem}\nEvery bounded monotone sequence converges, e.g. $a_n = 1 - 1/n$.\n\\end{theorem}",
    },
    CodeSample {
        language: "json",
        extension: "json",
        body: "{\n  \"name\": \"@id\",\n  \"version\": \"1.@n.0\",\n  \"private\": true\n}",
    },
    CodeSample {
        language: "",
        extension: "txt",
        body: "step 1: read the problem\nstep 2: sketch the @id\nstep 3: check units | signs",
    },
    CodeSample {
        language: "markdown",
        extension: "md",
        body: "# Not a heading\n- [ ] not a task\n$not math$ and [[not a link]]",
    },
];

const IDENTIFIERS: &[&str] = &[
    "solve",
    "integrate",
    "step",
    "update",
    "normalize",
    "parse_notes",
    "count_words",
    "residual",
    "estimate",
];

/// A fenced code block, sometimes with `~~~` fences or a `title:"…"` attribute.
pub fn code_block(rng: &mut Rng) -> String {
    let sample = rng.pick(CODE_SAMPLES);
    let id = *rng.pick(IDENTIFIERS);
    let number = rng.range(2, 9).to_string();
    let body = sample.body.replace("@id", id).replace("@n", &number);
    let fence = if rng.chance(0.15) { "~~~" } else { "```" };
    let title = if rng.chance(0.25) && !sample.language.is_empty() {
        format!(" title:\"{id}.{}\"", sample.extension)
    } else {
        String::new()
    };
    format!("{fence}{}{title}\n{body}\n{fence}", sample.language)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_has_requested_rows_and_uses_budget() {
        let mut rng = Rng::new(4);
        for _ in 0..30 {
            let mut budget = Budget {
                inline_math: 100,
                br: 5,
                ..Budget::default()
            };
            let spec = TableSpec::take(&mut rng, 6, &mut budget);
            let text = table(&mut rng, spec);
            let rows = text.lines().filter(|l| !l.contains("---")).count();
            assert_eq!(rows, 6);
            assert_eq!(100 - budget.inline_math, text.matches('$').count() / 2);
            assert_eq!(5 - budget.br, text.matches("<br>").count());
        }
    }

    #[test]
    fn task_list_item_count() {
        let mut rng = Rng::new(1);
        let text = task_list(&mut rng, 9);
        assert_eq!(text.lines().count(), 9);
        assert!(text.lines().all(|l| l.trim_start().contains(" [")));
    }

    #[test]
    fn code_block_is_closed_by_same_fence() {
        let mut rng = Rng::new(2);
        for _ in 0..50 {
            let text = code_block(&mut rng);
            let first = &text[..3];
            assert!(text.ends_with(first));
        }
    }

    #[test]
    fn prefix_lines_handles_blank_lines() {
        assert_eq!(prefix_lines("a\n\nb", "> "), "> a\n>\n> b");
    }

    #[test]
    fn frontmatter_has_chronotyper_keys() {
        let mut rng = Rng::new(3);
        let text = frontmatter(&mut rng, FrontmatterKind::ChronotyperOnly);
        assert!(text.starts_with("---\nupdated: "));
        assert!(text.contains("\nedited_seconds: "));
        assert_eq!(text.lines().count(), 4);
    }
}
