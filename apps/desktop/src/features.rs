//! Connects the standalone views (file tree, pickers, find bar, vault
//! search, settings, export, sync) to a workspace's commands and slots.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use gasp_config::Platform;
use gasp_config::keys::KeyChord;
use gpui::{
    App, AppContext, Entity, EntityId, Focusable, Global, KeyBinding, Subscription, Window,
};

use crate::commands::handles;
use crate::editor::EditorView;
use crate::export_ui::{self, ExportDialog};
use crate::file_tree::{FileTree, FileTreeEvent, FileTreeOptions};
use crate::find::{FindBar, FindBarEvent};
use crate::keymap::{
    KEY_CONTEXT, RunCommand, WORKSPACE_CONTEXT, keystroke_for, keystroke_variants,
};
use crate::note::markdown_files;
use crate::outline::{OutlineEvent, OutlinePicker};
use crate::palette::{CommandPalette, PaletteEvent};
use crate::print::PrintDialog;
use crate::settings_view::{SettingsEvent, SettingsRequest, SettingsView};
use crate::switcher::{QuickSwitcher, SwitcherEvent};
use crate::sync::{ConflictResolver, SyncIndicator, SyncIndicatorEvent, SyncPhase, SyncService};
use crate::text_input::{self, TEXT_INPUT_CONTEXT};
use crate::vault_search::{VaultSearch, VaultSearchEvent};
use crate::workspace::deleted::DeletedNote;
use crate::workspace::{OpenIn, Workspace};

/// How many palette commands count as recent.
const RECENT_COMMANDS: usize = 8;

/// Commands this module gives a handler, for the menus.
pub const WIRED_COMMANDS: [&str; 26] = [
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
    "export.html",
    "export.pdf",
    "toolbar.customize",
    "app.print",
    "file-tree.reveal-active",
    "file-tree.focus",
    "sync.now",
    "sync.resolve-conflicts",
    "sidebar.right.toggle",
    "sidebar.right.focus",
    "sidebar.backlinks",
    "sidebar.outgoing-links",
    "sidebar.outline",
    "sidebar.tags",
    "daily.open",
    "template.insert",
];

/// Binds the keys the standalone views use inside themselves. Their text
/// inputs' editing keys come from the rules, bound by `keymap::bind_rules`.
pub fn bind_view_keys(cx: &mut App) {
    crate::ui::focus_visible::install(cx);
    crate::picker::bind_keys(cx);
    crate::find::bind_keys(cx);
    crate::vault_search::bind_keys(cx);
    export_ui::bind_keys(cx);
    crate::print::bind_keys(cx);
}

/// Replaces every key binding with those from `rules` plus the views' own
/// keys, so edits to rules.toml take effect, removals included.
pub fn bind_all_keys(rules: &gasp_config::RuleSet, cx: &mut App) {
    cx.clear_key_bindings();
    crate::keymap::bind_rules(rules, cx);
    bind_view_keys(cx);
    crate::workspace::menus::bind_window_keys(cx);
}

/// State shared by every window: recently run commands, and each pane's
/// find bar with the editor it searches.
#[derive(Default)]
struct Features {
    recent_commands: Vec<String>,
    find_bars: HashMap<EntityId, (EntityId, Entity<FindBar>)>,
    /// Each window's sync indicator, by its sync service.
    sync_indicators: HashMap<EntityId, Entity<SyncIndicator>>,
    subscriptions: Vec<Subscription>,
}

impl Global for Features {}

fn features(cx: &mut App) -> &mut Features {
    cx.default_global::<Features>()
}

