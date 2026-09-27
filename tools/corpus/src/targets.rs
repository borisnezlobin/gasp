//! The feature table from PLAN.md ("What your vault actually uses"), which the corpus imitates.

/// How often a feature appears in the real vault.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeatureTarget {
    /// Key used in `manifest.json` and by the scanner.
    pub key: &'static str,
    /// Number of uses in the whole vault.
    pub uses: usize,
    /// Number of notes that use the feature at least once.
    pub notes: usize,
}

/// Number of notes in the measured vault.
pub const VAULT_NOTE_COUNT: usize = 204;

/// Uses of `<br>` in the measured vault.
pub const HTML_BR_USES: usize = 326;
/// Uses of `<hr>` in the measured vault.
pub const HTML_HR_USES: usize = 312;
/// Other raw HTML tags (`<div>`, `<img>`, `<u>` and friends).
pub const HTML_OTHER_USES: usize = 45;

/// Deliberately broken footnotes per kind, for the full-size vault.
pub const BROKEN_MISSING: usize = 3;
/// Definitions nothing refers to.
pub const BROKEN_UNUSED: usize = 2;
/// Labels defined twice.
pub const BROKEN_DUPLICATE: usize = 2;
/// Definitions with no text.
pub const BROKEN_EMPTY: usize = 2;
/// References written `^[1]` instead of `[^1]`.
pub const BROKEN_TYPO: usize = 2;

/// The table. The HTML row says "370+ (mostly `<br>` 326 and `<hr>` 312)", so the
/// corpus uses those two counts plus a handful of other tags.
pub const FEATURE_TARGETS: [FeatureTarget; 16] = [
    target("inline_math", 3660, 50),
    target("block_math_delimiters", 748, 28),
    target(
        "html_tags",
        HTML_BR_USES + HTML_HR_USES + HTML_OTHER_USES,
        25,
    ),
    target("embeds", 238, 47),
    target("task_items", 243, 14),
    target("table_rows", 203, 17),
    target("markdown_links", 187, 52),
    target("code_fence_lines", 107, 19),
    target("callouts", 91, 29),
    target("frontmatter", 55, 55),
    target("footnote_refs", 54, 19),
    target("footnote_defs", 44, 19),
    target("comments", 17, 7),
    target("tags", 10, 5),
    target("highlights", 7, 7),
    target("wikilinks", 1, 1),
];

const fn target(key: &'static str, uses: usize, notes: usize) -> FeatureTarget {
    FeatureTarget { key, uses, notes }
}

/// Looks up a target by key.
pub fn find(key: &str) -> Option<FeatureTarget> {
    FEATURE_TARGETS.iter().copied().find(|t| t.key == key)
}

/// Scales a count from the 204-note vault to `note_count` notes, keeping every
/// non-zero count at least 1.
pub fn scaled(value: usize, note_count: usize) -> usize {
    if value == 0 || note_count == 0 {
        return 0;
    }
    ((value * note_count + VAULT_NOTE_COUNT / 2) / VAULT_NOTE_COUNT).max(1)
}

/// Uses and notes of every target, scaled to `note_count` notes.
pub fn scaled_targets(note_count: usize) -> Vec<FeatureTarget> {
    FEATURE_TARGETS
        .iter()
        .map(|t| FeatureTarget {
            key: t.key,
            uses: scaled(t.uses, note_count),
            notes: scaled(t.notes, note_count).min(note_count),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_size_is_unchanged() {
        assert_eq!(scaled(3660, VAULT_NOTE_COUNT), 3660);
        assert_eq!(scaled_targets(VAULT_NOTE_COUNT)[0].notes, 50);
    }

    #[test]
    fn small_vaults_keep_rare_features() {
        assert_eq!(scaled(1, 10), 1);
        assert_eq!(scaled(0, 10), 0);
        assert_eq!(scaled(3660, 102), 1830);
    }

    #[test]
    fn find_knows_every_key() {
        for t in FEATURE_TARGETS {
            assert_eq!(find(t.key), Some(t));
        }
        assert_eq!(find("nope"), None);
    }
}
