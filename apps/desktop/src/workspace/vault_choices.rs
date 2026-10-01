//! The vaults the switcher offers: recent ones that still exist, each
//! once, never a temporary folder, and with where it lives beside its
//! name whenever two share a name.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// A vault in the switcher.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultChoice {
    pub path: PathBuf,
    pub name: String,
    /// Where it lives, shown only when another choice has the same name.
    pub location: Option<String>,
}

/// The folder iCloud Drive keeps its files in, inside the home folder.
const ICLOUD_DRIVE: &str = "Library/Mobile Documents/com~apple~CloudDocs";

/// Whether `path` is in a folder the system clears on its own, where a
/// test or a scratch run left it rather than someone choosing it.
pub fn is_temporary(path: &Path) -> bool {
    let temp = std::env::temp_dir();
    let canonical_temp = temp.canonicalize().unwrap_or(temp);
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    [canonical_temp.as_path(), Path::new("/private/var/folders"), Path::new("/private/tmp"), Path::new("/tmp")]
        .iter()
        .any(|folder| canonical.starts_with(folder) || path.starts_with(folder))
}

/// The switcher's vaults from `recent` (most recent first), leaving out
/// `current` and any that `exists` says are gone.
pub fn vault_choices(
    recent: &[PathBuf],
    current: &Path,
    home: Option<&Path>,
    exists: impl Fn(&Path) -> bool,
) -> Vec<VaultChoice> {
    let mut seen: HashSet<PathBuf> = HashSet::from([same_folder(current)]);
    let kept: Vec<&PathBuf> = recent
        .iter()
        .filter(|vault| exists(vault) && !is_temporary(vault))
        .filter(|vault| seen.insert(same_folder(vault)))
        .collect();
    let mut names: HashMap<String, usize> = HashMap::new();
    for vault in &kept {
        *names.entry(super::files::folder_name(vault)).or_default() += 1;
    }
    kept.into_iter()
        .map(|vault| {
            let name = super::files::folder_name(vault);
            let shared = names.get(&name).is_some_and(|count| *count > 1);
            VaultChoice {
                path: vault.clone(),
                location: shared.then(|| location_of(vault, home)),
                name,
            }
        })
        .collect()
}

/// One spelling for a folder reached by different paths, such as through
/// `/var` and `/private/var`.
fn same_folder(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// Where a vault lives, as people name it: "iCloud Drive", or its parent
/// folder from the home folder, such as "~/Documents".
fn location_of(vault: &Path, home: Option<&Path>) -> String {
    let parent = vault.parent().unwrap_or(vault);
    let icloud = home.map(|home| home.join(ICLOUD_DRIVE));
    if let Some(rest) = icloud.as_deref().and_then(|drive| parent.strip_prefix(drive).ok()) {
        return match rest.as_os_str().is_empty() {
            true => "iCloud Drive".to_owned(),
            false => format!("iCloud Drive/{}", rest.display()),
        };
    }
    match home.and_then(|home| parent.strip_prefix(home).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Some(rest) => format!("~/{}", rest.display()),
        None => parent.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(list: &[&str]) -> Vec<PathBuf> {
        list.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn same_names_say_where_they_live_and_unique_ones_do_not() {
        let home = Path::new("/Users/ana");
        let recent = paths(&[
            "/Users/ana/Library/Mobile Documents/com~apple~CloudDocs/Gasp",
            "/Users/ana/Documents/Gasp",
            "/Users/ana/Documents/Vault",
        ]);
        let choices = vault_choices(&recent, Path::new("/Users/ana/Notes"), Some(home), |_| true);
        let shown: Vec<(&str, Option<&str>)> = choices
            .iter()
            .map(|choice| (choice.name.as_str(), choice.location.as_deref()))
            .collect();
        assert_eq!(
            shown,
            [("Gasp", Some("iCloud Drive")), ("Gasp", Some("~/Documents")), ("Vault", None)]
        );
    }

    #[test]
    fn the_open_vault_gone_folders_and_repeats_are_left_out() {
        let recent = paths(&["/Users/ana/Notes", "/Users/ana/Old", "/Users/ana/Work", "/Users/ana/Work"]);
        let choices = vault_choices(&recent, Path::new("/Users/ana/Notes"), None, |path| !path.ends_with("Old"));
        let names: Vec<&str> = choices.iter().map(|choice| choice.name.as_str()).collect();
        assert_eq!(names, ["Work"]);
    }

    #[test]
    fn temporary_folders_never_show() {
        let scratch = tempfile::tempdir().unwrap();
        let vault = scratch.path().join("Gasp");
        std::fs::create_dir(&vault).unwrap();
        assert!(is_temporary(&vault));
        assert!(is_temporary(Path::new("/private/var/folders/xy/abc/T/Gasp")));
        assert!(!is_temporary(Path::new("/Users/ana/Documents/Gasp")));
        let choices = vault_choices(&[vault], Path::new("/Users/ana/Notes"), None, |_| true);
        assert!(choices.is_empty());
    }
}