/// Installs every feature into a new workspace.
pub fn install(workspace: &mut Workspace, window: &mut Window, cx: &mut gpui::Context<Workspace>) {
    // The vault's rules.toml can add, change or remove shortcuts.
    let rules = workspace.config().rules.clone();
    let span = crate::trace::span("bind-all-keys");
    bind_all_keys(&rules, cx);
    drop(span);
    // The note header's reading-view button shows a book while Markdown
    // symbols are hidden everywhere.
    let reading: crate::workspace::pane::ReadingProbe = std::rc::Rc::new(|editor| {
        editor.symbol_mode() == gasp_config::settings::SymbolMode::AlwaysHidden
    });
    workspace.set_reading_probe(reading, cx);
    install_file_tree(workspace, window, cx);
    if crate::sandbox::reaches_outside() {
        install_sync(workspace, window, cx);
    }
    crate::knowledge::install(workspace, window, cx);
    crate::prose::commands::install(workspace, cx);
    crate::recovery::install(workspace, cx);
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
    workspace.on_command("export.html", |ws, window, cx| {
        export_now(ws, export_ui::ExportFormat::Html, window, cx)
    });
    workspace.on_command("export.pdf", |ws, window, cx| {
        export_now(ws, export_ui::ExportFormat::Pdf, window, cx)
    });
    workspace.on_command("toolbar.customize", open_toolbar_settings);
    workspace.on_command("app.print", print_note);
    workspace.on_command("file-tree.reveal-active", reveal_active);
}

// ---- File tree ----

fn install_file_tree(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut gpui::Context<Workspace>,
) {
    let _span = crate::trace::span("file-tree");
    let vault = workspace.vault().to_path_buf();
    let options = FileTreeOptions::from_settings(&workspace.config().settings.files);
    let tree = cx.new(|cx| FileTree::with_options(vault, options, window, cx));
    workspace.set_file_tree(tree.clone(), cx);
    let subscription = cx.subscribe_in(&tree, window, on_tree_event);
    features(cx).subscriptions.push(subscription);
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
        FileTreeEvent::Renamed { from, to } => {
            workspace.entry_moved(from, to, cx);
        }
        FileTreeEvent::Dismissed => workspace.leave_left_panel(window, cx),
        FileTreeEvent::Trashed { path, text } => {
            let text = workspace.text_before_delete(path, cx).or(text.clone());
            if let Some(text) = text {
                let path = path.clone();
                workspace.remember_deleted(DeletedNote { path, text }, cx);
            }
        }
        FileTreeEvent::Failed { message } => {
            crate::notices::problem(message.clone(), cx);
        }
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
    workspace.reveal_in_tree(&path, true, window, cx);
}

fn open_note(
    workspace: &mut Workspace,
    path: &Path,
    open_in: OpenIn,
    window: &mut Window,
    cx: &mut gpui::Context<Workspace>,
) {
    if let Err(error) = workspace.open_path(path, open_in, window, cx) {
        crate::notices::open_failed(path, error, cx);
        return;
    }
    // A picker that opened the note closes after this and hands focus back
    // to what had it, which may be the tab the note just replaced. Focus
    // the note once that's done.
    cx.defer_in(window, |workspace, window, cx| {
        workspace.focus_active(window, cx)
    });
}

// ---- Sync ----

fn install_sync(workspace: &mut Workspace, window: &mut Window, cx: &mut gpui::Context<Workspace>) {
    let store = crate::sync::credential_store(cx);
    let settings = workspace.config().settings.sync.clone();
    let vault = workspace.vault().to_path_buf();
    let service = cx.new(|cx| SyncService::new(&vault, settings, store, cx));
    let indicator = cx.new(|cx| SyncIndicator::new(service.clone(), cx));
    let events = cx.subscribe_in(
        &indicator,
        window,
        |workspace, _, event, window, cx| match event {
            SyncIndicatorEvent::OpenSettings => open_settings_at(workspace, "sync", window, cx),
            SyncIndicatorEvent::Resolve => open_resolver(workspace, window, cx),
        },
    );
    let activation = cx.observe_window_activation(window, |workspace, window, cx| {
        if let Some(sync) = workspace
            .sync()
            .cloned()
            .filter(|_| window.is_window_active())
        {
            sync.update(cx, |sync, cx| sync.window_activated(cx));
        }
    });
    let state = features(cx);
    state.subscriptions.push(events);
    state.subscriptions.push(activation);
    state
        .sync_indicators
        .insert(workspace_key(&service), indicator.clone());
    workspace.set_sync(service, indicator.into(), cx);
    workspace.on_command("sync.now", sync_now);
    workspace.on_command("sync.resolve-conflicts", open_resolver);
}

