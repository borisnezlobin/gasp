//! Connects the standalone views (file tree, pickers, find bar, vault
//! search, settings, export) to a workspace's commands and slots.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use editor_config::Platform;
use editor_config::keys::KeyChord;
use gpui::{
    App, AppContext, Entity, EntityId, Focusable, Global, KeyBinding, Subscription, Window,
};

use crate::commands::handles;
use crate::editor::EditorView;
use crate::export_ui::{self, ExportDialog};
use crate::file_tree::{FileTree, FileTreeEvent};
use crate::find::{FindBar, FindBarEvent};
use crate::keymap::{
    KEY_CONTEXT, RunCommand, WORKSPACE_CONTEXT, keystroke_for, keystroke_variants,
};
use crate::note::markdown_files;
use crate::outline::{OutlineEvent, OutlinePicker};
use crate::palette::{CommandPalette, PaletteEvent};
use crate::settings_view::{SettingsEvent, SettingsView};
use crate::switcher::{QuickSwitcher, SwitcherEvent};
use crate::text_input::{self, TEXT_INPUT_CONTEXT};
use crate::vault_search::{VaultSearch, VaultSearchEvent};
use crate::workspace::{OpenIn, Workspace};

/// How many palette commands count as recent.
const RECENT_COMMANDS: usize = 8;

/// Commands this module gives a handler, for the menus.
pub const WIRED_COMMANDS: [&str; 13] = [
    "palette.open",
    "switcher.open",
    "outline.jump-to-heading",
    "find.open",
    "find.replace",
    "find.next",
    "find.previous",
    "search.open",
    "settings.open",
    "app.export",
    "app.print",
    "file-tree.reveal-active",
    "file-tree.focus",
];

/// Binds the keys the standalone views use inside themselves. Their text
/// inputs' editing keys come from the rules, bound by `keymap::bind_rules`.
pub fn bind_view_keys(cx: &mut App) {
    crate::picker::bind_keys(cx);
    crate::find::bind_keys(cx);
    crate::vault_search::bind_keys(cx);
    export_ui::bind_keys(cx);
}

/// State shared by every window: recently run commands, and each pane's
/// find bar with the editor it searches.
#[derive(Default)]
struct Features {
    recent_commands: Vec<String>,
    find_bars: HashMap<EntityId, (EntityId, Entity<FindBar>)>,
    trees: HashMap<EntityId, Entity<FileTree>>,
    subscriptions: Vec<Subscription>,
}

impl Global for Features {}

fn features(cx: &mut App) -> &mut Features {
    cx.default_global::<Features>()
}

/// Installs every feature into a new workspace.
pub fn install(workspace: &mut Workspace, window: &mut Window, cx: &mut gpui::Context<Workspace>) {
    install_file_tree(workspace, window, cx);
    workspace.on_command("palette.open", open_palette);
    workspace.on_command("switcher.open", open_switcher);
    workspace.on_command("outline.jump-to-heading", open_outline);
    workspace.on_command("find.open", |ws, window, cx| {
        show_find(ws, false, window, cx)
    });
    workspace.on_command("find.replace", |ws, window, cx| {
        show_find(ws, true, window, cx)
    });
    workspace.on_command("find.next", |ws, window, cx| {
        step_find(ws, true, window, cx)
    });
    workspace.on_command("find.previous", |ws, window, cx| {
        step_find(ws, false, window, cx)
    });
    workspace.on_command("search.open", open_vault_search);
    workspace.on_command("settings.open", open_settings);
    workspace.on_command("app.export", open_export);
    workspace.on_command("app.print", print_note);
    workspace.on_command("file-tree.reveal-active", reveal_active);
}

// ---- File tree ----

fn install_file_tree(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut gpui::Context<Workspace>,
) {
    let vault = workspace.vault().to_path_buf();
    let tree = cx.new(|cx| FileTree::new(vault, window, cx));
    let focus = tree.focus_handle(cx);
    workspace.set_left_panel(tree.clone().into(), Some(focus), cx);
    let subscription = cx.subscribe_in(&tree, window, on_tree_event);
    let workspace_id = cx.entity_id();
    let state = features(cx);
    state.trees.insert(workspace_id, tree);
    state.subscriptions.push(subscription);
}

