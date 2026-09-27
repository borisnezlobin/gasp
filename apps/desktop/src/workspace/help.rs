//! The help button's dialog: the main commands with their shortcuts. A
//! row runs its command on click or Enter, so it doubles as a short
//! command list for the mouse.

use gpui::{
    App, Context, DismissEvent, ElementId, EventEmitter, FocusHandle, Focusable, KeyDownEvent,
    SharedString, Window, div, prelude::*,
};

use crate::ui::hints::{command_title, shortcut};
use crate::ui::ui_theme;

/// The commands the dialog lists, in groups shown apart.
pub const HELP_COMMANDS: [(&str, &[&str]); 5] = [
    (
        "find",
        &["palette.open", "switcher.open", "search.open", "note.new"],
    ),
    (
        "tabs",
        &[
            "tab.new",
            "tab.close",
            "tab.reopen",
            "sidebar.files.toggle",
            "file-tree.focus",
        ],
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
    (
        "app",
        &["app.print", "app.export", "settings.open", "vault.open"],
    ),
];

/// A row was chosen: run this command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HelpEvent {
    Run(SharedString),
}

struct HelpRow {
    id: &'static str,
    title: SharedString,
    shortcut: Option<SharedString>,
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

impl Render for ShortcutsHelp {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = ui_theme(cx);
        let mut list = div().flex().flex_col();
        let mut last_group = None;
        for (index, row) in self.rows.iter().enumerate() {
            if last_group.is_some_and(|group| group != row.group) {
                list = list.child(div().h(ui.space_md));
            }
            last_group = Some(row.group);
            let keycap = row.shortcut.clone().map(|shortcut| {
                div()
                    .px(ui.keycap_padding_x)
                    .rounded(ui.keycap_radius)
                    .bg(ui.keycap_background)
                    .text_size(ui.small_font_size)
                    .text_color(ui.text_muted)
                    .child(shortcut)
            });
            let selector = format!("help-{}", row.id);
            list = list.child(
                div()
                    .id(ElementId::NamedInteger("help-row".into(), index as u64))
                    .debug_selector(|| selector)
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .h(ui.help_row_height)
                    .px(ui.space_md)
                    .rounded(ui.menu_row_radius)
                    .when(index == self.selected, |row| row.bg(ui.menu_highlight))
                    .hover(|style| style.bg(ui.control_hover))
                    .on_click(cx.listener(move |help, _, _, cx| help.run(index, cx)))
                    .child(row.title.clone())
                    .children(keycap),
            );
        }
        div()
            .id("shortcuts-help")
            .key_context("ShortcutsHelp")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .w_full()
            .p(ui.menu_padding * 2.)
            .rounded(ui.menu_radius)
            .bg(ui.menu_background)
            .font_family(ui.font_family.clone())
            .text_size(ui.font_size)
            .text_color(ui.text)
            .child(list)
    }
}