fn workspace_key(service: &Entity<SyncService>) -> EntityId {
    service.entity_id()
}

/// `sync.now`: syncs, or shows what's in the way (signing in, a vault on
/// the wrong branch) in the sync popover. Notes waiting on a conflict
/// aren't in the way: everything else syncs.
fn sync_now(workspace: &mut Workspace, window: &mut Window, cx: &mut gpui::Context<Workspace>) {
    let Some(service) = workspace.sync().cloned() else {
        return;
    };
    let phase = service.read(cx).phase();
    let blocked = matches!(phase, SyncPhase::Setup(_) | SyncPhase::SignIn { .. });
    if matches!(phase, SyncPhase::Hidden | SyncPhase::Starting) {
        return;
    }
    if !blocked {
        service.update(cx, |service, cx| service.sync_now(cx));
        return;
    }
    let indicator = features(cx)
        .sync_indicators
        .get(&workspace_key(&service))
        .cloned();
    if let Some(indicator) = indicator.filter(|indicator| !indicator.read(cx).is_open()) {
        indicator.update(cx, |indicator, cx| indicator.toggle(window, cx));
    }
}

/// `sync.resolve-conflicts`: the resolver, over the window.
fn open_resolver(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut gpui::Context<Workspace>,
) {
    let Some(service) = workspace.sync().cloned() else {
        return;
    };
    workspace.toggle_modal(window, cx, |_, cx| ConflictResolver::new(service, cx));
    if workspace.active_modal::<ConflictResolver>().is_some() {
        workspace.set_modal_self_sized(cx);
    }
}

