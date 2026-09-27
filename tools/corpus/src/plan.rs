//! Decides which notes use which features, and how often, so the whole vault
//! matches the scaled feature table.

use crate::rng::Rng;
use crate::targets::{self, scaled};

/// How many of each feature one note must contain.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Budget {
    pub inline_math: usize,
    pub block_math: usize,
    pub br: usize,
    pub hr: usize,
    pub html_other: usize,
    pub embeds: usize,
    pub tasks: usize,
    pub table_rows: usize,
    pub links: usize,
    pub code_blocks: usize,
    pub callouts: usize,
    pub comments: usize,
    pub tags: usize,
    pub highlights: usize,
    pub wikilinks: usize,
}

/// Footnotes for one note, including the deliberately broken ones.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FootnotePlan {
    /// Labels that get a proper first reference (numbered 1..=labels).
    pub labels: usize,
    /// Extra references to labels already cited.
    pub repeats: usize,
    pub missing: usize,
    pub unused: usize,
    pub duplicate: usize,
    pub empty: usize,
    pub typo: usize,
}

impl FootnotePlan {
    /// Labels already taken by a broken case that consumes a label.
    fn broken_labels(&self) -> usize {
        self.missing + self.duplicate + self.empty + self.typo
    }
}

/// What kind of frontmatter a note starts with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FrontmatterKind {
    #[default]
    None,
    /// Only `updated` and `edited_seconds`.
    ChronotyperOnly,
    /// The Chronotyper keys plus others.
    WithOtherKeys,
}

/// Everything decided about one note before its text is written.
#[derive(Clone, Debug, Default)]
pub struct NotePlan {
    pub budget: Budget,
    pub footnotes: FootnotePlan,
    pub frontmatter: FrontmatterKind,
    pub math_heavy: bool,
}

type BudgetField = fn(&mut Budget) -> &mut usize;
type FootnoteField = fn(&mut FootnotePlan) -> &mut usize;

struct Allocation {
    field: BudgetField,
    uses: usize,
    notes: usize,
    min_each: usize,
    skew: u32,
}

/// Plans every note of a vault with `note_count` notes.
pub fn plan_notes(note_count: usize, rng: &mut Rng) -> Vec<NotePlan> {
    let mut plans = vec![NotePlan::default(); note_count];
    if note_count == 0 {
        return plans;
    }
    let s = |v| scaled(v, note_count);
    let math_notes = choose(rng, note_count, s(50), &[], 0);
    let block_math_notes = choose(rng, note_count, s(28), &math_notes, s(24));
    let table_notes = choose(rng, note_count, s(17), &math_notes, s(4));
    for &i in &math_notes {
        plans[i].math_heavy = true;
    }
    allocate(
        &mut plans,
        rng,
        &math_notes,
        alloc(|b| &mut b.inline_math, s(3660), 1, 3),
    );
    allocate(
        &mut plans,
        rng,
        &block_math_notes,
        alloc(|b| &mut b.block_math, s(748) / 2, 1, 2),
    );
    allocate(
        &mut plans,
        rng,
        &table_notes,
        alloc(|b| &mut b.table_rows, s(203), 3, 1),
    );
    plan_html(&mut plans, rng);
    for allocation in simple_allocations(note_count) {
        let notes = choose(rng, note_count, allocation.notes, &[], 0);
        allocate(&mut plans, rng, &notes, allocation);
    }
    plan_frontmatter(&mut plans, rng);
    plan_footnotes(&mut plans, rng);
    plans
}

fn alloc(field: BudgetField, uses: usize, min_each: usize, skew: u32) -> Allocation {
    Allocation {
        field,
        uses,
        notes: 0,
        min_each,
        skew,
    }
}

fn simple_allocations(note_count: usize) -> Vec<Allocation> {
    let s = |v| scaled(v, note_count);
    let rows: [(BudgetField, usize, usize, usize); 9] = [
        (|b| &mut b.embeds, 238, 47, 1),
        (|b| &mut b.tasks, 243, 14, 2),
        (|b| &mut b.links, 187, 52, 1),
        (|b| &mut b.code_blocks, 107 / 2, 19, 1),
        (|b| &mut b.callouts, 91, 29, 1),
        (|b| &mut b.comments, 17, 7, 1),
        (|b| &mut b.tags, 10, 5, 1),
        (|b| &mut b.highlights, 7, 7, 1),
        (|b| &mut b.wikilinks, 1, 1, 1),
    ];
    rows.into_iter()
        .map(|(field, uses, notes, min_each)| Allocation {
            field,
            uses: s(uses),
            notes: s(notes).min(note_count),
            min_each,
            skew: 2,
        })
        .collect()
}

