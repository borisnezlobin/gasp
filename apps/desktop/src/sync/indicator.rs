//! The sync indicator in the status bar, and the popover it opens: when
//! the vault last synced, what came in and went out, what went wrong and
//! how to fix it, and a button for the one thing worth doing next.

use gpui::{
    Animation, AnimationExt, AnyElement, App, ClickEvent, Context, Corner, Entity, EventEmitter,
    FocusHandle, Focusable, KeyDownEvent, MouseButton, SharedString, Subscription, Transformation,
    Window, anchored, deferred, div, percentage, point, prelude::*, px,
};

use super::service::SyncService;
use super::state::{self, SyncPhase, SyncRun, ago, file_list};
use crate::icons::{IconName, icon};
use crate::settings_view::controls::{button, inert_button};
use crate::theme::{SettingsTheme, UiTheme};
use crate::ui::Selectable;
use crate::ui::{Tooltip, ui_theme};

/// How many files each recent sync lists before "and N more".
const FILES_SHOWN: usize = 3;
/// How many recent syncs the popover lists.
const RUNS_SHOWN: usize = 3;

/// What the indicator asks its host to open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncIndicatorEvent {
    /// The Sync page of the settings screen.
    OpenSettings,
    /// The conflict resolver.
    Resolve,
}

/// The status bar's sync indicator.
pub struct SyncIndicator {
    service: Entity<SyncService>,
    open: bool,
    focus_handle: FocusHandle,
    previous_focus: Option<FocusHandle>,
    style: SettingsTheme,
    _observe: Subscription,
}

impl EventEmitter<SyncIndicatorEvent> for SyncIndicator {}

impl Focusable for SyncIndicator {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// The icon and colour for a phase.
fn look(phase: &SyncPhase, ui: &UiTheme) -> (IconName, gpui::Hsla) {
    match phase {
        SyncPhase::Hidden | SyncPhase::Starting | SyncPhase::Synced => {
            (IconName::CloudCheck, ui.sync_quiet)
        }
        SyncPhase::Syncing(_) => (IconName::ArrowsClockwise, ui.sync_busy),
        SyncPhase::Offline { .. } => (IconName::CloudSlash, ui.sync_busy),
        SyncPhase::Setup(_) => (IconName::CloudWarning, ui.sync_busy),
        SyncPhase::SignIn { .. } => (IconName::Key, ui.icon_active),
        SyncPhase::Conflict { .. } => (IconName::GitMerge, ui.sync_attention),
        SyncPhase::Failed { .. } => (IconName::CloudWarning, ui.sync_attention),
    }
}

/// The phase's icon, turning slowly while a sync runs.
pub fn phase_icon(phase: &SyncPhase, size: gpui::Pixels, ui: &UiTheme) -> AnyElement {
    let (name, color) = look(phase, ui);
    if matches!(phase, SyncPhase::Syncing(_)) {
        return turning_icon(size, color, ui);
    }
    icon(name)
        .size(size)
        .flex_none()
        .text_color(color)
        .into_any_element()
}

/// Sync's turning arrows in `color`, for work that's under way.
pub fn turning_icon(size: gpui::Pixels, color: gpui::Hsla, ui: &UiTheme) -> AnyElement {
    icon(IconName::ArrowsClockwise)
        .size(size)
        .flex_none()
        .text_color(color)
        .with_animation(
            "sync-spin",
            Animation::new(ui.sync_spin).repeat(),
            |glyph, delta| glyph.with_transformation(Transformation::rotate(percentage(delta))),
        )
        .into_any_element()
}

/// The one action the popover offers for a phase: its label, whether it's
/// the main thing to do, and what it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    SyncNow,
    Syncing,
    Resolve,
    Settings,
}

fn action_for(phase: &SyncPhase) -> Action {
    match phase {
        SyncPhase::Syncing(_) | SyncPhase::Starting => Action::Syncing,
        SyncPhase::Conflict { .. } => Action::Resolve,
        SyncPhase::SignIn { .. } | SyncPhase::Setup(_) => Action::Settings,
        _ => Action::SyncNow,
    }
}

