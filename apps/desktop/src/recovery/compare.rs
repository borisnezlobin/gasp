//! How an earlier version of a note differs from the note now, as the
//! recovery dialog shows it: line by line, and inside a line that was
//! edited rather than rewritten, word by word, the way track changes
//! reads. Long unchanged stretches fold to a count.

use similar::{ChangeTag, DiffOp, TextDiff};

/// Unchanged lines shown on each side of a change.
const CONTEXT_LINES: usize = 2;

/// A line that shares less than this much with its old self reads better
/// as the old line struck out and the new one after it than as a word-by-
/// word mix.
const EDITED_LINE_SIMILARITY: f32 = 0.5;

/// What restoring does to a stretch of text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// It stays.
    Same,
    /// It's only in the snapshot, so restoring brings it back.
    Back,
    /// It's only in the note now, so restoring removes it.
    Gone,
}

/// One row of the comparison.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiffRow {
    Same(String),
    /// A line that differs, as stretches marked by what restoring does.
    Changed(Vec<(Mark, String)>),
    /// This many unchanged lines, folded away.
    Folded(usize),
}

/// How `snapshot` compares with `current`, row by row, and how many rows
/// differ.
pub fn compare(current: &str, snapshot: &str) -> (Vec<DiffRow>, usize) {
    let diff = TextDiff::from_lines(current, snapshot);
    let (old, new) = (diff.old_slices(), diff.new_slices());
    let mut rows = Vec::new();
    for op in diff.ops() {
        let gone = &old[op.old_range()];
        let back = &new[op.new_range()];
        match op {
            DiffOp::Equal { .. } => rows.extend(gone.iter().map(|line| DiffRow::Same(trim(line)))),
            _ => rows.extend(changed_rows(gone, back)),
        }
    }
    let changed = rows
        .iter()
        .filter(|row| matches!(row, DiffRow::Changed(_)))
        .count();
    (fold_unchanged(rows), changed)
}

/// Lines replaced by others: each pair edited in place reads word by word,
/// the rest as whole lines.
fn changed_rows(gone: &[&str], back: &[&str]) -> Vec<DiffRow> {
    let paired = gone.len().min(back.len());
    let mut rows = Vec::new();
    for (old, new) in gone.iter().zip(back.iter()) {
        rows.extend(edited_line(old, new));
    }
    rows.extend(gone[paired..].iter().map(|line| whole(Mark::Gone, line)));
    rows.extend(back[paired..].iter().map(|line| whole(Mark::Back, line)));
    rows
}

fn edited_line(old: &str, new: &str) -> Vec<DiffRow> {
    let (old, new) = (trim(old), trim(new));
    let words = TextDiff::from_words(old.as_str(), new.as_str());
    if words.ratio() < EDITED_LINE_SIMILARITY {
        return vec![whole(Mark::Gone, &old), whole(Mark::Back, &new)];
    }
    let mut stretches: Vec<(Mark, String)> = Vec::new();
    for change in words.iter_all_changes() {
        let mark = match change.tag() {
            ChangeTag::Equal => Mark::Same,
            ChangeTag::Insert => Mark::Back,
            ChangeTag::Delete => Mark::Gone,
        };
        match stretches.last_mut() {
            Some((last, text)) if *last == mark => text.push_str(change.value()),
            _ => stretches.push((mark, change.value().to_owned())),
        }
    }
    vec![DiffRow::Changed(stretches)]
}

fn whole(mark: Mark, line: &str) -> DiffRow {
    DiffRow::Changed(vec![(mark, trim(line))])
}

fn trim(line: &str) -> String {
    line.trim_end_matches(['\n', '\r']).to_owned()
}

/// Folds runs of unchanged lines, keeping a few beside each change.
fn fold_unchanged(rows: Vec<DiffRow>) -> Vec<DiffRow> {
    let mut folded = Vec::with_capacity(rows.len());
    let mut run: Vec<DiffRow> = Vec::new();
    let mut seen_change = false;
    for row in rows {
        if matches!(row, DiffRow::Same(_)) {
            run.push(row);
            continue;
        }
        flush_run(&mut folded, std::mem::take(&mut run), seen_change, true);
        seen_change = true;
        folded.push(row);
    }
    flush_run(&mut folded, run, seen_change, false);
    folded
}

/// Puts a run of unchanged lines back, keeping context after the change
/// before it (`after_change`) and before the change after it
/// (`before_change`).
fn flush_run(out: &mut Vec<DiffRow>, run: Vec<DiffRow>, after_change: bool, before_change: bool) {
    let head = if after_change { CONTEXT_LINES } else { 0 };
    let tail = if before_change { CONTEXT_LINES } else { 0 };
    if run.len() <= head + tail + 1 {
        out.extend(run);
        return;
    }
    let hidden = run.len() - head - tail;
    let mut rows = run.into_iter();
    out.extend(rows.by_ref().take(head));
    out.push(DiffRow::Folded(hidden));
    out.extend(rows.skip(hidden));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_edited_line_reads_word_by_word() {
        let current = "Title\n\nIt was a bright cold day.\nShared.\n";
        let snapshot = "Title\n\nIt was a dark cold day.\nShared.\n";
        let (rows, changed) = compare(current, snapshot);
        assert_eq!(changed, 1);
        assert!(rows.contains(&DiffRow::Changed(vec![
            (Mark::Same, "It was a ".into()),
            (Mark::Gone, "bright".into()),
            (Mark::Back, "dark".into()),
            (Mark::Same, " cold day.".into()),
        ])));
        assert!(rows.contains(&DiffRow::Same("Shared.".into())));
    }

    #[test]
    fn a_rewritten_line_shows_both_whole() {
        let (rows, changed) = compare("Completely new words here.\n", "Old text entirely.\n");
        assert_eq!(changed, 2);
        assert_eq!(
            rows,
            [
                DiffRow::Changed(vec![(Mark::Gone, "Completely new words here.".into())]),
                DiffRow::Changed(vec![(Mark::Back, "Old text entirely.".into())]),
            ]
        );
    }

    #[test]
    fn added_and_removed_lines_stand_alone() {
        let (rows, changed) = compare("a\nb\n", "a\nb\nc\n");
        assert_eq!(changed, 1);
        assert_eq!(
            rows.last(),
            Some(&DiffRow::Changed(vec![(Mark::Back, "c".into())]))
        );
    }

    #[test]
    fn long_unchanged_stretches_fold() {
        let lines: Vec<String> = (0..20).map(|n| format!("line {n}")).collect();
        let current = lines.join("\n") + "\n";
        let snapshot = current.replace("line 10\n", "line ten\n");
        let (rows, _) = compare(&current, &snapshot);
        assert_eq!(rows.first(), Some(&DiffRow::Folded(8)));
        assert_eq!(rows[1], DiffRow::Same("line 8".into()));
        assert_eq!(rows.last(), Some(&DiffRow::Folded(7)));
        assert_eq!(rows.len(), 1 + 2 + 1 + 2 + 1);
    }
}