fn plan_html(plans: &mut [NotePlan], rng: &mut Rng) {
    let n = plans.len();
    let s = |v| scaled(v, n);
    let html_notes = choose(rng, n, s(25), &[], 0);
    let br_count = (html_notes.len() * 4).div_ceil(5);
    let hr_count = (html_notes.len() * 2).div_ceil(3);
    let br_notes = html_notes[..br_count].to_vec();
    let hr_notes = html_notes[html_notes.len() - hr_count..].to_vec();
    let mut other_notes = html_notes.clone();
    rng.shuffle(&mut other_notes);
    other_notes.truncate(html_notes.len().div_ceil(2));
    allocate(
        plans,
        rng,
        &br_notes,
        alloc(|b| &mut b.br, s(targets::HTML_BR_USES), 1, 2),
    );
    allocate(
        plans,
        rng,
        &hr_notes,
        alloc(|b| &mut b.hr, s(targets::HTML_HR_USES), 1, 2),
    );
    let other = s(targets::HTML_OTHER_USES);
    allocate(
        plans,
        rng,
        &other_notes,
        alloc(|b| &mut b.html_other, other, 1, 1),
    );
}

fn plan_frontmatter(plans: &mut [NotePlan], rng: &mut Rng) {
    let n = plans.len();
    let notes = choose(rng, n, scaled(55, n), &[], 0);
    let with_other = scaled(55, n).saturating_sub(scaled(49, n)).max(1);
    for (position, &i) in notes.iter().enumerate() {
        plans[i].frontmatter = if position < with_other {
            FrontmatterKind::WithOtherKeys
        } else {
            FrontmatterKind::ChronotyperOnly
        };
    }
}

fn plan_footnotes(plans: &mut [NotePlan], rng: &mut Rng) {
    let n = plans.len();
    let s = |v| scaled(v, n);
    let notes = choose(rng, n, s(19), &[], 0);
    let label_counts = distribute(rng, s(43), notes.len(), 1, 2);
    let repeat_counts = distribute(rng, s(13), notes.len(), 0, 1);
    for (slot, &i) in notes.iter().enumerate() {
        plans[i].footnotes.labels = label_counts[slot];
        plans[i].footnotes.repeats = repeat_counts[slot];
    }
    let broken: [(FootnoteField, usize); 4] = [
        (|f| &mut f.missing, targets::BROKEN_MISSING),
        (|f| &mut f.duplicate, targets::BROKEN_DUPLICATE),
        (|f| &mut f.empty, targets::BROKEN_EMPTY),
        (|f| &mut f.typo, targets::BROKEN_TYPO),
    ];
    for (field, count) in broken {
        for _ in 0..s(count) {
            let open: Vec<usize> = notes
                .iter()
                .copied()
                .filter(|&i| has_room_for_broken(&plans[i].footnotes))
                .collect();
            if let Some(&i) = open.get(rng.below(open.len().max(1))) {
                *field(&mut plans[i].footnotes) += 1;
            }
        }
    }
    for _ in 0..s(targets::BROKEN_UNUSED) {
        let i = *rng.pick(&notes);
        plans[i].footnotes.unused += 1;
    }
}

/// A note can take another broken label while at least one label stays
/// healthy for repeated references to point at.
fn has_room_for_broken(footnotes: &FootnotePlan) -> bool {
    footnotes.broken_labels() + usize::from(footnotes.repeats > 0) < footnotes.labels
}

fn allocate(plans: &mut [NotePlan], rng: &mut Rng, notes: &[usize], allocation: Allocation) {
    let slots = allocation
        .uses
        .checked_div(allocation.min_each)
        .map_or(notes.len(), |fit| notes.len().min(fit).max(1));
    let slots = slots.min(notes.len());
    let counts = distribute(
        rng,
        allocation.uses,
        slots,
        allocation.min_each,
        allocation.skew,
    );
    for (&i, count) in notes.iter().zip(counts) {
        *(allocation.field)(&mut plans[i].budget) += count;
    }
}

