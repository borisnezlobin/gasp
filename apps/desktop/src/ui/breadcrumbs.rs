//! A path as clickable steps: `Essays / Drafts / Calendar`. Every step but
//! the last can be clicked; the last one is where you are.

use std::rc::Rc;

use gpui::{App, ElementId, SharedString, Window, div, prelude::*};

use super::ui_theme;

type CrumbHandler = Rc<dyn Fn(&mut Window, &mut App)>;

/// One step of the path.
#[derive(Clone)]
pub struct Crumb {
    pub label: SharedString,
    on_click: Option<CrumbHandler>,
}

impl Crumb {
    pub fn new(label: impl Into<SharedString>) -> Crumb {
        Crumb {
            label: label.into(),
            on_click: None,
        }
    }

    pub fn on_click(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Crumb {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

/// The breadcrumb trail. Long trails shrink each step with an ellipsis.
#[derive(IntoElement)]
pub struct Breadcrumbs {
    id: SharedString,
    crumbs: Vec<Crumb>,
}

impl Breadcrumbs {
    pub fn new(id: impl Into<SharedString>, crumbs: Vec<Crumb>) -> Breadcrumbs {
        Breadcrumbs {
            id: id.into(),
            crumbs,
        }
    }
}

impl RenderOnce for Breadcrumbs {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = ui_theme(cx);
        let last = self.crumbs.len().saturating_sub(1);
        let mut children = Vec::new();
        for (index, crumb) in self.crumbs.into_iter().enumerate() {
            if index > 0 {
                children.push(
                    div()
                        .flex_none()
                        .text_color(theme.text_faint)
                        .child("/")
                        .into_any_element(),
                );
            }
            let selector = format!("crumb-{}", crumb.label);
            let step = div()
                .id(ElementId::NamedInteger(self.id.clone(), index as u64))
                .debug_selector(|| selector)
                .min_w_0()
                .truncate()
                .px(theme.space_xs)
                .rounded(theme.icon_button_radius)
                .text_color(if index == last {
                    theme.text
                } else {
                    theme.text_muted
                })
                .when(index == last, |step| step.flex_none())
                .when_some(crumb.on_click, |step, handler| {
                    step.hover(|style| style.text_color(theme.text).bg(theme.control_hover))
                        .on_click(move |_, window, cx| handler(window, cx))
                })
                .child(crumb.label);
            children.push(step.into_any_element());
        }
        div()
            .flex()
            .flex_row()
            .items_center()
            .min_w_0()
            .gap(theme.space_sm)
            .children(children)
    }
}
