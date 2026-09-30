//! The status bar's sync widget for a vault in iCloud Drive, where git's
//! indicator has nothing to say. iCloud syncs on its own, so the widget
//! only speaks up when something needs a look: files iCloud is still
//! downloading, and copies it left beside notes two devices changed at
//! once. Its popover says where the vault is and lists those copies, each
//! with a way to open it beside its note to compare.

use std::path::{Path, PathBuf};
use std::time::Duration;

use gasp_sync::icloud::{ICloudCopy, icloud_copies};
use gpui::{
    AnyElement, App, ClickEvent, Context, Corner, EventEmitter, FocusHandle, Focusable,
    KeyDownEvent, MouseButton, Task, Window, anchored, deferred, div, point, prelude::*, px,
};

use super::icloud::shown_location;
use crate::icons::{IconName, icon};
use crate::settings_view::controls::button;
use crate::theme::{SettingsTheme, UiTheme};
use crate::ui::{Selectable, Tooltip, ui_theme};

/// How often the vault is looked over for downloads and copies.
const LOOK_EVERY: Duration = Duration::from_secs(20);

/// What iCloud is doing with the vault, as the widget shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ICloudState {
    /// Files iCloud hasn't downloaded yet.
    pub downloading: usize,
    /// Copies iCloud left beside notes.
    pub copies: Vec<ICloudCopy>,
}

impl ICloudState {
    /// Looks the vault at `root` over, asking iCloud for what's missing.
    pub fn of(root: &Path) -> Self {
        ICloudState {
            downloading: super::icloud::request_downloads(root),
            copies: icloud_copies(root),
        }
    }

    /// What the widget's tooltip and the popover's title say.
    pub fn headline(&self) -> String {
        match (self.copies.len(), self.downloading) {
            (0, 0) => "In iCloud".to_owned(),
            (0, files) => format!(
                "Downloading {} from iCloud",
                super::state::count(files, "file")
            ),
            (1, _) => "iCloud kept two versions of a note".to_owned(),
            (notes, _) => format!("iCloud kept two versions of {notes} notes"),
        }
    }

    fn look(&self, ui: &UiTheme) -> (IconName, gpui::Hsla) {
        if !self.copies.is_empty() {
            return (IconName::CloudWarning, ui.sync_attention);
        }
        if self.downloading > 0 {
            return (IconName::CloudArrowDown, ui.sync_busy);
        }
        (IconName::CloudCheck, ui.sync_quiet)
    }
}

/// What the widget asks its host to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ICloudStatusEvent {
    /// Open a note and the copy iCloud left of it side by side.
    Compare { original: PathBuf, copy: PathBuf },
}

/// The widget, for one window's vault.
pub struct ICloudStatus {
    root: PathBuf,
    state: ICloudState,
    open: bool,
    focus_handle: FocusHandle,
    previous_focus: Option<FocusHandle>,
    style: SettingsTheme,
    _looking: Task<()>,
}

impl EventEmitter<ICloudStatusEvent> for ICloudStatus {}

impl Focusable for ICloudStatus {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl ICloudStatus {
    pub fn new(root: PathBuf, cx: &mut Context<Self>) -> Self {
        let looking = cx.spawn(async move |status, cx| {
            loop {
                let root = status.read_with(cx, |status, _| status.root.clone());
                let Ok(root) = root else {
                    return;
                };
                let state = cx
                    .background_spawn(async move { ICloudState::of(&root) })
                    .await;
                if status
                    .update(cx, |status, cx| status.show(state, cx))
                    .is_err()
                {
                    return;
                }
                cx.background_executor().timer(LOOK_EVERY).await;
            }
        });
        ICloudStatus {
            root,
            state: ICloudState::default(),
            open: false,
            focus_handle: cx.focus_handle(),
            previous_focus: None,
            style: crate::ui::settings_theme(cx),
            _looking: looking,
        }
    }

    pub fn state(&self) -> &ICloudState {
        &self.state
    }

    fn show(&mut self, state: ICloudState, cx: &mut Context<Self>) {
        if state != self.state {
            self.state = state;
            cx.notify();
        }
    }

    pub fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open {
            return self.close(window, cx);
        }
        self.previous_focus = window.focused(cx);
        self.open = true;
        self.state = ICloudState::of(&self.root);
        window.focus(&self.focus_handle);
        cx.notify();
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = false;
        if let Some(previous) = self.previous_focus.take() {
            window.focus(&previous);
        }
        cx.notify();
    }

    fn compare(&mut self, copy: ICloudCopy, window: &mut Window, cx: &mut Context<Self>) {
        self.close(window, cx);
        cx.emit(ICloudStatusEvent::Compare {
            original: self.root.join(copy.original),
            copy: self.root.join(copy.copy),
        });
    }

    fn render_button(&self, ui: &UiTheme, cx: &mut Context<Self>) -> AnyElement {
        let (name, color) = self.state.look(ui);
        let headline = self.state.headline();
        div()
            .id("icloud-status-button")
            .selector(|| "icloud-status-button".to_owned())
            .flex()
            .items_center()
            .justify_center()
            .size(ui.small_icon_size + ui.space_md)
            .rounded(ui.icon_button_radius)
            .cursor_pointer()
            .hover(|style| style.bg(ui.control_hover))
            .active(|style| style.bg(ui.control_pressed))
            .when(self.open, |button| button.bg(ui.control_active))
            .when(!self.open, |button| {
                button.tooltip(move |window, cx| {
                    Tooltip::new(headline.clone(), None).builder()(window, cx)
                })
            })
            .on_click(cx.listener(|status, _: &ClickEvent, window, cx| status.toggle(window, cx)))
            .child(icon(name).size(ui.small_icon_size).text_color(color))
            .into_any_element()
    }

