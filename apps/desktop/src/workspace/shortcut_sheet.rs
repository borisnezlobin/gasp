//! Holding Mod (Cmd on macOS, Ctrl elsewhere) alone for a moment shows a
//! sheet of the shortcuts that work where the keyboard is: the file
//! tree's or the right sidebar's own keys first, then every command with
//! a key that runs there, grouped as the palette groups them. Letting go,
//! pressing another key or clicking hides it.

use std::time::Duration;

use editor_config::commands::BUILTIN_COMMANDS;
use editor_config::keys::{Key, NamedKey};
use gpui::{
    AnyElement, App, Context, Focusable, Modifiers, ModifiersChangedEvent, SharedString, Task,
    Window, div, prelude::*,
};

use super::Workspace;
use crate::commands::handles;
use crate::icons::IconName;
use crate::picker::shortcut::Shortcut;
use crate::theme::UiTheme;
use crate::ui::hints::{chord, command_title, shortcut};
use crate::ui::keycap::{Glyph, glyphs, keycap_glyphs};
use crate::ui::{truncated, ui_theme};

/// The most columns the sheet has; a narrower window gets fewer.
const SHEET_COLUMNS: usize = 3;

/// How long Mod has to be held alone before the sheet shows.
pub const HOLD_DELAY: Duration = Duration::from_millis(600);

/// Where the keyboard is, as far as the sheet cares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusArea {
    Editor,
    FileTree,
    RightSidebar,
}

impl FocusArea {
    /// The name over the area's own keys.
    fn group_title(self) -> &'static str {
        match self {
            FocusArea::Editor => "Editor",
            FocusArea::FileTree => "File tree",
            FocusArea::RightSidebar => "Sidebar",
        }
    }

    fn title(self) -> &'static str {
        match self {
            FocusArea::Editor => "Shortcuts in the editor",
            FocusArea::FileTree => "Shortcuts in the file tree",
            FocusArea::RightSidebar => "Shortcuts in the sidebar",
        }
    }

    /// The area's own keys, which aren't commands.
    fn own_keys(self) -> &'static [(&'static str, &'static str)] {
        match self {
            FocusArea::Editor => &[],
            FocusArea::FileTree => &crate::file_tree::KEY_HINTS,
            FocusArea::RightSidebar => &crate::knowledge::KEY_HINTS,
        }
    }
}

/// Commands everyone knows the keys for, left off so the rest stand out.
const WELL_KNOWN: [&str; 16] = [
    "edit.undo",
    "edit.redo",
    "edit.copy",
    "edit.cut",
    "edit.paste",
    "select.all",
    "edit.indent",
    "edit.outdent",
    "edit.newline",
    "edit.delete-backward",
    "edit.delete-forward",
    "edit.delete-word-backward",
    "edit.delete-word-forward",
    "edit.delete-to-line-start",
    "edit.delete-to-line-end",
    "palette.open",
];

/// Shortcuts that share their modifiers and differ by arrow, shown as one
/// row: (the commands, left, right, up and down; the row's title).
const FAMILIES: [([&str; 4], &str); 2] = [
    (
        [
            "pane.focus-left",
            "pane.focus-right",
            "pane.focus-up",
            "pane.focus-down",
        ],
        "Focus a pane",
    ),
    (
        [
            "pane.move-tab-left",
            "pane.move-tab-right",
            "pane.move-tab-up",
            "pane.move-tab-down",
        ],
        "Move the tab to a pane",
    ),
];

/// One shortcut in the sheet.
#[derive(Clone, Debug, PartialEq)]
pub struct SheetRow {
    pub title: SharedString,
    pub keys: Vec<Glyph>,
}

/// Shortcuts shown together, under a name.
#[derive(Clone, Debug, PartialEq)]
pub struct SheetGroup {
    pub title: SharedString,
    pub rows: Vec<SheetRow>,
}

/// Whether a command earns a row: not one everyone knows, and one row
/// for the nine tab numbers.
fn earns_row(id: &str) -> bool {
    let later_tab = id.starts_with("tab.go-") && id != "tab.go-1";
    !later_tab && !WELL_KNOWN.contains(&id)
}

/// The row for command `id`, or `None` when it has no key or another
/// command's row stands for it.
fn command_row(id: &str, cx: &App) -> Option<SheetRow> {
    let family = FAMILIES.iter().find(|(members, _)| members.contains(&id));
    if let Some((members, title)) = family
        && let Some(keys) = family_keys(members, cx)
    {
        return (members[0] == id).then(|| SheetRow {
            title: (*title).into(),
            keys,
        });
    }
    let title = if id == "tab.go-1" {
        "Go to tabs 1–9".into()
    } else {
        command_title(id)
    };
    Some(SheetRow {
        title,
        keys: glyphs(shortcut(id, cx)?),
    })
}

