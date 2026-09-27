//! Drawing the right sidebar: a header with a button for each view and
//! the hide button, then the view's rows in a virtualised list, so a note
//! with hundreds of backlinks lays out only what's on screen.

use gpui::{
    AnyElement, Context, Div, FontWeight, HighlightStyle, MouseButton, Render, SharedString,
    StyledText, Window, div, list, prelude::*,
};

use super::sidebar::{KnowledgeSidebar, Row, SidebarEvent, SidebarView};
use crate::icons::{IconName, icon};
use crate::theme::UiTheme;
use crate::ui::{Button, IconButton, Tooltip, truncated, ui_theme};

impl Render for KnowledgeSidebar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = ui_theme(cx);
        let rows = list(
            self.list.clone(),
            cx.processor(|sidebar, index, window, cx| sidebar.render_row(index, window, cx)),
        )
        .size_full();
        div()
            .id("knowledge-sidebar")
            .key_context("KnowledgeSidebar")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .flex()
            .flex_col()
            .size_full()
            .font_family(ui.font_family.clone())
            .text_size(ui.font_size)
            .text_color(ui.text)
            .child(self.render_header(&ui, cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .px(ui.sidebar_padding - ui.space_sm)
                    .pb(ui.space_sm)
                    .child(rows),
            )
    }
}

impl KnowledgeSidebar {
    fn render_header(&self, ui: &UiTheme, cx: &mut Context<Self>) -> impl IntoElement {
        let views = SidebarView::ALL.map(|view| {
            IconButton::new(format!("knowledge-{}", view.key()), view.icon())
                .command(view.command(), cx)
                .label(view.title())
                .active(view == self.view())
                .on_click(cx.listener(move |_, _, _, cx| cx.emit(SidebarEvent::Show(view))))
        });
        let hide = IconButton::new("knowledge-hide", IconName::SidebarSimpleRight)
            .command("sidebar.right.toggle", cx)
            .label("Hide right sidebar")
            .on_click(cx.listener(|_, _, _, cx| cx.emit(SidebarEvent::Hide)));
        div()
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .justify_between()
            .h(ui.tab_bar_height)
            .px(ui.sidebar_padding)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(ui.space_xs)
                    .children(views),
            )
            .child(hide)
    }

    fn render_row(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ui = ui_theme(cx);
        let current = self.selected == Some(index) && self.focus_handle.is_focused(window);
        let ringed = crate::ui::focus_visible::ring(current, cx);
        let Some(row) = self.rows().get(index).cloned() else {
            return div().into_any_element();
        };
        let content = match row {
            Row::Summary(text) => summary(text, &ui),
            Row::Message(text) => message(text, &ui),
            Row::Source { title, folder, .. } => source(title, folder, &ui)
                .on_click(cx.listener(move |sidebar, _, _, cx| sidebar.activate(index, cx))),
            Row::Context { .. } => self.context_row(index, row, &ui, cx),
            Row::UnlinkedToggle { open, count } => unlinked_toggle(open, count, &ui)
                .on_click(cx.listener(|sidebar, _, _, cx| sidebar.toggle_unlinked(cx))),
            Row::Outgoing { .. } => outgoing(index, row, &ui, cx),
            Row::Heading { .. } => heading(index, row, &ui, cx),
            Row::Tag { .. } => tag(index, row, &ui, cx),
        };
        // The row the keys act on: a fill whichever way the sidebar got
        // the keyboard, and the ring while the keyboard is driving.
        let content = content
            .when(current, |row| {
                row.bg(crate::theme::over(
                    ui.tree_active_background,
                    ui.app_background,
                ))
            })
            .when(ringed, |row| row.shadow(vec![ui.focus()]));
        // Room around the row so the list's clipping doesn't cut it.
        div()
            .id(("knowledge-row", index))
            .debug_selector(move || format!("knowledge-row-{index}"))
            .w_full()
            .px(ui.space_xs)
            .py(ui.hairline)
            .child(content)
            .into_any_element()
    }

    fn context_row(
        &self,
        index: usize,
        row: Row,
        ui: &UiTheme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let Row::Context {
            excerpt, mention, ..
        } = row
        else {
            return div().id("empty");
        };
        let highlight = if mention.is_some() {
            HighlightStyle {
                background_color: Some(ui.match_background),
                color: Some(ui.text),
                ..HighlightStyle::default()
            }
        } else {
            HighlightStyle {
                color: Some(ui.text),
                font_weight: Some(FontWeight::MEDIUM),
                ..HighlightStyle::default()
            }
        };
        let text = StyledText::new(SharedString::from(excerpt.text.clone()))
            .with_highlights([(excerpt.highlight.clone(), highlight)]);
        let link = mention.map(|_| {
            Button::new(("knowledge-link", index), "Link")
                .quiet()
                .on_click(cx.listener(move |sidebar, _, _, cx| sidebar.link_mention(index, cx)))
        });
        // The button floats over the row's right end, with room kept for
        // it: text beside a flex sibling wraps at the wrong width.
        let has_link = link.is_some();
        div()
            .id(("knowledge-context", index))
            .relative()
            .w_full()
            .pl(ui.space_md + ui.small_icon_size + ui.tree_row_gap)
            .pr(ui.space_sm)
            .when(has_link, |row| row.pr(ui.inline_button_width))
            .py(ui.space_xs)
            .rounded(ui.tree_row_radius)
            .text_size(ui.small_font_size)
            .text_color(ui.text_muted)
            .cursor_pointer()
            .hover(|style| style.bg(ui.tree_hover_background))
            .on_click(cx.listener(move |sidebar, _, _, cx| sidebar.activate(index, cx)))
            .child(div().w_full().child(text))
            .children(link.map(|link| {
                div()
                    .absolute()
                    .top(ui.hairline)
                    .right(ui.space_xs)
                    .child(link)
            }))
    }
}

