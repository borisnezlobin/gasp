//! Writes one note from its plan: frontmatter, sections of blocks, footnote definitions.

use std::collections::{BTreeMap, BTreeSet};

use crate::blocks::{self, CALLOUT_TYPES, CalloutSpec, TableSpec};
use crate::inline::{self, FootnoteState, InlineEnv, Token};
use crate::plan::{Budget, FootnotePlan, FrontmatterKind, NotePlan};
use crate::png;
use crate::prose;
use crate::rng::Rng;
use crate::{Attachment, BrokenFootnote, FootnoteProblem};

const MATH_FOLDERS: &[&str] = &[
    "Physics",
    "Physics/Quantum Mechanics",
    "Maths/Linear Algebra",
    "Maths/Real Analysis",
    "Course Notes/Probability and Statistics",
    "Course Notes/Classical Mechanics",
];
const OTHER_FOLDERS: &[&str] = &[
    "Essays",
    "Daily Notes",
    "Reading",
    "Projects/Editor Ideas",
    "Inbox",
    "",
    "Personal",
];
const TOPICS: &[&str] = &[
    "Eigenvalues",
    "Fourier Series",
    "Harmonic Oscillator",
    "Green's Functions",
    "Taylor Expansion",
    "Bayes' Theorem",
    "Uniform Convergence",
    "Lagrangian Mechanics",
    "Spin Systems",
    "Markov Chains",
    "Normal Modes",
    "Compactness",
    "Linear Maps",
    "Entropy",
    "Rigid Bodies",
    "Wave Packets",
    "Central Limit Theorem",
    "Metric Spaces",
    "Perturbation Theory",
    "Inner Products",
];
const FOOTNOTE_NAMES: &[&str] = &[
    "aside", "source", "proof", "caveat", "history", "errata", "why", "longer",
];

/// Where a note lives and what it is called.
#[derive(Clone, Debug)]
pub struct NoteMeta {
    pub folder: String,
    pub title: String,
}

impl NoteMeta {
    /// Vault-relative path of the note.
    pub fn path(&self) -> String {
        join(&self.folder, &format!("{}.md", self.title))
    }

    /// Vault-relative path of the note's attachment folder.
    pub fn images_dir(&self) -> String {
        join(&self.folder, "images")
    }
}

fn join(folder: &str, name: &str) -> String {
    if folder.is_empty() {
        name.to_string()
    } else {
        format!("{folder}/{name}")
    }
}

/// Picks a folder and a unique title for every note.
pub fn note_metas(rng: &mut Rng, plans: &[NotePlan]) -> Vec<NoteMeta> {
    let mut used = BTreeSet::new();
    plans
        .iter()
        .map(|plan| {
            let folder = if plan.math_heavy {
                *rng.pick(MATH_FOLDERS)
            } else {
                *rng.pick(OTHER_FOLDERS)
            };
            let base = title_for(rng, folder, plan.math_heavy);
            let mut title = base.clone();
            let mut copy = 2;
            while !used.insert(title.to_lowercase()) {
                title = format!("{base} {copy}");
                copy += 1;
            }
            NoteMeta {
                folder: folder.to_string(),
                title,
            }
        })
        .collect()
}

fn title_for(rng: &mut Rng, folder: &str, math: bool) -> String {
    if folder == "Daily Notes" {
        return format!("2024-{:02}-{:02}", rng.range(1, 12), rng.range(1, 28));
    }
    if math {
        let topic = rng.pick(TOPICS);
        return match rng.below(4) {
            0 => format!("Lecture {} - {topic}", rng.range(1, 30)),
            1 => format!("{topic} Problem Set {}", rng.range(1, 9)),
            2 => format!("Notes on {topic}"),
            _ => topic.to_string(),
        };
    }
    match rng.below(4) {
        0 => format!("On {}", prose::title_case(&prose::phrase(rng))),
        1 => format!("{} Ideas", prose::title_case(prose::noun(rng))),
        2 => format!("Reading - {}", prose::title_case(&prose::phrase(rng))),
        _ => prose::title_case(&prose::phrase(rng)),
    }
}

/// State shared by every note while the vault is written.
pub struct VaultContext {
    pub metas: Vec<NoteMeta>,
    pub titles: Vec<String>,
    pub attachments: Vec<Attachment>,
    pub broken: Vec<BrokenFootnote>,
    image_counter: usize,
    callout_counter: usize,
}

