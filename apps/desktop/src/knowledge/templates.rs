//! Templates: notes in the templates folder whose text goes in at the
//! cursor with `{{title}}`, `{{date}}` and `{{time}}` filled in, as in
//! Obsidian's core Templates plugin. `{{date:dddd D MMMM}}` gives its own
//! format.
//!
//! "Insert template" (`template.insert`) lists them in a picker.

use std::path::{Path, PathBuf};

use gpui::{
    AnyElement, App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, Render,
    SharedString, Subscription, Window, div, prelude::*,
};
use jiff::civil::DateTime;

use super::dates;
use crate::picker::fuzzy::{Candidate, Matcher, Query};
use crate::picker::{Confirmed, Picker, PickerDelegate, highlighted_text};
use crate::theme::PickerTheme;

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
    let mut found: Vec<String> = crate::note::markdown_files(folder)
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

/// The template picker's rows and matching.
pub struct TemplateDelegate {
    folder: PathBuf,
    folder_name: String,
    names: Vec<String>,
    candidates: Vec<Candidate>,
    matches: Vec<(usize, Vec<usize>)>,
    matcher: Matcher,
}

impl TemplateDelegate {
    /// The templates in `folder`, whose name in the settings is
    /// `folder_name`.
    pub fn new(folder: PathBuf, folder_name: &str) -> Self {
        let names = list_templates(&folder);
        TemplateDelegate {
            candidates: names.iter().map(|name| Candidate::new(name)).collect(),
            names,
            folder,
            folder_name: folder_name.to_string(),
            matches: Vec::new(),
            matcher: Matcher::new(),
        }
    }

    pub fn names(&self) -> &[String] {
        &self.names
    }

    /// The template shown in row `index`.
    pub fn path_at(&self, index: usize) -> Option<PathBuf> {
        let (template, _) = self.matches.get(index)?;
        Some(self.folder.join(format!("{}.md", self.names[*template])))
    }
}

impl PickerDelegate for TemplateDelegate {
    type Event = PathBuf;

    fn placeholder(&self) -> SharedString {
        "Insert a template".into()
    }

    fn match_count(&self) -> usize {
        self.matches.len()
    }

    fn update_matches(&mut self, query: &str) {
        let query = Query::new(query);
        let mut scored: Vec<(i32, usize, Vec<usize>)> = self
            .candidates
            .iter()
            .enumerate()
            .filter_map(|(index, candidate)| {
                let found = self.matcher.score(&query, candidate)?;
                Some((found.score, index, found.positions))
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        self.matches = scored
            .into_iter()
            .map(|(_, index, positions)| (index, positions))
            .collect();
    }

    fn render_match(&self, index: usize, _selected: bool, theme: &PickerTheme) -> AnyElement {
        let Some((template, positions)) = self.matches.get(index) else {
            return div().into_any_element();
        };
        div()
            .w_full()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .text_size(theme.row_font_size)
            .child(highlighted_text(
                self.names[*template].clone(),
                positions,
                theme,
            ))
            .into_any_element()
    }

    fn confirm(&mut self, index: usize) -> Option<PathBuf> {
        self.path_at(index)
    }

    fn empty_message(&self, query: &str) -> SharedString {
        if self.names.is_empty() {
            return format!(
                "There are no templates yet. Notes you put in the “{}” folder show up here.",
                self.folder_name
            )
            .into();
        }
        format!("No templates match “{}”.", query.trim()).into()
    }
}

/// Insert template. Emits the chosen template's path, then [`DismissEvent`].
pub struct TemplatePicker {
    picker: Entity<Picker<TemplateDelegate>>,
    _subscriptions: Vec<Subscription>,
}

/// The template the user picked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplateChosen(pub PathBuf);

impl EventEmitter<TemplateChosen> for TemplatePicker {}
impl EventEmitter<DismissEvent> for TemplatePicker {}

impl Focusable for TemplatePicker {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.picker.focus_handle(cx)
    }
}

impl TemplatePicker {
    pub fn new(
        folder: PathBuf,
        folder_name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let delegate = TemplateDelegate::new(folder, folder_name);
        let picker = cx.new(|cx| Picker::new(delegate, window, cx));
        let subscriptions = vec![
            cx.subscribe(&picker, |_, _, event: &Confirmed<PathBuf>, cx| {
                cx.emit(TemplateChosen(event.0.clone()));
                cx.emit(DismissEvent);
            }),
            cx.subscribe(&picker, |_, _, _: &DismissEvent, cx| cx.emit(DismissEvent)),
        ];
        TemplatePicker {
            picker,
            _subscriptions: subscriptions,
        }
    }

    pub fn picker(&self) -> &Entity<Picker<TemplateDelegate>> {
        &self.picker
    }
}

impl Render for TemplatePicker {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.picker.clone()
    }
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
        let mut delegate = TemplateDelegate::new(dir.path().into(), "Templates");
        delegate.update_matches("one");
        assert_eq!(
            delegate.confirm(0),
            Some(dir.path().join("Meetings/One to one.md"))
        );
    }

    #[test]
    fn an_empty_folder_says_where_templates_go() {
        let dir = tempfile::tempdir().unwrap();
        let mut delegate = TemplateDelegate::new(dir.path().join("missing"), "Templates");
        delegate.update_matches("");
        assert_eq!(delegate.match_count(), 0);
        assert!(delegate.empty_message("").contains("“Templates” folder"));
    }
}