/// The shared modifiers and the four arrows, when every member has its
/// arrow under the same modifiers; otherwise each keeps its own row.
fn family_keys(members: &[&str; 4], cx: &App) -> Option<Vec<Glyph>> {
    let shortcuts: Vec<Shortcut> = members
        .iter()
        .map(|id| shortcut(id, cx))
        .collect::<Option<_>>()?;
    let modifiers = shortcuts[0].chord.modifiers;
    let arrows = [
        NamedKey::Left,
        NamedKey::Right,
        NamedKey::Up,
        NamedKey::Down,
    ];
    let matches = shortcuts.iter().zip(arrows).all(|(shortcut, arrow)| {
        shortcut.chord.modifiers == modifiers && shortcut.chord.key == Key::Named(arrow)
    });
    if !matches {
        return None;
    }
    let mut keys = glyphs(shortcuts[0]);
    keys.pop();
    keys.extend(
        [
            IconName::ArrowLeft,
            IconName::ArrowRight,
            IconName::ArrowUp,
            IconName::ArrowDown,
        ]
        .map(Glyph::Icon),
    );
    Some(keys)
}

/// The groups the sheet shows for `area`.
/// Only commands `runs` accepts are listed.
pub fn sheet_groups(area: FocusArea, runs: impl Fn(&str) -> bool, cx: &App) -> Vec<SheetGroup> {
    let own: Vec<SheetRow> = area
        .own_keys()
        .iter()
        .filter_map(|(keys, title)| {
            Some(SheetRow {
                title: (*title).into(),
                keys: glyphs(chord(keys, cx)?),
            })
        })
        .collect();
    let mut groups: Vec<SheetGroup> = Vec::new();
    if !own.is_empty() {
        groups.push(SheetGroup {
            title: area.group_title().into(),
            rows: own,
        });
    }
    // Outside the editor only the workspace's commands run.
    let runs_here = |id: &str| area == FocusArea::Editor || !handles(id);
    let commands = BUILTIN_COMMANDS
        .iter()
        .filter(|spec| spec.palette && earns_row(spec.id) && runs_here(spec.id) && runs(spec.id));
    for spec in commands {
        let Some(row) = command_row(spec.id, cx) else {
            continue;
        };
        match groups.iter_mut().find(|group| group.title == spec.category) {
            Some(group) => group.rows.push(row),
            None => groups.push(SheetGroup {
                title: spec.category.into(),
                rows: vec![row],
            }),
        }
    }
    groups
}

/// A line of the sheet: a group's name or one of its rows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SheetLine<'a> {
    Title(&'a str),
    Row(&'a SheetRow),
}

/// Flows the groups down `columns` columns of about equal height, like a
/// newspaper: a long group carries on at the top of the next column, and
/// a name never ends a column.
pub fn flow(groups: &[SheetGroup], columns: usize) -> Vec<Vec<SheetLine<'_>>> {
    let lines: Vec<SheetLine<'_>> = groups
        .iter()
        .flat_map(|group| {
            std::iter::once(SheetLine::Title(&group.title))
                .chain(group.rows.iter().map(SheetLine::Row))
        })
        .collect();
    let height = lines.len().div_ceil(columns.max(1)).max(1);
    let mut flowed: Vec<Vec<SheetLine<'_>>> = vec![Vec::new()];
    for line in lines {
        let filled = flowed.last().map_or(0, Vec::len);
        let full = filled >= height;
        let orphan = matches!(line, SheetLine::Title(_)) && filled > 0 && filled + 1 >= height;
        if (full || orphan) && flowed.len() < columns {
            flowed.push(Vec::new());
        }
        if let Some(column) = flowed.last_mut() {
            column.push(line);
        }
    }
    flowed
}

/// The sheet's state: the wait while Mod is held, then what it shows.
#[derive(Default)]
pub struct ShortcutSheet {
    shown: Option<FocusArea>,
    hold: Option<Task<()>>,
    /// A key or click came while modifiers were down: they were for a
    /// shortcut, so nothing shows until every modifier is let go.
    spent: bool,
}

impl Workspace {
    /// Where the keyboard is, or `None` while a dialog has it: dialogs
    /// show their keys themselves.
    pub fn focus_area(&self, window: &Window, cx: &App) -> Option<FocusArea> {
        if self.modal.has_focus(window, cx) {
            return None;
        }
        let in_tree = self
            .file_tree
            .as_ref()
            .is_some_and(|tree| tree.focus_handle(cx).contains_focused(window, cx));
        if in_tree {
            return Some(FocusArea::FileTree);
        }
        if self.right_panel_has_focus(window, cx) {
            return Some(FocusArea::RightSidebar);
        }
        Some(FocusArea::Editor)
    }

    /// Which area's sheet shows, if one does.
    pub fn shortcut_sheet(&self) -> Option<FocusArea> {
        self.sheet.shown
    }

