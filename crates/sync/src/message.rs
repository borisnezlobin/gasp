use std::path::{Path, PathBuf};

/// Most files a commit message names before it counts them instead.
const MOST_NAMED_FILES: usize = 3;

/// The message for a commit of `paths` made on `device`, as `vault-sync`
/// wrote them: `mac: Lemma.md, Habit Ideas.md`, or `mac: 5 files changed`
/// once there are too many to name.
pub fn commit_message(device: &str, paths: &[PathBuf]) -> String {
    let summary = if paths.len() > MOST_NAMED_FILES {
        format!("{} files changed", paths.len())
    } else {
        let names: Vec<String> = paths.iter().map(|path| file_name(path)).collect();
        names.join(", ")
    };
    format!("{device}: {summary}")
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn a_few_files_are_named_without_their_folders() {
        let changed = paths(&["Lemma.md", "Ideas/Habit Ideas.md"]);
        assert_eq!(
            commit_message("mac", &changed),
            "mac: Lemma.md, Habit Ideas.md"
        );
        assert_eq!(commit_message("mac", &changed[..1]), "mac: Lemma.md");
    }

    #[test]
    fn many_files_are_counted() {
        let changed = paths(&["a.md", "b.md", "c.md", "d.md", "e.md"]);
        assert_eq!(commit_message("mac", &changed), "mac: 5 files changed");
        assert_eq!(
            commit_message("laptop", &changed[..3]),
            "laptop: a.md, b.md, c.md"
        );
    }
}
