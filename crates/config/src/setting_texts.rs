//! What each setting is called and what it's for, as both apps show it,
//! and how each option of a choice reads. A description says what the
//! setting changes and why someone would want it, in as few words as that
//! takes, and never restates the title.

/// (key, title, description) for every setting a person sees.
pub const SETTING_TEXTS: &[(&str, &str, &str)] = &[
    (
        "sidebar.files.reveal",
        "Show the file sidebar",
        "Always, from its shortcut, or when the pointer reaches the left edge.",
    ),
    (
        "sidebar.files.mode",
        "Sidebar placement",
        "Over the note, or beside it so the note moves aside.",
    ),
    (
        "markdown.symbols.mode",
        "Markdown symbols",
        "When marks such as ** and # are visible.",
    ),
    (
        "markdown.symbols.scope",
        "Reveal near the cursor",
        "How much markup shows around the cursor.",
    ),
    (
        "markdown.symbols.overrides",
        "Rules for one kind of syntax",
        "Give one kind of syntax its own rule, such as never showing link addresses.",
    ),
    (
        "prose.sentence-length.enabled",
        "Colour sentences by length",
        "Tints short, medium and long sentences so a paragraph's rhythm shows.",
    ),
    (
        "prose.sentence-length.short-below",
        "Short sentences",
        "Fewer words than this counts as short.",
    ),
    (
        "prose.sentence-length.long-above",
        "Long sentences",
        "More words than this counts as long.",
    ),
    (
        "prose.grammar.enabled",
        "Check writing",
        "Underlines doubled words, stray spaces and a and an mix-ups.",
    ),
    (
        "prose.grammar.spelling",
        "Check spelling",
        "Underlines misspelled words. A word you've used in three notes is never flagged.",
    ),
    (
        "prose.grammar.english",
        "Spelling",
        "Which English to follow, such as colour or color.",
    ),
    (
        "recovery.interval-minutes",
        "Minutes between snapshots",
        "While you edit, a copy of the note is kept this often so you can go back to it.",
    ),
    (
        "recovery.keep-days",
        "Keep snapshots for",
        "Days before a snapshot is deleted.",
    ),
    (
        "files.attachments-folder",
        "Attachments folder",
        "Where pasted and dropped images go, relative to the note.",
    ),
    (
        "files.update-links-on-rename",
        "Update links when renaming",
        "Links to a note follow it when you rename or move it.",
    ),
    (
        "files.trash",
        "Deleted notes",
        "Where a note goes when you delete it.",
    ),
    (
        "editor.show-inline-title",
        "Show the note's title",
        "The file name, editable, above the text.",
    ),
    (
        "editor.smart-quotes",
        "Smart quotes",
        "Straight quotes become curly as you type, outside code, math and links.",
    ),
    (
        "editor.curl-pasted-quotes",
        "Curl quotes in pasted text",
        "Pasted text gets curly quotes too.",
    ),
    (
        "editor.auto-pair",
        "Close brackets as you type",
        "Typing ( adds ), and typing * over a selection wraps it.",
    ),
    (
        "editor.code-line-numbers",
        "Number lines in code blocks",
        "A block can still choose with ln:true or ln:false after its language.",
    ),
    (
        "editor.renumber-footnotes",
        "Keep footnotes in order",
        "Numbered footnotes renumber to follow the text when you pause typing.",
    ),
    (
        "editor.snippets",
        "Expand snippets",
        "A snippet's trigger, such as mk, turns into its expansion as you type.",
    ),
    (
        "editor.replacements",
        "Replace as you type",
        "-- becomes an em dash and -> an arrow, outside code and math.",
    ),
    (
        "math.auto-fraction",
        "Make fractions with a slash",
        "In math, a term followed by / becomes a fraction.",
    ),
    (
        "math.matrix-shortcuts",
        "Tab and Enter in matrices",
        "Tab adds a column and Enter a row inside pmatrix, cases, align and the like.",
    ),
    (
        "math.tab-out",
        "Tab out of brackets",
        "In math, Tab jumps past the next closing bracket, then out of the math.",
    ),
    (
        "math.enlarge-brackets",
        "Grow brackets around big operators",
        "Brackets around a sum, integral or fraction become \\left( and \\right).",
    ),
    (
        "math.bracket-colours",
        "Colour matching brackets",
        "Both halves of a bracket pair share a colour in math source.",
    ),
    (
        "appearance.theme",
        "Theme",
        "Light, dark, or whichever the system is using.",
    ),
    (
        "appearance.base-font-size",
        "Font size",
        "Body text size in points. Headings scale with it.",
    ),
    (
        "sync.auto",
        "Sync automatically",
        "After you stop typing, when you come back, and every few minutes. When it's off, sync runs only when you ask.",
    ),
    (
        "sync.interval-minutes",
        "Minutes between checks",
        "How often to look for changes from your other devices.",
    ),
    (
        "sync.branch",
        "Branch",
        "The Git branch this device saves to.",
    ),
    (
        "sync.legacy-branch",
        "Also bring in",
        "A branch older sync tools use. Its changes are merged in one way. Leave it empty to turn it off.",
    ),
    (
        "sync.device-only",
        "Files that stay on this device",
        "Patterns for files that never sync, such as window layouts.",
    ),
    (
        "daily-notes.folder",
        "Daily notes folder",
        "Leave it empty to put them at the top of the vault.",
    ),
    (
        "daily-notes.format",
        "Daily note name",
        "YYYY is the year, MM the month and DD the day, so YYYY-MM-DD gives names like 2026-09-29. A slash makes folders.",
    ),
    (
        "daily-notes.template",
        "Daily note template",
        "A note each new daily note starts as a copy of. Leave it empty to start blank.",
    ),
    (
        "templates.folder",
        "Templates folder",
        "Insert template offers the notes in this folder.",
    ),
    (
        "templates.date-format",
        "Date format",
        "How {{date}} is written. YYYY is the year, MM the month, DD the day and dddd the weekday.",
    ),
    (
        "templates.time-format",
        "Time format",
        "How {{time}} is written. HH is the hour and mm the minute, so HH:mm gives 14:05.",
    ),
    (
        "mcp.enabled",
        "Agent access",
        concat!(
            "Lets agents and the command line see and change what's open in Gasp, through ",
            crate::command_name!(),
            " mcp."
        ),
    ),
    (
        "telemetry.enabled",
        "Send anonymous usage data",
        "Once a day, Gasp sends its version, system version and chip type so we can count its users. Never your notes or file names.",
    ),
];

