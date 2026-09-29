//! Daily notes: one note per day, named by the date in the daily-notes
//! folder, started from a template when it's first opened.

use std::io;
use std::path::{Path, PathBuf};

use gasp_config::settings::{DailyNoteSettings, TemplateSettings};
use jiff::civil::DateTime;

use super::dates;
use super::templates::{TemplateContext, fill};
use crate::files::atomic_write;

/// Where the daily note for `now` lives.
pub fn daily_note_path(vault: &Path, settings: &DailyNoteSettings, now: DateTime) -> PathBuf {
    let format = match settings.format.trim() {
        "" => "YYYY-MM-DD",
        format => format,
    };
    let name = dates::format(now, format);
    let folder = settings.folder.trim().trim_matches('/');
    let dir = if folder.is_empty() {
        vault.to_path_buf()
    } else {
        vault.join(folder)
    };
    dir.join(format!("{name}.md"))
}

/// The daily note for `now`, made from its template if it doesn't exist
/// yet. Returns its path and whether it was just made.
pub fn ensure_daily_note(
    vault: &Path,
    daily: &DailyNoteSettings,
    templates: &TemplateSettings,
    now: DateTime,
) -> io::Result<(PathBuf, bool)> {
    let path = daily_note_path(vault, daily, now);
    if path.exists() {
        return Ok((path, false));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = match template_path(vault, &daily.template) {
        Some(template) => {
            let context = TemplateContext {
                title: super::note_title(&path),
                now,
                date_format: templates.date_format.clone(),
                time_format: templates.time_format.clone(),
            };
            fill(&std::fs::read_to_string(template)?, &context)
        }
        None => String::new(),
    };
    atomic_write(&path, &text)?;
    Ok((path, true))
}

/// The template note the setting names, with or without `.md`.
fn template_path(vault: &Path, setting: &str) -> Option<PathBuf> {
    let name = setting.trim().trim_matches('/');
    if name.is_empty() {
        return None;
    }
    let file = if name.to_lowercase().ends_with(".md") {
        name.to_string()
    } else {
        format!("{name}.md")
    };
    Some(vault.join(file))
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::*;

    fn settings(folder: &str, format: &str, template: &str) -> DailyNoteSettings {
        DailyNoteSettings {
            folder: folder.into(),
            format: format.into(),
            template: template.into(),
        }
    }

    #[test]
    fn paths_follow_the_folder_and_format() {
        let now = date(2026, 9, 27).at(9, 0, 0, 0);
        let vault = Path::new("/v");
        assert_eq!(
            daily_note_path(vault, &settings("", "YYYY-MM-DD", ""), now),
            Path::new("/v/2026-09-27.md")
        );
        assert_eq!(
            daily_note_path(vault, &settings("/Daily Notes/", "YYYY/MM/D MMM", ""), now),
            Path::new("/v/Daily Notes/2026/09/27 Sep.md")
        );
        assert_eq!(
            daily_note_path(vault, &settings("", " ", ""), now),
            Path::new("/v/2026-09-27.md")
        );
    }

    #[test]
    fn a_new_daily_note_starts_from_its_template_once() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path();
        std::fs::create_dir_all(vault.join("Templates")).unwrap();
        std::fs::write(
            vault.join("Templates/Day.md"),
            "# {{title}}\n{{date:dddd}}\n",
        )
        .unwrap();
        let now = date(2026, 9, 27).at(9, 0, 0, 0);
        let daily = settings("Journal", "YYYY-MM-DD", "Templates/Day");
        let templates = TemplateSettings::default();
        let (path, created) = ensure_daily_note(vault, &daily, &templates, now).unwrap();
        assert!(created);
        assert_eq!(path, vault.join("Journal/2026-09-27.md"));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "# 2026-09-27\nSunday\n"
        );
        std::fs::write(&path, "edited").unwrap();
        let (_, created) = ensure_daily_note(vault, &daily, &templates, now).unwrap();
        assert!(!created);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "edited");
    }

    #[test]
    fn a_missing_template_is_an_error_not_an_empty_note() {
        let dir = tempfile::tempdir().unwrap();
        let now = date(2026, 9, 27).at(9, 0, 0, 0);
        let daily = settings("", "YYYY-MM-DD", "Nope");
        let result = ensure_daily_note(dir.path(), &daily, &TemplateSettings::default(), now);
        assert!(result.is_err());
        assert!(!dir.path().join("2026-09-27.md").exists());
    }
}