fn on_tree_event(
    workspace: &mut Workspace,
    tree: &Entity<FileTree>,
    event: &FileTreeEvent,
    window: &mut Window,
    cx: &mut gpui::Context<Workspace>,
) {
    match event {
        FileTreeEvent::Open { path, new_tab } => {
            let open_in = if *new_tab {
                OpenIn::NewTab
            } else {
                OpenIn::ActiveTab
            };
            open_note(workspace, path, open_in, window, cx);
            tree.update(cx, |tree, cx| tree.set_active_path(Some(path), cx));
        }
        FileTreeEvent::Dismissed => workspace.focus_active(window, cx),
        FileTreeEvent::Failed { message } => eprintln!("{message}"),
        _ => {}
    }
}

fn reveal_active(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut gpui::Context<Workspace>,
) {
    let Some(path) = workspace.active_path(cx) else {
        return;
    };
    let workspace_id = cx.entity_id();
    let Some(tree) = features(cx).trees.get(&workspace_id).cloned() else {
        return;
    };
    workspace.run_command("sidebar.files.show", window, cx);
    tree.update(cx, |tree, cx| {
        tree.set_active_path(Some(&path), cx);
        tree.reveal(&path, cx);
    });
    window.focus(&tree.focus_handle(cx));
}

fn open_note(
    workspace: &mut Workspace,
    path: &Path,
    open_in: OpenIn,
    window: &mut Window,
    cx: &mut gpui::Context<Workspace>,
) {
    if let Err(error) = workspace.open_path(path, open_in, window, cx) {
        eprintln!("could not open {}: {error}", path.display());
    }
}

// ---- Command palette ----

fn open_palette(workspace: &mut Workspace, window: &mut Window, cx: &mut gpui::Context<Workspace>) {
    let rules = workspace.config().rules.clone();
    let recent = features(cx).recent_commands.clone();
    workspace.toggle_modal(window, cx, |window, cx| {
        CommandPalette::new(&rules, recent, window, cx)
    });
    let Some(palette) = workspace.active_modal::<CommandPalette>() else {
        return;
    };
    let subscription = cx.subscribe_in(&palette, window, on_palette_event);
    features(cx).subscriptions.push(subscription);
}

fn on_palette_event(
    workspace: &mut Workspace,
    _: &Entity<CommandPalette>,
    event: &PaletteEvent,
    window: &mut Window,
    cx: &mut gpui::Context<Workspace>,
) {
    match event {
        PaletteEvent::Run(id) => {
            remember_command(id, cx);
            run_after_modal_closes(id.clone(), window, cx);
        }
        PaletteEvent::Bind { command, chord } => {
            if let Err(error) = bind_user_key(workspace.vault(), command, chord, cx) {
                eprintln!("could not save the shortcut: {error}");
            }
        }
    }
}

fn remember_command(id: &str, cx: &mut App) {
    let recent = &mut features(cx).recent_commands;
    recent.retain(|known| known != id);
    recent.insert(0, id.to_owned());
    recent.truncate(RECENT_COMMANDS);
}

/// Runs `id` once the modal has closed and focus is back where it was, so
/// editor commands reach the editor.
fn run_after_modal_closes(id: String, window: &mut Window, cx: &mut gpui::Context<Workspace>) {
    cx.defer_in(window, move |workspace, window, cx| {
        if !workspace.run_command(&id, window, cx) {
            window.dispatch_action(Box::new(RunCommand { id: id.into() }), cx);
        }
    });
}

/// Saves a new key rule in the vault's rules.toml and binds it now.
fn bind_user_key(vault: &Path, command: &str, chord: &str, cx: &mut App) -> std::io::Result<()> {
    let rules_file = vault.join(".editor").join("rules.toml");
    let mut text = std::fs::read_to_string(&rules_file).unwrap_or_default();
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&format!(
        "\n[[rule]]\nid   = \"user.key.{command}\"\non   = \"key\"\nkeys = \"{chord}\"\ndo   = \"{command}\"\n"
    ));
    if let Some(parent) = rules_file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&rules_file, text)?;
    let Ok(parsed) = KeyChord::parse(chord) else {
        return Ok(());
    };
    let context = if handles(command) {
        KEY_CONTEXT
    } else {
        WORKSPACE_CONTEXT
    };
    // Editing commands also run in every text input.
    let contexts = [
        Some(context),
        text_input::handles(command).then_some(TEXT_INPUT_CONTEXT),
    ];
    let keystroke = keystroke_for(parsed, Platform::current());
    let bindings = keystroke_variants(&keystroke)
        .into_iter()
        .flat_map(|keystroke| {
            contexts.into_iter().flatten().map(move |context| {
                let action = RunCommand {
                    id: command.to_owned().into(),
                };
                KeyBinding::new(&keystroke, action, Some(context))
            })
        });
    cx.bind_keys(bindings.collect::<Vec<_>>());
    Ok(())
}