impl VaultContext {
    pub fn new(metas: Vec<NoteMeta>) -> Self {
        let titles = metas.iter().map(|m| m.title.clone()).collect();
        Self {
            metas,
            titles,
            attachments: Vec::new(),
            broken: Vec::new(),
            image_counter: 0,
            callout_counter: 0,
        }
    }

    fn next_callout_kind(&mut self, rng: &mut Rng) -> String {
        let counter = self.callout_counter;
        self.callout_counter += 1;
        let kind = if counter < CALLOUT_TYPES.len() {
            CALLOUT_TYPES[counter]
        } else {
            *rng.pick(&[
                "note",
                "info",
                "tip",
                "warning",
                "example",
                "important",
                "question",
                "abstract",
                "quote",
            ])
        };
        if rng.chance(0.1) {
            kind.to_uppercase()
        } else {
            kind.to_string()
        }
    }

    fn new_image(&mut self, rng: &mut Rng, folder_images: &str, allow_spaces: bool) -> String {
        self.image_counter += 1;
        let counter = self.image_counter;
        let name = if allow_spaces && rng.chance(0.4) {
            format!(
                "Pasted image 2024{:02}{:02}{:06}.png",
                rng.range(1, 12),
                rng.range(1, 28),
                100_000 + counter
            )
        } else {
            format!("{}-{counter}.png", prose::noun(rng).replace(' ', "-"))
        };
        let width = rng.range(1, 4) as u32;
        let height = rng.range(1, 4) as u32;
        let pixels: Vec<[u8; 3]> = (0..width * height)
            .map(|_| {
                [
                    rng.below(256) as u8,
                    rng.below(256) as u8,
                    rng.below(256) as u8,
                ]
            })
            .collect();
        self.attachments.push(Attachment {
            path: format!("{folder_images}/{name}"),
            bytes: png::encode_rgb(width, height, &pixels),
        });
        name
    }
}

enum Block {
    Paragraph(Vec<Token>),
    Callout(CalloutSpec),
    DisplayMath,
    Table(TableSpec),
    Tasks(usize),
    Code,
    Embeds(usize),
    Hr,
    Div,
    Img,
    Comment,
    List,
    Quote,
}

/// Writes note `index` of the vault.
pub fn write_note(seed: u64, index: usize, plan: &NotePlan, ctx: &mut VaultContext) -> String {
    let mut rng = Rng::stream(seed, index as u64 + 1);
    let footnotes = FootnoteSetup::new(&mut rng, &plan.footnotes);
    footnotes.record_broken(&ctx.metas[index].path(), &mut ctx.broken);
    let mut writer = NoteWriter {
        rng,
        index,
        budget: plan.budget.clone(),
        footnote_plan: plan.footnotes.clone(),
        state: footnotes.state.clone(),
        images: Vec::new(),
    };
    let mut parts = Vec::new();
    if plan.frontmatter != FrontmatterKind::None {
        parts.push(
            blocks::frontmatter(&mut writer.rng, plan.frontmatter)
                .trim_end()
                .to_string(),
        );
    }
    if writer.rng.chance(0.35) {
        parts.push(format!("# {}", ctx.metas[index].title));
    }
    let blocks = writer.plan_blocks(ctx);
    parts.extend(writer.render_sections(ctx, blocks));
    let definitions = footnotes.definitions(&mut writer.rng);
    if !definitions.is_empty() {
        parts.push(definitions);
    }
    parts.join("\n\n") + "\n"
}

struct NoteWriter {
    rng: Rng,
    index: usize,
    budget: Budget,
    footnote_plan: FootnotePlan,
    state: FootnoteState,
    images: Vec<String>,
}

impl NoteWriter {
    fn plan_blocks(&mut self, ctx: &mut VaultContext) -> Vec<Block> {
        let mut callouts = self.callouts(ctx);
        let mut blocks = self.structural_blocks();
        let mut tokens = self.tokens(&mut blocks);
        self.rng.shuffle(&mut tokens);
        let containers = self
            .rng
            .range(2, 5)
            .max(tokens.len().div_ceil(self.rng.range(3, 6)));
        let paragraphs = containers.saturating_sub(callouts.len()).max(1);
        let mut dealt: Vec<Vec<Token>> = vec![Vec::new(); paragraphs + callouts.len()];
        for token in tokens {
            let at = self.rng.below(dealt.len());
            dealt[at].push(token);
        }
        for (spec, tokens) in callouts.iter_mut().zip(dealt.drain(paragraphs..)) {
            spec.tokens = tokens;
        }
        blocks.extend(dealt.into_iter().map(Block::Paragraph));
        blocks.extend(callouts.into_iter().map(Block::Callout));
        self.add_fillers(&mut blocks);
        self.rng.shuffle(&mut blocks);
        if let Some(first) = blocks.iter().position(|b| matches!(b, Block::Paragraph(_))) {
            blocks.swap(0, first);
        }
        blocks
    }

