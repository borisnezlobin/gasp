//! The Dock icon row's tiles: each whale as the Dock would show it in the
//! system's current appearance, with its name under it. The chosen tile
//! has a ring and a check on its icon; both are drawn over the layout, so
//! choosing moves nothing.

use gasp_config::settings::AppIconChoice;
use gpui::{AnyElement, ClickEvent, Context, Div, SharedString, Stateful, div, img, prelude::*};

use super::model::{SettingItem, choice_label};
use super::view::SettingsView;
use crate::dock_icon::{IconArtwork, preview};
use crate::icons::{IconName, icon};
use crate::theme::SettingsTheme;
use crate::ui::Selectable;

impl SettingsView {
    /// A tile for each app icon, side by side, ringed together while the
    /// row has keyboard focus.
    pub(super) fn app_icon_tiles(
        &self,
        item: &SettingItem,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let current = self.current_value(item);
        let dark = crate::ui::system_dark(cx);
        let tiles: Vec<AnyElement> = AppIconChoice::ALL
            .into_iter()
            .map(|choice| {
                let chosen = current.as_str() == Some(choice.id());
                self.app_icon_tile(item, IconArtwork { choice, dark }, chosen, cx)
            })
            .collect();
        let style = &self.style;
        div()
            .selector(|| "app-icon-tiles".to_string())
            .flex()
            .gap(style.gap_sm)
            .rounded(tile_radius(style))
            .when(focused, |group| group.shadow(vec![style.focus()]))
            .children(tiles)
            .into_any_element()
    }

    fn app_icon_tile(
        &self,
        item: &SettingItem,
        artwork: IconArtwork,
        chosen: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let style = &self.style;
        let id = artwork.choice.id();
        let picture = img(preview(artwork, cx)).size(style.app_icon_choice_size);
        let item = item.clone();
        tile_surface(id, chosen, style)
            .child(
                div()
                    .relative()
                    .child(picture)
                    .children(chosen.then(|| chosen_badge(style))),
            )
            .child(
                div()
                    .text_size(style.small_text_size)
                    .text_color(if chosen { style.text } else { style.text_muted })
                    .child(choice_label(id)),
            )
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                view.choose(&item, id, cx);
            }))
            .into_any_element()
    }
}

fn tile_radius(style: &SettingsTheme) -> gpui::Pixels {
    style.radius + style.gap_sm
}

/// The tile's shape: ringed in the text colour once chosen (on the card's
/// fill, since a shadow fills whatever is see-through above it), filled
/// under the pointer while it can still be chosen.
fn tile_surface(id: &'static str, chosen: bool, style: &SettingsTheme) -> Stateful<Div> {
    let tile = div()
        .id(SharedString::from(format!("app-icon-{id}")))
        .selector(move || format!("app-icon-{id}"))
        .flex_none()
        .flex()
        .flex_col()
        .items_center()
        .gap(style.gap_xs)
        .p(style.gap_sm)
        .rounded(tile_radius(style));
    if chosen {
        return tile
            .bg(style.card_background)
            .shadow(vec![style.ring(style.text)]);
    }
    let (hover, pressed) = (style.hover_fill, style.pressed);
    tile.cursor_pointer()
        .hover(move |tile| tile.bg(hover))
        .active(move |tile| tile.bg(pressed))
}

/// The check on the chosen icon's corner.
fn chosen_badge(style: &SettingsTheme) -> Div {
    let offset = style.gap_xs;
    div()
        .absolute()
        .right(offset)
        .bottom(offset)
        .size(style.app_badge_size)
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(style.text)
        .shadow(vec![style.ring(style.card_background)])
        .child(
            icon(IconName::Check)
                .size(style.app_badge_size * 0.7)
                .text_color(crate::styling::ink_on(style.text)),
        )
}