impl SyncIndicator {
    pub fn new(service: Entity<SyncService>, cx: &mut Context<Self>) -> Self {
        let observe = cx.observe(&service, |_, _, cx| cx.notify());
        SyncIndicator {
            service,
            open: false,
            focus_handle: cx.focus_handle(),
            previous_focus: None,
            style: super::resolver::resolved_style(cx),
            _observe: observe,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Opens or closes the popover.
    pub fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open {
            self.close(window, cx);
        } else {
            self.previous_focus = window.focused(cx);
            self.open = true;
            window.focus(&self.focus_handle);
            cx.notify();
        }
    }

    pub fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = false;
        if let Some(previous) = self.previous_focus.take() {
            window.focus(&previous);
        }
        cx.notify();
    }

    fn act(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        match action {
            Action::SyncNow => self.service.update(cx, |service, cx| service.sync_now(cx)),
            Action::Syncing => {}
            Action::Resolve => {
                self.close(window, cx);
                cx.emit(SyncIndicatorEvent::Resolve);
            }
            Action::Settings => {
                self.close(window, cx);
                cx.emit(SyncIndicatorEvent::OpenSettings);
            }
        }
    }

    fn render_button(&self, phase: &SyncPhase, ui: &UiTheme, cx: &mut Context<Self>) -> AnyElement {
        let service = self.service.clone();
        let shortcut = crate::ui::hints::shortcut("sync.now", cx);
        div()
            .id("sync-indicator-button")
            .selector(|| "sync-indicator-button".to_owned())
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
                    let service = service.read(cx);
                    let text = state::tooltip(&service.phase(), service.times());
                    Tooltip::new(text, shortcut).builder()(window, cx)
                })
            })
            .on_click(
                cx.listener(|indicator, _: &ClickEvent, window, cx| indicator.toggle(window, cx)),
            )
            .child(phase_icon(phase, ui.small_icon_size, ui))
            .into_any_element()
    }

    fn render_popover(
        &self,
        phase: &SyncPhase,
        ui: &UiTheme,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let viewport = window.viewport_size();
        let backdrop = div()
            .id("sync-popover-backdrop")
            .w(viewport.width)
            .h(viewport.height)
            .occlude()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|indicator, _, window, cx| indicator.close(window, cx)),
            );
        let backdrop =
            deferred(anchored().position(point(px(0.), px(0.))).child(backdrop)).with_priority(1);
        let panel = anchored()
            .anchor(Corner::BottomRight)
            .offset(point(px(0.), -ui.space_sm))
            .snap_to_window_with_margin(ui.space_md)
            .child(self.render_panel(phase, ui, cx));
        div()
            .absolute()
            .bottom_full()
            .right_0()
            .child(backdrop)
            .child(deferred(panel).with_priority(2))
            .into_any_element()
    }

    fn render_panel(&self, phase: &SyncPhase, ui: &UiTheme, cx: &mut Context<Self>) -> AnyElement {
        let service = self.service.read(cx);
        let headline = state::headline(phase, service.synced_ago());
        let explanation = state::explanation(phase);
        let now = service.now();
        // The headline already says when the last sync was.
        let said = (*phase == SyncPhase::Synced)
            .then(|| service.synced_ago().map(ago))
            .flatten();
        let runs: Vec<AnyElement> = service
            .recent_runs()
            .take(RUNS_SHOWN)
            .map(|run| render_run(run, now, said.as_deref(), ui))
            .collect();
        let title = div()
            .flex()
            .items_center()
            .gap(ui.space_md)
            .child(phase_icon(phase, ui.icon_size, ui))
            .child(
                div()
                    .text_size(ui.font_size)
                    .text_color(ui.text)
                    .font_weight(self.style.strong_weight)
                    .child(headline),
            );
        div()
            .id("sync-popover")
            .selector(|| "sync-popover".to_owned())
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(|indicator, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    indicator.close(window, cx);
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
            .children(explanation.map(|text| div().child(text)))
            .when(!runs.is_empty(), |panel| {
                panel.child(div().flex().flex_col().gap(ui.space_md).children(runs))
            })
            .child(self.render_footer(phase, ui, cx))
            .into_any_element()
    }

    fn render_footer(&self, phase: &SyncPhase, ui: &UiTheme, cx: &mut Context<Self>) -> AnyElement {
        let style = &self.style;
        let action = action_for(phase);
        let main = match action {
            Action::Syncing => inert_button("sync-popover-action", "Syncing…", style),
            Action::SyncNow => button("sync-popover-action", "Sync now", false, false, style),
            Action::Resolve => button("sync-popover-action", "Resolve", true, false, style),
            Action::Settings => button(
                "sync-popover-action",
                "Open sync settings",
                true,
                false,
                style,
            ),
        };
        let main = main
            .selector(|| "sync-popover-action".to_owned())
            .on_click(cx.listener(move |indicator, _: &ClickEvent, window, cx| {
                indicator.act(action, window, cx)
            }));
        let settings = (action != Action::Settings).then(|| {
            button(
                "sync-popover-settings",
                "Sync settings",
                false,
                false,
                style,
            )
            .selector(|| "sync-popover-settings".to_owned())
            .on_click(cx.listener(|indicator, _: &ClickEvent, window, cx| {
                indicator.act(Action::Settings, window, cx)
            }))
        });
        div()
            .flex()
            .justify_end()
            .gap(ui.space_md)
            .text_size(style.small_text_size)
            .children(settings)
            .child(main)
            .into_any_element()
    }
}