// ---- Quick switcher and outline ----

fn vault_note_paths(vault: &Path) -> Vec<String> {
    markdown_files(vault)
        .unwrap_or_default()
        .iter()
        .filter_map(|path| relative(vault, path))
        .collect()
}

fn relative(vault: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(vault)
        .ok()
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
}

fn open_switcher(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut gpui::Context<Workspace>,
) {
    let vault = workspace.vault().to_path_buf();
    let paths = vault_note_paths(&vault);
    let recent = workspace
        .recent_notes()
        .iter()
        .filter_map(|path| relative(&vault, path))
        .collect();
    workspace.toggle_modal(window, cx, |window, cx| {
        QuickSwitcher::new(paths, recent, window, cx)
    });
    let Some(switcher) = workspace.active_modal::<QuickSwitcher>() else {
        return;
    };
    let subscription = cx.subscribe_in(&switcher, window, on_switcher_event);
    features(cx).subscriptions.push(subscription);
}

fn on_switcher_event(
    workspace: &mut Workspace,
    _: &Entity<QuickSwitcher>,
    event: &SwitcherEvent,
    window: &mut Window,
    cx: &mut gpui::Context<Workspace>,
) {
    let vault = workspace.vault().to_path_buf();
    match event {
        SwitcherEvent::Open { path, new_tab } => {
            let open_in = if *new_tab {
                OpenIn::NewTab
            } else {
                OpenIn::ActiveTab
            };
            open_note(workspace, &vault.join(path), open_in, window, cx);
        }
        SwitcherEvent::Create(name) => create_note(workspace, &vault, name, window, cx),
    }
}

/// Creates `name.md` (a path like `folder/name` makes the folder too) and
/// opens it. An existing note of that name just opens.
fn create_note(
    workspace: &mut Workspace,
    vault: &Path,
    name: &str,
    window: &mut Window,
    cx: &mut gpui::Context<Workspace>,
) {
    let file = if name.ends_with(".md") {
        name.to_owned()
    } else {
        format!("{name}.md")
    };
    let path: PathBuf = vault.join(file);
    if !path.exists() {
        let created = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&path, ""));
        if let Err(error) = created {
            eprintln!("could not create {}: {error}", path.display());
            return;
        }
    }
    open_note(workspace, &path, OpenIn::ActiveTab, window, cx);
}

fn open_outline(workspace: &mut Workspace, window: &mut Window, cx: &mut gpui::Context<Workspace>) {
    let Some(editor) = workspace.active_editor(cx) else {
        return;
    };
    let (text, cursor) = {
        let editor = editor.read(cx);
        (editor.text(), editor.cursor())
    };
    workspace.toggle_modal(window, cx, |window, cx| {
        OutlinePicker::new(&text, cursor, window, cx)
    });
    let Some(outline) = workspace.active_modal::<OutlinePicker>() else {
        return;
    };
    let subscription = cx.subscribe_in(&outline, window, move |_, _, event, window, cx| {
        let OutlineEvent::Jump(offset) = *event;
        let editor = editor.clone();
        cx.defer_in(window, move |_, window, cx| {
            editor.update(cx, |editor, cx| editor.select(offset, offset, cx));
            window.focus(&editor.focus_handle(cx));
        });
    });
    features(cx).subscriptions.push(subscription);
}

// ---- Find bar ----

fn show_find(
    workspace: &mut Workspace,
    replace: bool,
    window: &mut Window,
    cx: &mut gpui::Context<Workspace>,
) {
    let Some(editor) = workspace.active_editor(cx) else {
        return;
    };
    let bar = find_bar_for(workspace, &editor, window, cx);
    bar.update(cx, |bar, cx| bar.show(replace, window, cx));
}

