//! The sample vault the tour offers: a few notes that show what Gasp
//! does, written into a new folder in Documents each time it's asked for.

use std::io;
use std::path::{Path, PathBuf};

/// The sample vault's folder name; a number follows when it's taken.
pub const SAMPLE_VAULT_NAME: &str = "Gasp sample";

/// The note the sample vault opens on.
pub const FIRST_NOTE: &str = "Start here.md";

const NOTES: [(&str, &str); 5] = [
    (
        FIRST_NOTE,
        include_str!("../../assets/tour/sample/Start here.md"),
    ),
    (
        "Writing in Markdown.md",
        include_str!("../../assets/tour/sample/Writing in Markdown.md"),
    ),
    (
        "Links between notes.md",
        include_str!("../../assets/tour/sample/Links between notes.md"),
    ),
    (
        "Math and tables.md",
        include_str!("../../assets/tour/sample/Math and tables.md"),
    ),
    (
        "Reading list.md",
        include_str!("../../assets/tour/sample/Reading list.md"),
    ),
];

/// Writes the sample vault into a new folder in `parent` and answers the
/// folder.
pub fn write_sample_vault(parent: &Path) -> io::Result<PathBuf> {
    let vault = unused_folder(parent, SAMPLE_VAULT_NAME);
    std::fs::create_dir_all(&vault)?;
    for (name, text) in NOTES {
        std::fs::write(vault.join(name), text)?;
    }
    Ok(vault)
}

/// `parent/name`, or `parent/name 2` and so on when that's taken.
fn unused_folder(parent: &Path, name: &str) -> PathBuf {
    let first = parent.join(name);
    if !first.exists() {
        return first;
    }
    (2..)
        .map(|number| parent.join(format!("{name} {number}")))
        .find(|candidate| !candidate.exists())
        .unwrap_or(first)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_sample_gets_its_own_folder() {
        let parent = tempfile::tempdir().unwrap();
        let first = write_sample_vault(parent.path()).unwrap();
        let second = write_sample_vault(parent.path()).unwrap();
        assert_eq!(first, parent.path().join(SAMPLE_VAULT_NAME));
        assert_eq!(second, parent.path().join(format!("{SAMPLE_VAULT_NAME} 2")));
        assert!(second.join(FIRST_NOTE).is_file());
    }

    #[test]
    fn every_link_in_the_sample_finds_a_note_or_makes_one_on_purpose() {
        let names: Vec<&str> = NOTES
            .iter()
            .map(|(name, _)| name.trim_end_matches(".md"))
            .collect();
        let made_on_purpose = ["Ideas for later"];
        for (_, text) in NOTES {
            let targets = text
                .split("[[")
                .skip(1)
                .filter_map(|rest| rest.split_once("]]").map(|(target, _)| target))
                .filter(|target| !target.contains(['`', '\n']));
            for target in targets {
                assert!(
                    names.contains(&target) || made_on_purpose.contains(&target),
                    "{target}"
                );
            }
        }
    }
}