/// The shape every clickable one-line row shares.
fn row_shell(id: impl Into<gpui::ElementId>, depth: usize, ui: &UiTheme) -> gpui::Stateful<Div> {
    let hover = ui.tree_hover_background;
    still_row(id, depth, ui)
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
}

/// A one-line row that does nothing when clicked.
fn still_row(id: impl Into<gpui::ElementId>, depth: usize, ui: &UiTheme) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_row()
        .items_center()
        .gap(ui.tree_row_gap)
        .w_full()
        .h(ui.tree_row_height)
        .pl(ui.space_md + ui.tree_indent * depth as f32)
        .pr(ui.space_md)
        .rounded(ui.tree_row_radius)
}

fn summary(text: SharedString, ui: &UiTheme) -> gpui::Stateful<Div> {
    div()
        .id("knowledge-summary")
        .px(ui.space_md)
        .pt(ui.space_sm)
        .pb(ui.space_xs)
        .text_size(ui.small_font_size)
        .text_color(ui.text_muted)
        .child(text)
}

fn message(text: SharedString, ui: &UiTheme) -> gpui::Stateful<Div> {
    div()
        .id("knowledge-message")
        .px(ui.space_md)
        .py(ui.space_sm)
        .text_size(ui.small_font_size)
        .text_color(ui.text_muted)
        .child(text)
}

fn row_icon(name: IconName, ui: &UiTheme) -> impl IntoElement {
    icon(name)
        .flex_none()
        .size(ui.small_icon_size)
        .text_color(ui.icon)
}

fn source(title: SharedString, folder: SharedString, ui: &UiTheme) -> gpui::Stateful<Div> {
    let tooltip = if folder.is_empty() {
        title.clone()
    } else {
        format!("{folder}/{title}").into()
    };
    row_shell(
        SharedString::from(format!("knowledge-note-{tooltip}")),
        0,
        ui,
    )
    .mt(ui.space_xs)
    .tooltip(Tooltip::new(tooltip, None).builder())
    .child(row_icon(IconName::FileText, ui))
    .child(
        div()
            .flex()
            .flex_none()
            .max_w(gpui::relative(0.75))
            .child(truncated(title).grow()),
    )
    .child(
        div()
            .flex()
            .flex_1()
            .min_w_0()
            .text_size(ui.small_font_size)
            .text_color(ui.text_faint)
            .child(truncated(folder).grow()),
    )
}

fn unlinked_toggle(open: bool, count: Option<usize>, ui: &UiTheme) -> gpui::Stateful<Div> {
    let caret = if open {
        IconName::CaretDown
    } else {
        IconName::CaretRight
    };
    row_shell("knowledge-unlinked", 0, ui)
        .mt(ui.space_md)
        .text_color(ui.text_muted)
        .child(row_icon(caret, ui))
        .child(div().flex_1().child("Unlinked mentions"))
        .children(count.map(|count| {
            div()
                .flex_none()
                .text_size(ui.small_font_size)
                .text_color(ui.text_faint)
                .child(count.to_string())
        }))
}