    fn callouts(&mut self, ctx: &mut VaultContext) -> Vec<CalloutSpec> {
        let mut specs = Vec::new();
        while self.budget.callouts > 0 {
            self.budget.callouts -= 1;
            let nested = self.budget.callouts > 0 && self.rng.chance(0.15);
            let nested_kind = nested.then(|| ctx.next_callout_kind(&mut self.rng));
            if nested {
                self.budget.callouts -= 1;
            }
            specs.push(self.callout_spec(ctx, nested_kind));
        }
        specs
    }

    fn callout_spec(&mut self, ctx: &mut VaultContext, nested_kind: Option<String>) -> CalloutSpec {
        let rng = &mut self.rng;
        let kind = ctx.next_callout_kind(rng);
        let fold = *rng.pick(&["", "", "", "", "-", "+"]);
        let title = rng
            .chance(0.7)
            .then(|| prose::title_case(&prose::phrase(rng)));
        let block_math = self.budget.block_math > 0 && rng.chance(0.3);
        self.budget.block_math -= usize::from(block_math);
        let tasks = if self.budget.tasks >= 2 && rng.chance(0.15) {
            rng.range(2, self.budget.tasks.min(4))
        } else {
            0
        };
        self.budget.tasks -= tasks;
        CalloutSpec {
            kind,
            fold,
            title,
            tokens: Vec::new(),
            block_math,
            tasks,
            nested_kind,
        }
    }

    fn structural_blocks(&mut self) -> Vec<Block> {
        let mut blocks = Vec::new();
        for rows in split(&mut self.rng, self.budget.table_rows, 3, 9) {
            blocks.push(Block::Table(TableSpec::take(
                &mut self.rng,
                rows,
                &mut self.budget,
            )));
        }
        for items in split(&mut self.rng, self.budget.tasks, 2, 12) {
            blocks.push(Block::Tasks(items));
        }
        for count in split(&mut self.rng, self.budget.embeds, 1, 3) {
            blocks.push(Block::Embeds(count));
        }
        blocks.extend((0..self.budget.code_blocks).map(|_| Block::Code));
        blocks.extend((0..self.budget.block_math).map(|_| Block::DisplayMath));
        blocks.extend((0..self.budget.hr).map(|_| Block::Hr));
        self.budget.tasks = 0;
        self.budget.embeds = 0;
        blocks
    }

    /// Inline tokens for the budget; HTML and comment units that become blocks go to `blocks`.
    fn tokens(&mut self, blocks: &mut Vec<Block>) -> Vec<Token> {
        let mut tokens = Vec::new();
        for _ in 0..self.budget.html_other {
            match self.rng.weighted(&[30, 25, 20, 10, 10, 5]) {
                0 => tokens.push(Token::Underline),
                1 => blocks.push(Block::Div),
                2 => blocks.push(Block::Img),
                3 => tokens.push(Token::Sup),
                4 => tokens.push(Token::Span),
                _ => tokens.push(Token::Kbd),
            }
        }
        for _ in 0..self.budget.comments {
            if self.rng.chance(0.4) {
                blocks.push(Block::Comment);
            } else {
                tokens.push(Token::Comment);
            }
        }
        let fixed = [
            (Token::Math, self.budget.inline_math),
            (Token::Br, self.budget.br),
            (Token::Link, self.budget.links),
            (Token::Highlight, self.budget.highlights),
            (Token::Tag, self.budget.tags),
            (Token::Wikilink, self.budget.wikilinks),
            (
                Token::Footnote,
                self.footnote_plan.labels + self.footnote_plan.repeats,
            ),
        ];
        for (token, count) in fixed {
            tokens.extend(std::iter::repeat_n(token, count));
        }
        tokens
    }

    fn add_fillers(&mut self, blocks: &mut Vec<Block>) {
        for _ in 0..self.rng.range(0, 2) {
            blocks.push(Block::List);
        }
        if self.rng.chance(0.3) {
            blocks.push(Block::Quote);
        }
    }