/// The active pane's find bar for `editor`, made and shown in the pane's
/// toolbar if needed.
fn find_bar_for(
    workspace: &mut Workspace,
    editor: &Entity<EditorView>,
    window: &mut Window,
    cx: &mut gpui::Context<Workspace>,
) -> Entity<FindBar> {
    let pane = workspace.active_pane().clone();
    let pane_id = pane.entity_id();
    let existing = features(cx).find_bars.get(&pane_id).cloned();
    if let Some((_, bar)) = existing.filter(|(id, _)| *id == editor.entity_id()) {
        pane.update(cx, |pane, cx| {
            pane.set_toolbar(Some(bar.clone().into()), cx)
        });
        return bar;
    }
    let bar = cx.new(|cx| FindBar::new(editor.clone(), window, cx));
    pane.update(cx, |pane, cx| {
        pane.set_toolbar(Some(bar.clone().into()), cx)
    });
    let subscription = cx.subscribe(&bar, move |_, _, event, cx| {
        if matches!(event, FindBarEvent::Dismissed) {
            pane.update(cx, |pane, cx| pane.set_toolbar(None, cx));
        }
    });
    let state = features(cx);
    state
        .find_bars
        .insert(pane_id, (editor.entity_id(), bar.clone()));
    state.subscriptions.push(subscription);
    bar
}

fn step_find(
    workspace: &mut Workspace,
    forward: bool,
    window: &mut Window,
    cx: &mut gpui::Context<Workspace>,
) {
    let Some(editor) = workspace.active_editor(cx) else {
        return;
    };
    let bar = find_bar_for(workspace, &editor, window, cx);
    bar.update(cx, |bar, cx| {
        if forward {
            bar.next(cx);
        } else {
            bar.previous(cx);
        }
    });
}

// ---- Vault search, settings and export ----

fn open_vault_search(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut gpui::Context<Workspace>,
) {
    let vault = workspace.vault().to_path_buf();
    workspace.toggle_modal(window, cx, |window, cx| VaultSearch::new(vault, window, cx));
    let Some(search) = workspace.active_modal::<VaultSearch>() else {
        return;
    };
    let subscription = cx.subscribe_in(&search, window, on_search_event);
    features(cx).subscriptions.push(subscription);
}

fn on_search_event(
    _: &mut Workspace,
    _: &Entity<VaultSearch>,
    event: &VaultSearchEvent,
    window: &mut Window,
    cx: &mut gpui::Context<Workspace>,
) {
    let VaultSearchEvent::Open { path, offset } = event else {
        return;
    };
    let (path, offset) = (path.clone(), *offset);
    cx.defer_in(window, move |workspace, window, cx| {
        open_note(workspace, &path, OpenIn::ActiveTab, window, cx);
        if let Some(editor) = workspace.active_editor(cx) {
            editor.update(cx, |editor, cx| editor.select(offset, offset, cx));
        }
    });
}

fn open_settings(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut gpui::Context<Workspace>,
) {
    let vault = workspace.vault().to_path_buf();
    workspace.toggle_modal(window, cx, |window, cx| {
        SettingsView::new(vault, window, cx)
    });
    let Some(settings) = workspace.active_modal::<SettingsView>() else {
        return;
    };
    let subscription = cx.subscribe(&settings, |workspace, _, event: &SettingsEvent, cx| {
        let SettingsEvent::Changed(_) = event;
        let vault = workspace.vault().to_path_buf();
        let workspace_id = cx.entity_id();
        if let Some(tree) = features(cx).trees.get(&workspace_id).cloned() {
            let options = crate::file_tree::FileTreeOptions::for_vault(&vault);
            tree.update(cx, |tree, _| tree.set_options(options));
        }
    });
    features(cx).subscriptions.push(subscription);
}

fn active_note(workspace: &Workspace, cx: &App) -> Option<(String, Option<PathBuf>)> {
    let editor = workspace.active_editor(cx)?;
    Some((editor.read(cx).text(), workspace.active_path(cx)))
}

fn open_export(workspace: &mut Workspace, window: &mut Window, cx: &mut gpui::Context<Workspace>) {
    let Some((text, path)) = active_note(workspace, cx) else {
        return;
    };
    workspace.toggle_modal(window, cx, |_, cx| ExportDialog::new(text, path, cx));
}

fn print_note(workspace: &mut Workspace, _: &mut Window, cx: &mut gpui::Context<Workspace>) {
    let Some((text, path)) = active_note(workspace, cx) else {
        return;
    };
    let printing = export_ui::print(text, path, cx);
    cx.spawn(async move |_, _| {
        if let Err(error) = printing.await {
            eprintln!("could not print: {error}");
        }
    })
    .detach();
}