/// One recent sync: when (unless that's `said` already), then what came
/// in and what went out.
fn render_run(
    run: &SyncRun,
    now: std::time::Duration,
    said: Option<&str>,
    ui: &UiTheme,
) -> AnyElement {
    let when = ago(now.saturating_sub(run.finished_at.unwrap_or(run.started_at)));
    let caption = (said != Some(when.as_str()))
        .then(|| div().text_color(ui.text_faint).child(capitalize(&when)));
    let line = |verb: &str, paths: &[std::path::PathBuf]| -> Option<AnyElement> {
        if paths.is_empty() {
            return None;
        }
        let (names, more) = file_list(paths, FILES_SHOWN);
        let text = sentence(verb, &names, more);
        Some(div().text_color(ui.text).child(text).into_any_element())
    };
    div()
        .flex()
        .flex_col()
        .gap(ui.space_xs)
        .children(caption)
        .children(line("Received", &run.received))
        .children(line("Sent", &run.sent))
        .into_any_element()
}

/// "Received Wave Packets, Notes and 2 more."
pub fn sentence(verb: &str, names: &[String], more: usize) -> SharedString {
    let mut parts: Vec<String> = names.to_vec();
    if more > 0 {
        parts.push(format!("{more} more"));
    }
    let list = match parts.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        Some((last, _)) => last.clone(),
        None => String::new(),
    };
    format!("{verb} {list}.").into()
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

impl Render for SyncIndicator {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The theme can change while the popover is open.
        self.style = super::resolver::resolved_style(cx);
        let phase = self.service.read(cx).phase();
        let root = div().id("sync-indicator").relative().flex_none();
        if matches!(phase, SyncPhase::Hidden | SyncPhase::Starting) {
            return root;
        }
        let ui = ui_theme(cx);
        let button = self.render_button(&phase, &ui, cx);
        let popover = self
            .open
            .then(|| self.render_popover(&phase, &ui, window, cx));
        root.child(button).children(popover)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sentences_list_names_plainly() {
        let names = |list: &[&str]| list.iter().map(|name| name.to_string()).collect::<Vec<_>>();
        assert_eq!(sentence("Sent", &names(&["A"]), 0), "Sent A.");
        assert_eq!(sentence("Sent", &names(&["A", "B"]), 0), "Sent A and B.");
        assert_eq!(
            sentence("Received", &names(&["A", "B", "C"]), 4),
            "Received A, B, C and 4 more."
        );
    }

    #[test]
    fn each_phase_offers_the_one_useful_action() {
        assert_eq!(action_for(&SyncPhase::Synced), Action::SyncNow);
        assert_eq!(
            action_for(&SyncPhase::Conflict { files: 1 }),
            Action::Resolve
        );
        assert_eq!(
            action_for(&SyncPhase::SignIn { has_token: false }),
            Action::Settings
        );
        assert_eq!(
            action_for(&SyncPhase::Syncing(gasp_sync::SyncStep::Push)),
            Action::Syncing
        );
    }
}