/// Picks `count` distinct notes out of `note_count`, taking `preferred_share`
/// of them from `preferred` first.
pub fn choose(
    rng: &mut Rng,
    note_count: usize,
    count: usize,
    preferred: &[usize],
    preferred_share: usize,
) -> Vec<usize> {
    let count = count.min(note_count);
    let mut from_preferred = preferred.to_vec();
    rng.shuffle(&mut from_preferred);
    from_preferred.truncate(preferred_share.min(count));
    let mut rest: Vec<usize> = (0..note_count)
        .filter(|i| !from_preferred.contains(i))
        .collect();
    rng.shuffle(&mut rest);
    rest.truncate(count - from_preferred.len());
    from_preferred.extend(rest);
    from_preferred
}

/// Splits `total` into `slots` counts, each at least `min_each` when possible,
/// with the rest spread by random weights. A higher `skew` makes a few slots
/// much larger than the others.
pub fn distribute(
    rng: &mut Rng,
    total: usize,
    slots: usize,
    min_each: usize,
    skew: u32,
) -> Vec<usize> {
    if slots == 0 {
        return Vec::new();
    }
    let base = min_each.min(total / slots);
    let mut counts = vec![base; slots];
    let rest = total - base * slots;
    let weights: Vec<f64> = (0..slots).map(|_| 0.1 + power(rng.unit(), skew)).collect();
    let sum: f64 = weights.iter().sum();
    let mut given = 0;
    let mut fractions = Vec::with_capacity(slots);
    for (i, weight) in weights.iter().enumerate() {
        let exact = rest as f64 * weight / sum;
        let whole = exact.floor() as usize;
        counts[i] += whole;
        given += whole;
        fractions.push((exact - whole as f64, i));
    }
    fractions.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    for &(_, i) in fractions.iter().take(rest - given) {
        counts[i] += 1;
    }
    counts
}

fn power(value: f64, exponent: u32) -> f64 {
    (0..exponent).fold(1.0, |acc, _| acc * value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distribute_keeps_total_and_minimum() {
        let mut rng = Rng::new(1);
        let counts = distribute(&mut rng, 100, 7, 3, 3);
        assert_eq!(counts.iter().sum::<usize>(), 100);
        assert!(counts.iter().all(|&c| c >= 3));
    }

    #[test]
    fn distribute_with_too_little_to_go_round() {
        let mut rng = Rng::new(1);
        let counts = distribute(&mut rng, 2, 5, 1, 1);
        assert_eq!(counts.iter().sum::<usize>(), 2);
    }

    #[test]
    fn choose_is_distinct_and_prefers() {
        let mut rng = Rng::new(9);
        let chosen = choose(&mut rng, 50, 10, &[1, 2, 3], 3);
        assert_eq!(chosen.len(), 10);
        assert!(chosen.contains(&1) && chosen.contains(&2) && chosen.contains(&3));
        let mut sorted = chosen.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 10);
    }

    #[test]
    fn full_plan_matches_table_totals() {
        let mut rng = Rng::new(42);
        let plans = plan_notes(204, &mut rng);
        let sum = |f: fn(&NotePlan) -> usize| plans.iter().map(f).sum::<usize>();
        let notes = |f: fn(&NotePlan) -> usize| plans.iter().filter(|p| f(p) > 0).count();
        assert_eq!(sum(|p| p.budget.inline_math), 3660);
        assert_eq!(notes(|p| p.budget.inline_math), 50);
        assert_eq!(sum(|p| p.budget.block_math), 374);
        assert_eq!(sum(|p| p.budget.table_rows), 203);
        assert_eq!(notes(|p| p.budget.callouts), 29);
        assert_eq!(sum(|p| p.footnotes.labels), 43);
        assert_eq!(sum(|p| p.footnotes.missing), 3);
        assert_eq!(
            notes(|p| usize::from(p.frontmatter != FrontmatterKind::None)),
            55
        );
        let html = |p: &NotePlan| p.budget.br + p.budget.hr + p.budget.html_other;
        assert_eq!(plans.iter().filter(|p| html(p) > 0).count(), 25);
    }
}