    fn render_sections(&mut self, ctx: &mut VaultContext, blocks: Vec<Block>) -> Vec<String> {
        let section_count = (blocks.len() / self.rng.range(3, 5)).max(1);
        let deep = self.index % 23 == 7;
        let mut parts = Vec::new();
        let mut section = 0;
        let per_section = blocks.len().div_ceil(section_count).max(1);
        for (i, block) in blocks.into_iter().enumerate() {
            let starts_section = i % per_section == 0;
            if starts_section && (i > 0 || self.rng.chance(0.5)) {
                let level = if deep {
                    2 + section % 5
                } else {
                    2 + self.rng.weighted(&[60, 30, 8, 2, 1])
                };
                parts.push(blocks::heading(&mut self.rng, level));
                section += 1;
            }
            parts.push(self.render_block(ctx, block));
        }
        parts
    }

    fn render_block(&mut self, ctx: &mut VaultContext, block: Block) -> String {
        match block {
            Block::Paragraph(tokens) => {
                self.with_env(ctx, |rng, env| inline::paragraph(rng, env, &tokens))
            }
            Block::Callout(spec) => self.with_env(ctx, |rng, env| blocks::callout(rng, env, &spec)),
            Block::DisplayMath => blocks::block_math(&mut self.rng),
            Block::Table(spec) => blocks::table(&mut self.rng, spec),
            Block::Tasks(items) => blocks::task_list(&mut self.rng, items),
            Block::Code => blocks::code_block(&mut self.rng),
            Block::Embeds(count) => self.embeds(ctx, count),
            Block::Hr => blocks::hr(&mut self.rng),
            Block::Div => blocks::div(&mut self.rng),
            Block::Img => self.img(ctx),
            Block::Comment => blocks::comment_block(&mut self.rng),
            Block::List => blocks::nested_list(&mut self.rng),
            Block::Quote => blocks::quote(&mut self.rng),
        }
    }

    fn with_env<F>(&mut self, ctx: &VaultContext, render: F) -> String
    where
        F: FnOnce(&mut Rng, &mut InlineEnv) -> String,
    {
        let mut env = InlineEnv {
            titles: &ctx.titles,
            self_index: self.index,
            footnotes: &mut self.state,
        };
        render(&mut self.rng, &mut env)
    }

    fn embeds(&mut self, ctx: &mut VaultContext, count: usize) -> String {
        (0..count)
            .map(|_| self.embed(ctx))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn embed(&mut self, ctx: &mut VaultContext) -> String {
        let rng = &mut self.rng;
        if ctx.titles.len() > 1 && rng.chance(0.03) {
            let offset = rng.range(1, ctx.titles.len() - 1);
            return format!(
                "![[{}]]",
                ctx.titles[(self.index + offset) % ctx.titles.len()]
            );
        }
        let name = if !self.images.is_empty() && rng.chance(0.05) {
            rng.pick(&self.images).clone()
        } else {
            let images_dir = ctx.metas[self.index].images_dir();
            ctx.new_image(rng, &images_dir, true)
        };
        self.images.push(name.clone());
        match rng.weighted(&[65, 28, 7]) {
            0 => format!("![[{name}]]"),
            1 => format!("![[{name}|{}]]", rng.pick(&[200, 300, 400, 500, 640])),
            _ => format!(
                "![[{name}|{}x{}]]",
                rng.range(2, 6) * 100,
                rng.range(1, 4) * 100
            ),
        }
    }

    fn img(&mut self, ctx: &mut VaultContext) -> String {
        let images_dir = ctx.metas[self.index].images_dir();
        let name = ctx.new_image(&mut self.rng, &images_dir, false);
        format!(
            "<img src=\"images/{name}\" width=\"{}\">",
            self.rng.range(2, 6) * 60
        )
    }
}

/// Splits `total` into chunks of `min..=max`, never leaving a chunk under `min`
/// unless `total` itself is.
fn split(rng: &mut Rng, total: usize, min: usize, max: usize) -> Vec<usize> {
    let mut chunks = Vec::new();
    let mut left = total;
    while left > 0 {
        let size = if left <= max {
            left
        } else {
            rng.range(min, max.min(left - min))
        };
        chunks.push(size);
        left -= size;
    }
    chunks
}

/// Which footnote labels are deliberately broken, and their names.
struct FootnoteSetup {
    labels: usize,
    missing: BTreeSet<usize>,
    duplicate: BTreeSet<usize>,
    empty: BTreeSet<usize>,
    typo: BTreeSet<usize>,
    unused: usize,
    state: FootnoteState,
}

impl FootnoteSetup {
    fn new(rng: &mut Rng, plan: &FootnotePlan) -> Self {
        let mut order: Vec<usize> = (1..=plan.labels).collect();
        rng.shuffle(&mut order);
        let mut take =
            |count: usize| -> BTreeSet<usize> { order.drain(..count.min(order.len())).collect() };
        let missing = take(plan.missing);
        let duplicate = take(plan.duplicate);
        let empty = take(plan.empty);
        let typo = take(plan.typo);
        let mut names = BTreeMap::new();
        let mut name_pool = FOOTNOTE_NAMES.to_vec();
        rng.shuffle(&mut name_pool);
        for number in 1..=plan.labels {
            if !typo.contains(&number)
                && rng.chance(0.12)
                && let Some(name) = name_pool.pop()
            {
                names.insert(number, name.to_string());
            }
        }
        let state =
            FootnoteState::new(plan.labels, plan.labels + plan.repeats, typo.clone(), names);
        Self {
            labels: plan.labels,
            missing,
            duplicate,
            empty,
            typo,
            unused: plan.unused,
            state,
        }
    }

