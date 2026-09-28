//! Templates: notes in the templates folder whose text goes in at the
//! cursor with `{{title}}`, `{{date}}` and `{{time}}` filled in, as in
//! Obsidian's core Templates plugin. `{{date:dddd D MMMM}}` gives its own
//! format.
//!
//! "Insert template" (`template.insert`) lists them in a picker.

use std::path::PathBuf;

use gpui::{
    AnyElement, App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, Render,
    SharedString, Subscription, Window, div, prelude::*,
};

use crate::picker::fuzzy::{Candidate, Matcher, Query};
use crate::picker::{Confirmed, Picker, PickerDelegate, highlighted_text};
use crate::theme::PickerTheme;
pub use editor_vault::knowledge::templates::{
    TemplateContext, fill, list_templates, templates_folder,
};

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
    use super::*;

    #[test]
    fn the_picker_finds_templates_in_subfolders() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Meetings")).unwrap();
        std::fs::write(dir.path().join("Daily.md"), "").unwrap();
        std::fs::write(dir.path().join("Meetings/One to one.md"), "").unwrap();
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