/// How each option of a choice reads. Values are unique across settings.
pub const CHOICE_LABELS: &[(&str, &str)] = &[
    ("always", "Always open"),
    ("toggle", "From its shortcut"),
    ("hover", "On hover"),
    ("overlay", "Over the note"),
    ("push", "Beside the note"),
    ("always-shown", "Always"),
    ("around-cursor", "Near the cursor"),
    ("always-hidden", "Never"),
    ("element", "Just the element"),
    ("line", "The whole line"),
    ("block", "The whole block"),
    ("system", "System trash"),
    ("vault", "The vault's .trash folder"),
    ("delete", "Delete for good"),
    ("light", "Light"),
    ("dark", "Dark"),
    ("match-system", "Match system"),
    ("american", "American"),
    ("british", "British"),
    ("canadian", "Canadian"),
    ("australian", "Australian"),
];

/// Settings no app shows, by key prefix.
pub const HIDDEN: &[&str] = &["mobile."];

/// The title and description of the setting `key`, if it has them.
pub fn setting_text(key: &str) -> Option<(&'static str, &'static str)> {
    SETTING_TEXTS
        .iter()
        .find(|(known, ..)| *known == key)
        .map(|(_, title, description)| (*title, *description))
}

/// How the choice `value` reads, if it has a label.
pub fn choice_label(value: &str) -> Option<&'static str> {
    CHOICE_LABELS
        .iter()
        .find(|(known, _)| *known == value)
        .map(|(_, label)| *label)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::setting_descriptors;

    #[test]
    fn every_shown_setting_has_a_title_and_a_description() {
        for descriptor in setting_descriptors() {
            if HIDDEN
                .iter()
                .any(|prefix| descriptor.key.starts_with(prefix))
            {
                continue;
            }
            assert!(
                setting_text(&descriptor.key).is_some(),
                "{} has no text",
                descriptor.key
            );
        }
    }

    #[test]
    fn every_key_in_the_table_is_a_setting() {
        let keys: Vec<String> = setting_descriptors().into_iter().map(|d| d.key).collect();
        for (key, ..) in SETTING_TEXTS {
            assert!(keys.iter().any(|k| k == key), "{key} isn't a setting");
        }
    }

    #[test]
    fn descriptions_stay_short() {
        for (key, _, description) in SETTING_TEXTS {
            assert!(description.len() <= 130, "{key} says too much");
        }
    }
}
