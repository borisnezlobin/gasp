//! What Obsidian users expect beyond editing: the vault's link index, the
//! right sidebar (backlinks, outgoing links, outline and tags), daily
//! notes, templates, and links that follow a renamed note.
//!
//! [`install`] wires it all into a workspace: the index for its vault,
//! the sidebar in its right panel, and the commands.

pub mod bench;
pub mod build;
pub mod daily;
pub mod dates;
pub mod edit;
pub mod index;
pub mod mentions;
pub mod parse;
pub mod rename;
pub mod sidebar;
mod sidebar_keys;
mod sidebar_render;
pub mod templates;

use std::path::Path;

use gpui::{AppContext, Context, Entity, Focusable, Window};

pub use self::sidebar::{KnowledgeSidebar, SidebarEvent, SidebarView};
pub use self::sidebar_keys::{KEY_HINTS, is_actionable};
use crate::link_update::parent_dir;
use crate::vault_search::VaultSearch;
use crate::workspace::{OpenIn, Workspace};

/// Commands this module gives a handler.
pub const COMMANDS: [&str; 8] = [
    "sidebar.right.toggle",
    "sidebar.right.focus",
    "sidebar.backlinks",
    "sidebar.outgoing-links",
    "sidebar.outline",
    "sidebar.tags",
    "daily.open",
    "template.insert",
];

/// The folder part of a vault-relative path, for showing beside a title.
fn index_folder(path: &str) -> String {
    parent_dir(path).to_string()
}

/// Gives `workspace` its vault's index, the right sidebar and the
/// commands for both, daily notes and templates.
pub fn install(workspace: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>) {
    let vault = workspace.vault().to_path_buf();
    let index = workspace.vault_index().clone();
    let view =
        SidebarView::from_key(&workspace.right_panel().view_key).unwrap_or(SidebarView::Backlinks);
    let sidebar = cx.new(|cx| KnowledgeSidebar::new(&vault, index, view, cx));
    workspace.set_right_panel(sidebar.clone().into(), cx);
    workspace.set_right_panel_focus(sidebar.focus_handle(cx));
    let events = cx.subscribe_in(&sidebar, window, on_sidebar_event);
    workspace.keep_subscription(events);
    // The workspace entity exists once it's built; follow it from then.
    let follower = sidebar.clone();
    cx.defer_in(window, move |_, _, cx| {
        let workspace = cx.entity();
        follower.update(cx, |sidebar, cx| sidebar.follow_workspace(&workspace, cx));
    });
    let toggle = sidebar.clone();
    workspace.on_command("sidebar.right.toggle", move |workspace, _, cx| {
        let visible = !workspace.right_panel().is_visible();
        let view = toggle.read(cx).view();
        show_sidebar(workspace, visible.then_some(view), &toggle, cx);
    });
    let focus = sidebar.clone();
    workspace.on_command("sidebar.right.focus", move |workspace, window, cx| {
        if !workspace.right_panel().is_visible() {
            let view = focus.read(cx).view();
            show_sidebar(workspace, Some(view), &focus, cx);
        }
        focus.update(cx, |sidebar, cx| sidebar.focus(window, cx));
    });
    for view in SidebarView::ALL {
        let sidebar = sidebar.clone();
        workspace.on_command(view.command(), move |workspace, _, cx| {
            let showing = workspace.right_panel().is_visible() && sidebar.read(cx).view() == view;
            show_sidebar(workspace, (!showing).then_some(view), &sidebar, cx);
        });
    }
    workspace.on_command("daily.open", open_daily_note);
    workspace.on_command("template.insert", open_template_picker);
}

/// Shows the sidebar on `view`, or hides it for `None`, and remembers
/// which.
fn show_sidebar(
    workspace: &mut Workspace,
    view: Option<SidebarView>,
    sidebar: &Entity<KnowledgeSidebar>,
    cx: &mut Context<Workspace>,
) {
    if let Some(view) = view {
        sidebar.update(cx, |sidebar, cx| sidebar.set_view(view, cx));
        workspace.set_right_panel_view_key(view.key());
    }
    workspace.set_right_panel_visible(view.is_some(), cx);
}