    pub(super) fn on_modifiers_changed(
        &mut self,
        event: &ModifiersChangedEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !event.modifiers.modified() {
            self.sheet.spent = false;
        }
        if event.modifiers != Modifiers::secondary_key() {
            return self.hide_shortcut_sheet(cx);
        }
        let waiting = self.sheet.hold.is_some() || self.sheet.shown.is_some();
        if waiting || self.sheet.spent {
            return;
        }
        self.sheet.hold = Some(cx.spawn_in(window, async move |workspace, cx| {
            cx.background_executor().timer(HOLD_DELAY).await;
            workspace
                .update_in(cx, |workspace, window, cx| {
                    workspace.sheet.hold = None;
                    workspace.sheet.shown = workspace.focus_area(window, cx);
                    cx.notify();
                })
                .ok();
        }));
    }

    /// Hides the sheet on every keystroke in this window, before the
    /// keystroke runs anything: a shortcut runs without the sheet
    /// flashing up behind it.
    pub(super) fn watch_keystrokes_for_sheet(&mut self, window: &Window, cx: &mut Context<Self>) {
        let this_window = window.window_handle();
        let workspace = cx.entity().downgrade();
        let watch = cx.intercept_keystrokes(move |event, window, cx| {
            if window.window_handle() != this_window {
                return;
            }
            let held = event.keystroke.modifiers.modified();
            workspace
                .update(cx, |workspace, cx| workspace.spend_shortcut_sheet(held, cx))
                .ok();
        });
        self._subscriptions.push(watch);
    }

    /// A key or click: whatever modifiers are `held` belong to it.
    pub(super) fn spend_shortcut_sheet(&mut self, held: bool, cx: &mut Context<Self>) {
        self.sheet.spent = held;
        self.hide_shortcut_sheet(cx);
    }

    /// Stops waiting for the hold and hides the sheet.
    pub(super) fn hide_shortcut_sheet(&mut self, cx: &mut Context<Self>) {
        self.sheet.hold = None;
        if self.sheet.shown.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn render_shortcut_sheet(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let area = self.sheet.shown?;
        let ui = ui_theme(cx);
        let groups = sheet_groups(area, |id| self.can_run(id), cx);
        // As many columns as fit the window; what doesn't fit its height
        // scrolls.
        let room = window.viewport_size().width - ui.space_xl * 4.;
        let fits = (room + ui.space_xl) / (ui.sheet_column_width + ui.space_xl);
        let count = (fits.floor() as usize).clamp(1, SHEET_COLUMNS);
        let columns = flow(&groups, count).into_iter().map(|lines| {
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w(ui.sheet_column_width)
                .children(lines.into_iter().map(|line| render_line(line, &ui)))
        });
        let sheet = crate::ui::dialog(&ui)
            .id("shortcut-sheet")
            .debug_selector(|| "shortcut-sheet".to_owned())
            .w(ui.sheet_width)
            .max_h_full()
            .overflow_hidden()
            .px(ui.space_xl)
            .pt(ui.space_lg)
            .pb(ui.space_xl)
            .gap(ui.space_md)
            .child(
                div()
                    .text_size(ui.font_size + gpui::px(2.))
                    .child(area.title()),
            )
            .child(
                div()
                    .id("shortcut-sheet-columns")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_row()
                    .gap(ui.space_xl)
                    .children(columns),
            );
        Some(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .p(ui.space_xl)
                .child(sheet)
                .into_any_element(),
        )
    }
}

fn render_line(line: SheetLine<'_>, ui: &UiTheme) -> AnyElement {
    match line {
        SheetLine::Title(title) => div()
            .flex()
            .items_end()
            .h(ui.sheet_row_height)
            .pb(ui.space_xs)
            .text_size(ui.small_font_size)
            .text_color(ui.text_muted)
            .child(SharedString::from(title.to_owned()))
            .into_any_element(),
        SheetLine::Row(row) => div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(ui.space_md)
            .h(ui.sheet_row_height)
            .child(truncated(row.title.clone()).grow())
            .child(keycap_glyphs(row.keys.clone(), &ui.keycap))
            .into_any_element(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(rows: usize) -> SheetGroup {
        let row = SheetRow {
            title: "x".into(),
            keys: vec![Glyph::Text("B".into())],
        };
        SheetGroup {
            title: "g".into(),
            rows: vec![row; rows],
        }
    }

    fn heights(groups: &[SheetGroup], columns: usize) -> Vec<usize> {
        flow(groups, columns).iter().map(Vec::len).collect()
    }

    #[test]
    fn groups_flow_down_even_columns() {
        let groups = [group(9), group(4), group(5), group(12), group(3)];
        // 38 lines: three columns of at most 13.
        assert_eq!(heights(&groups, 3), [13, 13, 12]);
        assert_eq!(heights(&groups[..1], 3), [4, 4, 2]);
        assert!(flow(&[], 3).iter().all(Vec::is_empty));
    }

    #[test]
    fn a_name_never_ends_a_column() {
        let groups = [group(3), group(1), group(4)];
        for column in flow(&groups, 3) {
            assert!(!matches!(column.last(), Some(SheetLine::Title(_))));
        }
    }

    #[test]
    fn tab_numbers_take_one_row() {
        assert!(earns_row("tab.go-1"));
        assert!(!earns_row("tab.go-5"));
        assert!(!earns_row("edit.undo"));
        assert!(earns_row("format.bold"));
    }
}
