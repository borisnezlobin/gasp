//! Templates: notes in the templates folder whose text goes in at the
//! cursor with `{{title}}`, `{{date}}` and `{{time}}` filled in, as in
//! Obsidian's core Templates plugin. `{{date:dddd D MMMM}}` gives its own
//! format.

use std::path::{Path, PathBuf};

use jiff::civil::DateTime;

use super::dates;

/// What `{{…}}` placeholders are filled with.
#[derive(Clone, Debug)]
pub struct TemplateContext {
    pub title: String,
    pub now: DateTime,
    pub date_format: String,
    pub time_format: String,
}

/// `template` with its placeholders filled in. Unknown ones stay as
/// written, so a template can hold other `{{…}}` text.
pub fn fill(template: &str, context: &TemplateContext) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find("{{") {
        out.push_str(&rest[..open]);
        let after = &rest[open + 2..];
        let Some(close) = after.find("}}") else {
            out.push_str(&rest[open..]);
            return out;
        };
        let inner = &after[..close];
        if inner.contains("{{") {
            // Another placeholder opens before this one closes.
            out.push_str("{{");
            rest = after;
            continue;
        }
        match placeholder(inner, context) {
            Some(value) => out.push_str(&value),
            None => out.push_str(&rest[open..open + 4 + close]),
        }
        rest = &after[close + 2..];
    }
    out.push_str(rest);
    out
}

fn placeholder(inner: &str, context: &TemplateContext) -> Option<String> {
    let (name, format) = match inner.split_once(':') {
        Some((name, format)) => (name.trim(), Some(format.trim())),
        None => (inner.trim(), None),
    };
    let default = match name.to_lowercase().as_str() {
        "title" => return Some(context.title.clone()),
        "date" => &context.date_format,
        "time" => &context.time_format,
        _ => return None,
    };
    let format = format.filter(|f| !f.is_empty()).unwrap_or(default);
    Some(dates::format(context.now, format))
}

/// Every template in `folder`, as paths relative to it without `.md`,
/// sorted.
pub fn list_templates(folder: &Path) -> Vec<String> {
    let mut found: Vec<String> = super::markdown_files(folder)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|path| {
            let relative = path.strip_prefix(folder).ok()?;
            let name = relative.to_string_lossy().replace('\\', "/");
            Some(name.strip_suffix(".md").unwrap_or(&name).to_string())
        })
        .collect();
    found.sort_by_key(|name| name.to_lowercase());
    found
}

/// Where the templates live: `folder` from the settings, inside `vault`.
pub fn templates_folder(vault: &Path, folder: &str) -> PathBuf {
    vault.join(folder.trim_matches('/'))
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::*;

    fn context() -> TemplateContext {
        TemplateContext {
            title: "Wave Packets".into(),
            now: date(2026, 9, 27).at(8, 30, 0, 0),
            date_format: "YYYY-MM-DD".into(),
            time_format: "HH:mm".into(),
        }
    }

    #[test]
    fn fills_title_date_and_time() {
        let template =
            "# {{title}}\nCreated {{date}} at {{time}}.\n{{date:dddd, D MMMM}} {{ time : h A }}";
        assert_eq!(
            fill(template, &context()),
            "# Wave Packets\nCreated 2026-09-27 at 08:30.\nSunday, 27 September 8 AM"
        );
    }

    #[test]
    fn leaves_other_braces_alone() {
        let template = "{{unknown}} {{ open and {{title}} }} {{date";
        assert_eq!(
            fill(template, &context()),
            "{{unknown}} {{ open and Wave Packets }} {{date"
        );
    }

    #[test]
    fn lists_templates_in_subfolders() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Meetings")).unwrap();
        std::fs::write(dir.path().join("Daily.md"), "").unwrap();
        std::fs::write(dir.path().join("Meetings/One to one.md"), "").unwrap();
        std::fs::write(dir.path().join("notes.txt"), "").unwrap();
        assert_eq!(list_templates(dir.path()), ["Daily", "Meetings/One to one"]);
    }
}