    fn render_popover(&self, ui: &UiTheme, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let viewport = window.viewport_size();
        let backdrop = div()
            .id("icloud-popover-backdrop")
            .w(viewport.width)
            .h(viewport.height)
            .occlude()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|status, _, window, cx| status.close(window, cx)),
            );
        let backdrop =
            deferred(anchored().position(point(px(0.), px(0.))).child(backdrop)).with_priority(1);
        let panel = anchored()
            .anchor(Corner::BottomRight)
            .offset(point(px(0.), -ui.space_sm))
            .snap_to_window_with_margin(ui.space_md)
            .child(self.render_panel(ui, cx));
        div()
            .absolute()
            .bottom_full()
            .right_0()
            .child(backdrop)
            .child(deferred(panel).with_priority(2))
            .into_any_element()
    }

    fn render_panel(&self, ui: &UiTheme, cx: &mut Context<Self>) -> AnyElement {
        let (name, color) = self.state.look(ui);
        let title = div()
            .flex()
            .items_center()
            .gap(ui.space_md)
            .child(icon(name).size(ui.icon_size).text_color(color))
            .child(
                div()
                    .text_size(ui.font_size)
                    .text_color(ui.text)
                    .font_weight(self.style.strong_weight)
                    .child(self.state.headline()),
            );
        let copies: Vec<AnyElement> = self
            .state
            .copies
            .iter()
            .enumerate()
            .map(|(index, copy)| self.render_copy(index, copy, ui, cx))
            .collect();
        div()
            .id("icloud-popover")
            .selector(|| "icloud-popover".to_owned())
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(|status, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    status.close(window, cx);
                    cx.stop_propagation();
                }
            }))
            .occlude()
            .w(ui.popover_width)
            .flex()
            .flex_col()
            .gap(ui.space_lg)
            .p(ui.popover_padding)
            .rounded(ui.menu_radius)
            .bg(ui.menu_background)
            .shadow(ui.menu_shadows())
            .font_family(ui.font_family.clone())
            .text_size(ui.small_font_size)
            .text_color(ui.text_muted)
            .child(title)
            .child(div().child(self.explanation()))
            .when(!copies.is_empty(), |panel| {
                panel.child(div().flex().flex_col().gap(ui.space_sm).children(copies))
            })
            .child(self.render_footer(ui))
            .into_any_element()
    }

    fn explanation(&self) -> String {
        let location = shown_location(&self.root);
        if self.state.copies.is_empty() {
            return format!(
                "The vault is in {location}. iCloud keeps it the same on your iPhone and other Macs."
            );
        }
        "When two devices change a note before iCloud catches up, iCloud keeps the other version as a copy beside it. Compare the two, keep what you want, then delete the copy.".to_owned()
    }

    fn render_copy(
        &self,
        index: usize,
        copy: &ICloudCopy,
        ui: &UiTheme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let title = crate::workspace::files::note_title(&copy.original);
        let copy_name = copy
            .copy
            .file_stem()
            .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
        let compared = copy.clone();
        let compare = button(
            ("icloud-compare", index),
            "Compare",
            false,
            false,
            &self.style,
        )
        .selector(move || format!("icloud-compare-{index}"))
        .on_click(cx.listener(move |status, _: &ClickEvent, window, cx| {
            status.compare(compared.clone(), window, cx)
        }));
        div()
            .flex()
            .items_center()
            .gap(ui.space_md)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(div().text_color(ui.text).truncate().child(title))
                    .child(div().truncate().child(format!("and {copy_name}"))),
            )
            .child(compare)
            .into_any_element()
    }

    fn render_footer(&self, ui: &UiTheme) -> AnyElement {
        let root = self.root.clone();
        let reveal = button("icloud-reveal", "Show in Finder", false, false, &self.style)
            .selector(|| "icloud-reveal".to_owned())
            .on_click(move |_: &ClickEvent, _, cx| crate::sandbox::reveal_path(&root, cx));
        div()
            .flex()
            .justify_end()
            .gap(ui.space_md)
            .child(reveal)
            .into_any_element()
    }
}

impl Render for ICloudStatus {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.style = super::resolver::resolved_style(cx);
        let ui = ui_theme(cx);
        let button = self.render_button(&ui, cx);
        let popover = self.open.then(|| self.render_popover(&ui, window, cx));
        div()
            .id("icloud-status")
            .relative()
            .flex_none()
            .child(button)
            .children(popover)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_headline_names_what_needs_a_look() {
        let mut state = ICloudState::default();
        assert_eq!(state.headline(), "In iCloud");
        state.downloading = 3;
        assert_eq!(state.headline(), "Downloading 3 files from iCloud");
        state.copies.push(ICloudCopy {
            original: "Plan.md".into(),
            copy: "Plan 2.md".into(),
        });
        assert_eq!(state.headline(), "iCloud kept two versions of a note");
    }
}
