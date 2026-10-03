//! Drawing the AI app tiles: each app's own icon and name, and one short
//! line saying where it's got to. A tile that connects its app is raised
//! like a button; a connected one lies flat with a check on its icon.
//! Each tile is as wide as the widest line it can show, so a change of
//! state moves nothing.

use std::time::Duration;

use gasp_mcp::clients::ClientApp;
use gpui::{
    Animation, AnimationExt, AnyElement, App, ClickEvent, Context, Div, Hsla, Stateful,
    Transformation, div, img, percentage, prelude::*,
};

use super::agent_apps::{AGENT_APPS_LINE, AgentAppRow, TileState};
use super::controls::{Fills, widest_element_of};
use super::view::{ControlRow, SettingsView};
use crate::app_icons::app_icon;
use crate::icons::{IconName, icon};
use crate::theme::SettingsTheme;
use crate::ui::{Selectable, Tooltip};

impl SettingsView {
    /// The line said once for all the apps, then their tiles, with the
    /// last connect's error hung under them.
    pub(super) fn render_agent_apps(&self, focused: bool, cx: &mut Context<Self>) -> AnyElement {
        let style = &self.style;
        let rows = self.agent_app_list().to_vec();
        let focused_tile = focused.then(|| self.focused_agent_app());
        let tiles: Vec<AnyElement> = rows
            .iter()
            .enumerate()
            .map(|(index, row)| self.agent_app_tile(index, row, focused_tile == Some(index), cx))
            .collect();
        let strip = div()
            .selector(|| "agent-apps".to_string())
            .flex()
            .flex_wrap()
            .gap(style.control_gap)
            .children(tiles)
            .into_any_element();
        div()
            .w_full()
            .flex()
            .flex_col()
            .items_start()
            .gap(style.control_gap)
            .child(
                div()
                    .text_size(style.small_text_size)
                    .text_color(style.text_muted)
                    .child(AGENT_APPS_LINE),
            )
            .child(self.with_error_note(&ControlRow::AgentApps, strip))
            .into_any_element()
    }

    fn agent_app_tile(
        &self,
        index: usize,
        row: &AgentAppRow,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let style = &self.style;
        let app = row.app;
        let state = row.state(cx);
        let spin = crate::ui::ui_theme(cx).sync_spin;
        let label = self.agent_app_label(app, cx).unwrap_or_default();
        let tile = tile_surface(index, state, style)
            .selector(move || format!("agent-app-{}", app.id()))
            .tooltip(Tooltip::new(label, None).builder())
            .child(tile_logo(row, state, style, cx))
            .child(tile_text(app, state, spin, style));
        let tile = match focused {
            true => tile.shadow(vec![style.focus()]),
            false => tile,
        };
        tile.on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
            view.focus_agent_app(index, cx);
            view.press_agent_app(app, cx);
        }))
        .into_any_element()
    }
}

/// The tile's shape and fill: raised like a button while pressing it
/// connects the app, flat once it's connected or busy, and filled under
/// the pointer when pressing it explains something.
fn tile_surface(index: usize, state: TileState, style: &SettingsTheme) -> Stateful<Div> {
    let tile = div()
        .id(("agent-app", index))
        .flex_none()
        .flex()
        .items_center()
        .gap(style.control_gap)
        .p(style.gap_sm * 1.5)
        .pr(style.control_padding_x)
        .rounded(style.radius + style.gap_sm * 1.5);
    if state.connects() {
        let fills = Fills::over(style.control_background, style);
        return tile
            .bg(fills.rest)
            .shadow(vec![style.outline(), style.lift()])
            .cursor_pointer()
            .hover(move |tile| tile.bg(fills.hover))
            .active(move |tile| tile.bg(fills.pressed));
    }
    if state == TileState::Unreadable {
        let (hover, pressed) = (style.hover_fill, style.pressed);
        return tile
            .cursor_pointer()
            .hover(move |tile| tile.bg(hover))
            .active(move |tile| tile.bg(pressed));
    }
    tile
}

/// The app's icon, with a check on its corner once connected or a
/// warning when its file can't be read.
fn tile_logo(row: &AgentAppRow, state: TileState, style: &SettingsTheme, cx: &App) -> Div {
    div()
        .relative()
        .flex_none()
        .size(style.app_icon_size)
        .child(logo_picture(row, style, cx))
        .children(logo_badge(state, style))
}