fn on_sidebar_event(
    workspace: &mut Workspace,
    sidebar: &Entity<KnowledgeSidebar>,
    event: &SidebarEvent,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    match event {
        SidebarEvent::Open { path, offset } => open_at(workspace, path, *offset, window, cx),
        SidebarEvent::Follow(target) => {
            if let Some(editor) = workspace.active_editor(cx) {
                workspace.follow_link(target, &editor, window, cx);
                workspace.focus_active(window, cx);
            }
        }
        SidebarEvent::Jump(offset) => {
            if let Some(editor) = workspace.active_editor(cx) {
                editor.update(cx, |editor, cx| editor.select(*offset, *offset, cx));
                window.focus(&editor.focus_handle(cx));
            }
        }
        SidebarEvent::SearchTag(tag) => search_for(workspace, tag, window, cx),
        SidebarEvent::LinkMention {
            source,
            range,
            expected,
            link,
        } => {
            let range = range.clone();
            let change = |text: &str| mentions::link_mention(text, range, expected, link);
            if let Err(error) = edit::edit_note(workspace, source, change, cx) {
                eprintln!("could not link the mention: {error}");
            }
        }
        SidebarEvent::Show(view) => show_sidebar(workspace, Some(*view), sidebar, cx),
        SidebarEvent::Hide => show_sidebar(workspace, None, sidebar, cx),
        SidebarEvent::Dismissed => workspace.focus_active(window, cx),
    }
}

/// Opens the note at `path` with the cursor at `offset`, and focuses it.
fn open_at(
    workspace: &mut Workspace,
    path: &Path,
    offset: Option<usize>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    if let Err(error) = workspace.open_path(path, OpenIn::ActiveTab, window, cx) {
        eprintln!("could not open {}: {error}", path.display());
        return;
    }
    if let (Some(offset), Some(editor)) = (offset, workspace.active_editor(cx)) {
        let offset = offset.min(editor.read(cx).doc().len());
        editor.update(cx, |editor, cx| editor.select(offset, offset, cx));
    }
    workspace.focus_active(window, cx);
}

/// Opens vault search with `query` typed in.
fn search_for(
    workspace: &mut Workspace,
    query: &str,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    if workspace.active_modal::<VaultSearch>().is_none() {
        workspace.run_command("search.open", window, cx);
    }
    if let Some(search) = workspace.active_modal::<VaultSearch>() {
        search.update(cx, |search, cx| search.set_query(query, cx));
    }
}

/// `daily.open`: today's note, made from the daily template the first
/// time, with the cursor at its end.
fn open_daily_note(workspace: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>) {
    let settings = &workspace.config().settings;
    let made = daily::ensure_daily_note(
        workspace.vault(),
        &settings.daily_notes,
        &settings.templates,
        dates::now(),
    );
    let path = match made {
        Ok((path, _)) => path,
        Err(error) => {
            eprintln!("could not make today's note: {error}");
            return;
        }
    };
    open_at(workspace, &path, Some(usize::MAX), window, cx);
}

/// `template.insert`: picks a template and puts it in at the cursor.
fn open_template_picker(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let Some(editor) = workspace.active_editor(cx) else {
        return;
    };
    let settings = workspace.config().settings.templates.clone();
    let folder = templates::templates_folder(workspace.vault(), &settings.folder);
    let title = workspace
        .active_path(cx)
        .map(|path| crate::workspace::files::note_title(&path))
        .unwrap_or_default();
    workspace.toggle_modal(window, cx, |window, cx| {
        templates::TemplatePicker::new(folder, &settings.folder, window, cx)
    });
    let Some(picker) = workspace.active_modal::<templates::TemplatePicker>() else {
        return;
    };
    let subscription = cx.subscribe_in(
        &picker,
        window,
        move |_, _, chosen: &templates::TemplateChosen, window, cx| {
            let context = templates::TemplateContext {
                title: title.clone(),
                now: dates::now(),
                date_format: settings.date_format.clone(),
                time_format: settings.time_format.clone(),
            };
            let text = match std::fs::read_to_string(&chosen.0) {
                Ok(template) => templates::fill(&template, &context),
                Err(error) => {
                    eprintln!("could not read {}: {error}", chosen.0.display());
                    return;
                }
            };
            let editor = editor.clone();
            cx.defer_in(window, move |_, window, cx| {
                editor.update(cx, |editor, cx| editor.insert(&text, cx));
                window.focus(&editor.focus_handle(cx));
            });
        },
    );
    workspace.keep_subscription(subscription);
}