fn outgoing(
    index: usize,
    row: Row,
    ui: &UiTheme,
    cx: &mut Context<KnowledgeSidebar>,
) -> gpui::Stateful<Div> {
    let Row::Outgoing {
        label,
        target,
        detail,
        exists,
        is_note,
    } = row
    else {
        return div().id("empty");
    };
    let icon_name = match (exists, is_note) {
        (false, true) => IconName::FilePlus,
        (true, true) => IconName::FileText,
        (_, false) if is_image(&target) => IconName::Image,
        (_, false) => IconName::File,
    };
    let tooltip = match (exists, is_note) {
        (true, _) => format!("Open “{label}”"),
        (false, true) => format!("Create “{label}”"),
        (false, false) => format!("“{label}” isn’t in the vault"),
    };
    let color = if exists { ui.text } else { ui.text_muted };
    let id = SharedString::from(format!("knowledge-out-{target}"));
    // A missing image or file can't be made from here.
    let row = if exists || is_note {
        row_shell(id, 0, ui)
            .on_click(cx.listener(move |sidebar, _, _, cx| sidebar.activate(index, cx)))
    } else {
        still_row(id, 0, ui)
    };
    row.tooltip(Tooltip::new(tooltip, None).builder())
        .child(row_icon(icon_name, ui))
        .child(
            div()
                .flex()
                .flex_none()
                .max_w(gpui::relative(0.75))
                .text_color(color)
                .child(truncated(label).grow()),
        )
        .children(detail.map(|detail| {
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .text_size(ui.small_font_size)
                .text_color(ui.text_faint)
                .child(truncated(detail).grow())
        }))
}

fn is_image(target: &str) -> bool {
    let lower = target.to_lowercase();
    IMAGE_EXTENSIONS.iter().any(|ext| lower.ends_with(ext))
}

const IMAGE_EXTENSIONS: [&str; 7] = [".png", ".jpg", ".jpeg", ".gif", ".svg", ".webp", ".bmp"];

fn heading(
    index: usize,
    row: Row,
    ui: &UiTheme,
    cx: &mut Context<KnowledgeSidebar>,
) -> gpui::Stateful<Div> {
    let Row::Heading {
        title,
        offset,
        depth,
        current,
    } = row
    else {
        return div().id("empty");
    };
    row_shell(("knowledge-heading", offset), depth, ui)
        .when(current, |row| row.bg(ui.tree_active_background))
        .text_color(if depth == 0 { ui.text } else { ui.text_muted })
        .on_click(cx.listener(move |sidebar, _, _, cx| sidebar.activate(index, cx)))
        .child(truncated(title).grow())
}

fn tag(
    index: usize,
    row: Row,
    ui: &UiTheme,
    cx: &mut Context<KnowledgeSidebar>,
) -> gpui::Stateful<Div> {
    let Row::Tag {
        name,
        label,
        depth,
        notes,
        children,
        collapsed,
    } = row
    else {
        return div().id("empty");
    };
    let caret = if collapsed {
        IconName::CaretRight
    } else {
        IconName::CaretDown
    };
    let toggle_name = name.clone();
    let selector = format!("knowledge-tag-toggle-{name}");
    let toggle = div()
        .id(SharedString::from(selector.clone()))
        .debug_selector(move || selector.clone())
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(ui.small_icon_size)
        .when(children, |toggle| {
            toggle
                .child(row_icon(caret, ui))
                .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                .on_click(cx.listener(move |sidebar, _, _, cx| {
                    cx.stop_propagation();
                    sidebar.toggle_tag(&toggle_name, cx)
                }))
        });
    let shown = if depth == 0 {
        format!("#{label}")
    } else {
        label.to_string()
    };
    let search = format!("#{name}");
    row_shell(
        SharedString::from(format!("knowledge-tag-{name}")),
        depth,
        ui,
    )
    .tooltip(Tooltip::new(format!("Search for {search}"), None).builder())
    .on_click(cx.listener(move |sidebar, _, _, cx| sidebar.activate(index, cx)))
    .child(toggle)
    .child(
        div()
            .flex()
            .flex_1()
            .min_w_0()
            .child(truncated(shown).grow()),
    )
    .child(
        div()
            .flex_none()
            .text_size(ui.small_font_size)
            .text_color(ui.text_faint)
            .child(notes.to_string()),
    )
}