/// The icon read from the app's bundle; nothing while it's being read;
/// or, for an app with no bundle or icon, a stand-in glyph.
fn logo_picture(row: &AgentAppRow, style: &SettingsTheme, cx: &App) -> AnyElement {
    let Some(bundle) = row.icon_bundle.as_deref() else {
        return stand_in_logo(row.app, style);
    };
    match app_icon(bundle, cx) {
        None => div().size(style.app_icon_size).into_any_element(),
        Some(Some(image)) => img(image).size(style.app_icon_size).into_any_element(),
        Some(None) => stand_in_logo(row.app, style),
    }
}

/// A terminal for the apps that are only a command, or a window for the
/// others, on a quiet square the size of an app icon.
fn stand_in_logo(app: ClientApp, style: &SettingsTheme) -> AnyElement {
    let glyph = match app {
        ClientApp::ClaudeCode | ClientApp::Codex => IconName::TerminalWindow,
        ClientApp::ClaudeDesktop | ClientApp::Cursor => IconName::AppWindow,
    };
    div()
        .size(style.app_icon_size)
        .flex()
        .items_center()
        .justify_center()
        .rounded(style.radius)
        .bg(style.hover)
        .child(
            icon(glyph)
                .size(style.icon_size)
                .text_color(style.text_muted),
        )
        .into_any_element()
}

fn logo_badge(state: TileState, style: &SettingsTheme) -> Option<Div> {
    let (glyph, fill) = match state {
        TileState::Unreadable => (IconName::Warning, style.warning),
        _ if state.is_connected() => (IconName::Check, style.connected),
        _ => return None,
    };
    let offset = -style.gap_xs;
    Some(
        div()
            .absolute()
            .right(offset)
            .bottom(offset)
            .size(style.app_badge_size)
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(fill)
            .shadow(vec![style.ring(style.card_background)])
            .child(
                icon(glyph)
                    .size(style.app_badge_size * 0.7)
                    .text_color(crate::styling::ink_on(fill)),
            ),
    )
}

/// The app's name over its status line. The line is laid out as wide as
/// the widest status the tile can show.
fn tile_text(app: ClientApp, state: TileState, spin: Duration, style: &SettingsTheme) -> Div {
    let marker = state_marker(app, state);
    let shown = status_line(state, Some(spin), style)
        .when_some(marker, |line, marker| line.selector(move || marker));
    let alternatives = TileState::all_for(app)
        .into_iter()
        .map(|state| status_line(state, None, style).into_any_element());
    div()
        .flex()
        .flex_col()
        .child(div().whitespace_nowrap().child(app.name()))
        .child(widest_element_of(shown, alternatives))
}

/// The debug selector tests find a tile's state by: `connect-<app>` while
/// pressing it connects, `connected-<app>` once it's connected.
fn state_marker(app: ClientApp, state: TileState) -> Option<String> {
    if state.connects() {
        return Some(format!("connect-{}", app.id()));
    }
    state
        .is_connected()
        .then(|| format!("connected-{}", app.id()))
}

/// One state's short line, with a turning mark while connecting. `spin`
/// is how long a turn takes; the sizing copies pass `None` and don't
/// turn.
fn status_line(state: TileState, spin: Option<Duration>, style: &SettingsTheme) -> Div {
    let spinner = (state == TileState::Connecting).then(|| busy_mark(spin, style));
    div()
        .flex()
        .items_center()
        .gap(style.gap_sm)
        .whitespace_nowrap()
        .text_size(style.small_text_size)
        .text_color(status_color(state, style))
        .children(spinner)
        .child(state.status())
}

fn status_color(state: TileState, style: &SettingsTheme) -> Hsla {
    match state {
        TileState::NotConnected | TileState::Stale => style.accent,
        TileState::Unreadable => style.warning,
        _ => style.text_muted,
    }
}

fn busy_mark(spin: Option<Duration>, style: &SettingsTheme) -> AnyElement {
    let mark = icon(IconName::CircleNotch)
        .flex_none()
        .size(style.small_icon_size)
        .text_color(style.text_muted);
    let Some(spin) = spin else {
        return mark.into_any_element();
    };
    mark.with_animation(
        "agent-app-spin",
        Animation::new(spin).repeat(),
        |mark, delta| mark.with_transformation(Transformation::rotate(percentage(delta))),
    )
    .into_any_element()
}