/// Opens the settings screen on the page `section`.
fn open_settings_at(
    workspace: &mut Workspace,
    section: &str,
    window: &mut Window,
    cx: &mut gpui::Context<Workspace>,
) {
    if workspace.active_modal::<SettingsView>().is_none() {
        open_settings(workspace, window, cx);
    }
    if let Some(settings) = workspace.active_modal::<SettingsView>() {
        settings.update(cx, |settings, cx| settings.show_section(section, cx));
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
                crate::notices::problem(format!("Couldn’t save the shortcut: {error}"), cx);
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
    let rules_file = gasp_config::config_files::rules_path(vault);
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
    let _span = crate::trace::span("switcher-open");
    crate::trace::presented(window, "switcher-open");
    let vault = workspace.vault().to_path_buf();
    // The vault index already has every path once its first read is done;
    // until then, walk the vault.
    let index = workspace.vault_index().read(cx);
    let paths = if index.is_ready() {
        index.notes().iter().map(|note| note.path.clone()).collect()
    } else {
        vault_note_paths(&vault)
    };
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
            crate::notices::problem(format!("Couldn’t make “{name}”: {error}"), cx);
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
    let _span = crate::trace::span("search-open");
    crate::trace::presented(window, "search-open");
    let texts = workspace.note_texts().clone();
    let index = workspace.vault_index().clone();
    workspace.toggle_modal(window, cx, |window, cx| {
        VaultSearch::with_texts(texts, window, cx).with_index(index)
    });
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
    let _span = crate::trace::span("settings-open");
    crate::trace::presented(window, "settings-open");
    let vault = workspace.vault().to_path_buf();
    workspace.toggle_modal(window, cx, |window, cx| {
        SettingsView::new(vault, window, cx)
    });
    let Some(settings) = workspace.active_modal::<SettingsView>() else {
        return;
    };
    workspace.set_modal_self_sized(cx);
    let config = workspace.config().clone();
    settings.update(cx, |settings, cx| settings.set_preview_config(&config, cx));
    if let Some(sync) = workspace.sync().cloned() {
        settings.update(cx, |settings, cx| settings.set_sync(sync, cx));
    }
    let changed = cx.subscribe(&settings, |workspace, _, event: &SettingsEvent, cx| {
        let SettingsEvent::Changed(key) = event;
        on_setting_changed(workspace, key, cx);
    });
    let requests = cx.subscribe_in(
        &settings,
        window,
        |_, _, request: &SettingsRequest, window, cx| {
            let SettingsRequest::RunCommand(id) = request;
            run_after_modal_closes(id.clone(), window, cx);
        },
    );
    let state = features(cx);
    state.subscriptions.push(changed);
    state.subscriptions.push(requests);
}

/// Applies a settings change everywhere it shows: every open note gets the
/// new config, new shortcuts are bound, and the file tree follows the files
/// settings.
fn on_setting_changed(workspace: &mut Workspace, key: &str, cx: &mut gpui::Context<Workspace>) {
    workspace.reload_config(cx);
    if let Some(settings) = workspace.active_modal::<SettingsView>() {
        let config = workspace.config().clone();
        settings.update(cx, |settings, cx| settings.set_preview_config(&config, cx));
    }
    // The config folder syncs too, but the watcher leaves it out.
    if let Some(sync) = workspace.sync().cloned() {
        let settings = workspace.config().settings.sync.clone();
        sync.update(cx, |sync, cx| {
            sync.apply_settings(settings, cx);
            sync.edited(cx);
        });
    }
    if key == "rules" {
        let rules = workspace.config().rules.clone();
        bind_all_keys(&rules, cx);
    }
    let vault = workspace.vault().to_path_buf();
    if let Some(tree) = workspace.file_tree().cloned() {
        let options = crate::file_tree::FileTreeOptions::for_vault(&vault);
        tree.update(cx, |tree, _| tree.set_options(options));
    }
}

fn active_note(workspace: &Workspace, cx: &App) -> Option<(String, Option<PathBuf>)> {
    let editor = workspace.active_editor(cx)?;
    Some((editor.read(cx).text(), workspace.active_path(cx)))
}

fn open_export(workspace: &mut Workspace, window: &mut Window, cx: &mut gpui::Context<Workspace>) {
    let Some((text, path)) = active_note(workspace, cx) else {
        return;
    };
    let vault = workspace.vault().to_path_buf();
    workspace.toggle_modal(window, cx, |_, cx| {
        ExportDialog::new(text, path, cx).with_vault_root(vault)
    });
}

/// Opens the export dialog already exporting as `format`.
fn export_now(
    workspace: &mut Workspace,
    format: export_ui::ExportFormat,
    window: &mut Window,
    cx: &mut gpui::Context<Workspace>,
) {
    if workspace.active_modal::<ExportDialog>().is_none() {
        open_export(workspace, window, cx);
    }
    if let Some(dialog) = workspace.active_modal::<ExportDialog>() {
        dialog.update(cx, |dialog, cx| dialog.export_as(format, cx));
    }
}

/// `toolbar.customize`: the settings screen's Toolbars page, with the
/// picker of things to add open when a toolbar's add button asked.
fn open_toolbar_settings(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut gpui::Context<Workspace>,
) {
    let adding_to = workspace.take_toolbar_to_add_to();
    open_settings_at(
        workspace,
        crate::settings_view::model::TOOLBARS_SECTION,
        window,
        cx,
    );
    let (Some(settings), Some(toolbar)) = (workspace.active_modal::<SettingsView>(), adding_to)
    else {
        return;
    };
    settings.update(cx, |settings, cx| {
        settings.start_adding_to(&toolbar, window, cx)
    });
}

fn print_note(workspace: &mut Workspace, window: &mut Window, cx: &mut gpui::Context<Workspace>) {
    let Some((text, path)) = active_note(workspace, cx) else {
        return;
    };
    let vault = Some(workspace.vault().to_path_buf());
    workspace.toggle_modal(window, cx, |window, cx| {
        PrintDialog::new(text, path, vault, window, cx)
    });
}