    fn record_broken(&self, note: &str, broken: &mut Vec<BrokenFootnote>) {
        let groups = [
            (&self.missing, FootnoteProblem::Missing),
            (&self.duplicate, FootnoteProblem::Duplicate),
            (&self.empty, FootnoteProblem::Empty),
            (&self.typo, FootnoteProblem::Typo),
        ];
        for (labels, problem) in groups {
            for &number in labels {
                broken.push(BrokenFootnote {
                    note: note.to_string(),
                    label: self.state.label(number),
                    problem,
                });
            }
        }
        for number in self.labels + 1..=self.labels + self.unused {
            broken.push(BrokenFootnote {
                note: note.to_string(),
                label: number.to_string(),
                problem: FootnoteProblem::Unused,
            });
        }
    }

    fn definitions(&self, rng: &mut Rng) -> String {
        let mut lines = Vec::new();
        for number in 1..=self.labels + self.unused {
            if self.missing.contains(&number) {
                continue;
            }
            let label = self.state.label(number);
            if self.empty.contains(&number) {
                lines.push(format!("[^{label}]:"));
                continue;
            }
            lines.push(format!("[^{label}]: {}", definition_text(rng)));
        }
        for &number in &self.duplicate {
            lines.push(format!(
                "[^{}]: {}",
                self.state.label(number),
                definition_text(rng)
            ));
        }
        lines.join("\n")
    }
}

fn definition_text(rng: &mut Rng) -> String {
    let first = prose::sentence(rng).render();
    match rng.below(10) {
        0 => format!("{first}\n    {}", prose::sentence(rng).render()),
        1 | 2 => format!("{first} See {}.", prose::url(rng)),
        _ => first,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_respects_bounds() {
        let mut rng = Rng::new(1);
        for total in 3..60 {
            let chunks = split(&mut rng, total, 3, 9);
            assert_eq!(chunks.iter().sum::<usize>(), total);
            assert!(chunks.iter().all(|&c| (3..=9).contains(&c)), "{chunks:?}");
        }
    }

    #[test]
    fn broken_labels_are_disjoint() {
        let mut rng = Rng::new(2);
        let plan = FootnotePlan {
            labels: 6,
            missing: 1,
            duplicate: 1,
            empty: 1,
            typo: 1,
            unused: 1,
            repeats: 0,
        };
        let setup = FootnoteSetup::new(&mut rng, &plan);
        let all: BTreeSet<usize> = [&setup.missing, &setup.duplicate, &setup.empty, &setup.typo]
            .into_iter()
            .flatten()
            .copied()
            .collect();
        assert_eq!(all.len(), 4);
        let mut broken = Vec::new();
        setup.record_broken("a.md", &mut broken);
        assert_eq!(broken.len(), 5);
    }

    #[test]
    fn titles_are_unique() {
        let mut rng = Rng::new(3);
        let plans = vec![NotePlan::default(); 300];
        let metas = note_metas(&mut rng, &plans);
        let unique: BTreeSet<String> = metas.iter().map(|m| m.path().to_lowercase()).collect();
        assert_eq!(unique.len(), 300);
    }
}
