use std::fs;
use std::path::Path;

use globset::{Glob, GlobSet, GlobSetBuilder};

use crate::error::{SyncError, SyncResult};

/// Files that belong to one device and never sync, as globs relative to the vault root.
///
/// Per-device stats under `.gasp/stats/` are deliberately absent: each
/// device writes its own file there and those files do sync.
pub const DEFAULT_DEVICE_ONLY_GLOBS: &[&str] = gasp_config::settings::DEFAULT_DEVICE_ONLY;

const EXCLUDE_BEGIN: &str = concat!(
    "# ",
    gasp_config::command_name!(),
    ": device-only files (managed, do not edit)"
);
const EXCLUDE_END: &str = concat!(
    "# ",
    gasp_config::command_name!(),
    ": end of device-only files"
);
const LEGACY_EXCLUDE_BEGIN: &str = "# editor: device-only files (managed, do not edit)";
const LEGACY_EXCLUDE_END: &str = "# editor: end of device-only files";

/// The set of device-only globs, matched against `/`-separated vault paths.
#[derive(Debug, Clone)]
pub struct DeviceOnlyFiles {
    globs: Vec<String>,
    matcher: GlobSet,
}

impl DeviceOnlyFiles {
    pub fn new<S: AsRef<str>>(globs: &[S]) -> SyncResult<Self> {
        let mut builder = GlobSetBuilder::new();
        for glob in globs {
            let glob = glob.as_ref();
            let parsed = Glob::new(glob).map_err(|error| SyncError::InvalidGlob {
                glob: glob.to_owned(),
                message: error.to_string(),
            })?;
            builder.add(parsed);
        }
        let matcher = builder.build().map_err(|error| SyncError::InvalidGlob {
            glob: globs
                .iter()
                .map(AsRef::as_ref)
                .collect::<Vec<_>>()
                .join(", "),
            message: error.to_string(),
        })?;
        let globs = globs.iter().map(|glob| glob.as_ref().to_owned()).collect();
        Ok(Self { globs, matcher })
    }

    pub fn globs(&self) -> &[String] {
        &self.globs
    }

    /// True when `path` (relative to the vault root) never syncs.
    pub fn matches(&self, path: &Path) -> bool {
        self.matcher.is_match(path)
    }

    /// The globs as gitignore lines: `**/name` floats, anything else is anchored.
    pub fn exclude_lines(&self) -> Vec<String> {
        self.globs.iter().map(|glob| gitignore_line(glob)).collect()
    }

    /// Rewrites the managed block of `.git/info/exclude`, keeping everything else.
    pub fn write_exclude(&self, git_dir: &Path) -> SyncResult<()> {
        let info_dir = git_dir.join("info");
        fs::create_dir_all(&info_dir)?;
        let exclude_path = info_dir.join("exclude");
        let existing = match fs::read_to_string(&exclude_path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(error.into()),
        };
        fs::write(&exclude_path, self.merged_exclude(&existing))?;
        Ok(())
    }

    fn merged_exclude(&self, existing: &str) -> String {
        let mut kept = without_managed_block(existing);
        if !kept.is_empty() && !kept.ends_with('\n') {
            kept.push('\n');
        }
        kept.push_str(EXCLUDE_BEGIN);
        kept.push('\n');
        for line in self.exclude_lines() {
            kept.push_str(&line);
            kept.push('\n');
        }
        kept.push_str(EXCLUDE_END);
        kept.push('\n');
        kept
    }
}

impl Default for DeviceOnlyFiles {
    fn default() -> Self {
        Self::new(DEFAULT_DEVICE_ONLY_GLOBS).expect("default device-only globs are valid")
    }
}

fn gitignore_line(glob: &str) -> String {
    match glob.strip_prefix("**/") {
        Some(floating) => floating.to_owned(),
        None => format!("/{glob}"),
    }
}

fn without_managed_block(text: &str) -> String {
    let mut kept = String::new();
    let mut inside = false;
    for line in text.lines() {
        match line {
            EXCLUDE_BEGIN | LEGACY_EXCLUDE_BEGIN => inside = true,
            EXCLUDE_END | LEGACY_EXCLUDE_END => inside = false,
            _ if !inside => {
                kept.push_str(line);
                kept.push('\n');
            }
            _ => {}
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use gasp_config::CONFIG_DIR;
    use gasp_config::names::LEGACY_CONFIG_DIR;

    use super::*;

    #[test]
    fn default_globs_match_device_files_but_not_stats() {
        let device = DeviceOnlyFiles::default();
        let never_sync = [
            format!("{CONFIG_DIR}/device.toml"),
            format!("{LEGACY_CONFIG_DIR}/device.toml"),
            ".obsidian/workspace.json".into(),
            ".obsidian/workspace-mobile.json".into(),
            ".DS_Store".into(),
            "notes/deep/.DS_Store".into(),
            ".trash/old note.md".into(),
        ];
        for path in never_sync {
            assert!(
                device.matches(Path::new(&path)),
                "{path} should be device-only"
            );
        }
        let syncs = [
            format!("{CONFIG_DIR}/stats/laptop.json"),
            format!("{CONFIG_DIR}/settings.toml"),
            ".obsidian/app.json".into(),
            "notes/a.md".into(),
        ];
        for path in syncs {
            assert!(!device.matches(Path::new(&path)), "{path} should sync");
        }
    }

    #[test]
    fn exclude_lines_anchor_rooted_globs() {
        let device = DeviceOnlyFiles::default();
        assert_eq!(
            device.exclude_lines(),
            vec![
                format!("/{CONFIG_DIR}/device.toml"),
                format!("/{LEGACY_CONFIG_DIR}/device.toml"),
                "/.obsidian/workspace*.json".into(),
                ".DS_Store".into(),
                "/.trash/**".into()
            ]
        );
    }

    #[test]
    fn a_legacy_managed_block_is_replaced_rather_than_kept() {
        let legacy = format!(
            "# user line\n{LEGACY_EXCLUDE_BEGIN}\n/{LEGACY_CONFIG_DIR}/device.toml\n{LEGACY_EXCLUDE_END}\n"
        );
        let merged = DeviceOnlyFiles::default().merged_exclude(&legacy);
        assert!(merged.starts_with("# user line\n"));
        assert!(!merged.contains(LEGACY_EXCLUDE_BEGIN));
        assert!(!merged.contains(LEGACY_EXCLUDE_END));
        assert_eq!(merged.matches(EXCLUDE_BEGIN).count(), 1);
        assert_eq!(merged.matches(EXCLUDE_END).count(), 1);
        assert!(merged.contains(&format!("/{CONFIG_DIR}/device.toml\n")));
    }

    #[test]
    fn managed_block_is_replaced_and_user_lines_kept() {
        let device = DeviceOnlyFiles::new(&["a.txt"]).unwrap();
        let first = device.merged_exclude("# user line\n*.log");
        let other = DeviceOnlyFiles::new(&["b.txt"]).unwrap();
        let second = other.merged_exclude(&first);
        assert!(second.starts_with("# user line\n*.log\n"));
        assert!(second.contains("/b.txt\n"));
        assert!(!second.contains("/a.txt"));
        assert_eq!(second.matches(EXCLUDE_BEGIN).count(), 1);
    }

    #[test]
    fn bad_glob_is_reported() {
        assert!(matches!(
            DeviceOnlyFiles::new(&["a[.txt"]),
            Err(SyncError::InvalidGlob { .. })
        ));
    }
}
