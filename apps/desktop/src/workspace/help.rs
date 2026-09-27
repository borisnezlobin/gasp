//! The help button's dialog: the main commands with their shortcuts. A
//! row runs its command on click or Enter, so it doubles as a short
//! command list for the mouse.

use gpui::{
    AnyElement, App, Context, DismissEvent, ElementId, EventEmitter, FocusHandle, Focusable,
    KeyDownEvent, SharedString, Window, div, prelude::*,
};

use crate::picker::shortcut::Shortcut;
use crate::theme::UiTheme;
use crate::ui::hints::{command_title, shortcut};
use crate::ui::{keycap, ui_theme};

/// The commands the dialog lists, in groups shown apart. The first
/// [`LEFT_COLUMN_GROUPS`] groups fill the left column, the rest the right,
/// so Up and Down move down one column and then the next.
pub const HELP_COMMANDS: [(&str, &[&str]); 5] = [
    (
        "find",
        &[
            "palette.open",
            "switcher.open",
            "search.open",
            "note.new",
            "daily.open",
        ],
    ),
    (
        "tabs",
        &[
            "tab.new",
            "tab.close",
            "tab.reopen",
            "sidebar.files.toggle",
            "sidebar.right.toggle",
            "file-tree.focus",
        ],
    ),
    (
        "app",
        &["app.print", "app.export", "settings.open", "vault.open"],
    ),
    (
        "note",
        &[
            "find.open",
            "find.replace",
            "outline.jump-to-heading",
            "history.back",
            "history.forward",
        ],
    ),
    (
        "format",
        &[
            "format.bold",
            "format.italic",
            "format.link",
            "markdown.cycle-symbols",
        ],
    ),
];

/// Groups in the left column: find, tabs and app on the left, the note and
/// formatting on the right.
const LEFT_COLUMN_GROUPS: usize = 3;

/// A row was chosen: run this command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HelpEvent {
    Run(SharedString),
}

struct HelpRow {
    id: &'static str,
    title: SharedString,
    shortcut: Option<Shortcut>,
    group: usize,
}

/// The keyboard shortcuts dialog.
pub struct ShortcutsHelp {
    focus_handle: FocusHandle,
    rows: Vec<HelpRow>,
    selected: usize,
}

impl EventEmitter<DismissEvent> for ShortcutsHelp {}
impl EventEmitter<HelpEvent> for ShortcutsHelp {}

impl Focusable for ShortcutsHelp {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl ShortcutsHelp {
    /// Lists the help commands that are in `available`.
    pub fn new(available: &[&str], _: &mut Window, cx: &mut Context<Self>) -> Self {
        let rows = HELP_COMMANDS
            .iter()
            .enumerate()
            .flat_map(|(group, (_, ids))| ids.iter().map(move |id| (group, *id)))
            .filter(|(_, id)| available.contains(id))
            .map(|(group, id)| HelpRow {
                id,
                title: command_title(id),
                shortcut: shortcut(id, cx),
                group,
            })
            .collect();
        ShortcutsHelp {
            focus_handle: cx.focus_handle(),
            rows,
            selected: 0,
        }
    }

    /// The listed commands, in order.
    pub fn commands(&self) -> Vec<&'static str> {
        self.rows.iter().map(|row| row.id).collect()
    }

    pub fn run(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(row) = self.rows.get(index) else {
            return;
        };
        cx.emit(HelpEvent::Run(row.id.into()));
        cx.emit(DismissEvent);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let count = self.rows.len();
        if count == 0 {
            return;
        }
        match event.keystroke.key.as_str() {
            "up" => self.selected = (self.selected + count - 1) % count,
            "down" => self.selected = (self.selected + 1) % count,
            "enter" => self.run(self.selected, cx),
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }
}

impl ShortcutsHelp {
    fn render_row(&self, index: usize, ui: &UiTheme, cx: &mut Context<Self>) -> AnyElement {
        let row = &self.rows[index];
        let selector = format!("help-{}", row.id);
        div()
            .id(ElementId::NamedInteger("help-row".into(), index as u64))
            .debug_selector(|| selector)
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(ui.space_lg)
            .h(ui.help_row_height)
            .px(ui.row_padding_x)
            .rounded(ui.row_radius)
            .when(index == self.selected, |row| row.bg(ui.row_selected))
            .when(index != self.selected, |row| {
                row.hover(|style| style.bg(ui.row_hover))
            })
            .on_click(cx.listener(move |help, _, _, cx| help.run(index, cx)))
            .child(crate::ui::truncated(row.title.clone()).grow())
            .children(row.shortcut.map(|shortcut| keycap(shortcut, &ui.keycap)))
            .into_any_element()
    }

    /// One column: its groups' rows, with room between groups.
    fn render_column(
        &self,
        groups: std::ops::Range<usize>,
        ui: &UiTheme,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mut column = div().flex().flex_col().flex_1().min_w_0();
        let mut last_group = None;
        for (index, row) in self.rows.iter().enumerate() {
            if !groups.contains(&row.group) {
                continue;
            }
            if last_group.is_some_and(|group| group != row.group) {
                column = column.child(div().h(ui.space_lg));
            }
            last_group = Some(row.group);
            column = column.child(self.render_row(index, ui, cx));
        }
        column
    }
}

impl Render for ShortcutsHelp {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = ui_theme(cx);
        let groups = HELP_COMMANDS.len();
        crate::ui::dialog(&ui)
            .id("shortcuts-help")
            .key_context("ShortcutsHelp")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .w(ui.wide_dialog_width)
            .p(ui.dialog_padding)
            .child(
                div()
                    .px(ui.row_padding_x)
                    .pt(ui.space_md)
                    .pb(ui.space_md)
                    .text_size(ui.font_size + gpui::px(2.))
                    .child("Keyboard shortcuts"),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(ui.space_lg)
                    .child(self.render_column(0..LEFT_COLUMN_GROUPS, &ui, cx))
                    .child(self.render_column(LEFT_COLUMN_GROUPS..groups, &ui, cx)),
            )
    }
}
