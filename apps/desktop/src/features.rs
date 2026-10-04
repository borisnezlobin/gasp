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
use crate::notices::Notice;
use crate::outline::{OutlineEvent, OutlinePicker};
use crate::palette::{CommandPalette, PaletteEvent};
use crate::print::PrintDialog;
use crate::settings_view::{SettingsEvent, SettingsRequest, SettingsView};
use crate::switcher::{QuickSwitcher, SwitcherEvent};
use crate::sync::{
    ConflictResolver, ICloudStatus, ICloudStatusEvent, StartAt, SyncIndicator, SyncIndicatorEvent,
    SyncPhase, SyncService, SyncSetup, SyncSetupEvent, SyncStart, SyncStartEvent,
};
use crate::text_input::{self, TEXT_INPUT_CONTEXT};
use crate::vault_search::{VaultSearch, VaultSearchEvent};
use crate::workspace::deleted::{DeletedNote, TrashedTo};
use crate::workspace::{OpenIn, Workspace};

/// How many palette commands count as recent.
const RECENT_COMMANDS: usize = 8;

/// Commands this module gives a handler, for the menus.
pub const WIRED_COMMANDS: [&str; 31] = [
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
    "sync.set-up",
    "sidebar.right.toggle",
    "sidebar.right.focus",
    "sidebar.backlinks",
    "sidebar.outgoing-links",
    "sidebar.outline",
    "sidebar.tags",
    "daily.open",
    "template.insert",
    "vault.import-obsidian",
    "export.copy-rich-text",
    "view.toggle-dark-mode",
    "note.move",
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
    /// Whether Mod+S in a vault that doesn't sync has offered to set sync
    /// up, which it does once a session.
    set_up_offered: bool,
    /// The last notice saying what Mod+S saved.
    save_notice: Option<u64>,
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
    if crate::sync::icloud::is_icloud_vault(workspace.vault(), cx) {
        install_icloud_status(workspace, window, cx);
    } else if crate::sandbox::reaches_outside() {
        install_sync(workspace, window, cx);
    }
    workspace.on_command("sync.now", sync_now);
    workspace.on_command("sync.set-up", open_sync_setup);
    crate::knowledge::install(workspace, window, cx);
    crate::prose::commands::install(workspace, cx);
    crate::recovery::install(workspace, cx);
    crate::obsidian_import::install(workspace, window, cx);
    crate::appearance_toggle::install(workspace);
    workspace.on_command("note.move", crate::move_picker::open);
    #[cfg(target_os = "macos")]
    crate::rich_copy::install(workspace);
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
    workspace.on_command(
        crate::attachment_cleanup::DELETE_COMMAND,
        crate::attachment_cleanup::delete_offered_image,
    );
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
    let opens_in_new_tab = workspace.config().settings.files.open_in_new_tab;
    let tree = cx.new(|cx| {
        let mut tree = FileTree::with_options(vault, options, window, cx);
        tree.set_opens_in_new_tab(opens_in_new_tab, cx);
        tree
    });
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
            let open_in = workspace.open_in_for(*new_tab, cx);
            open_note(workspace, path, open_in, window, cx);
            tree.update(cx, |tree, cx| tree.set_active_path(Some(path), cx));
        }
        FileTreeEvent::Renamed { from, to } => {
            workspace.entry_moved(from, to, cx);
        }
        FileTreeEvent::Dismissed => workspace.leave_left_panel(window, cx),
        FileTreeEvent::Trashed {
            path,
            text,
            trashed_to,
        } => {
            let text = workspace.text_before_delete(path, cx).or(text.clone());
            let mode = workspace.config().settings.files.trash;
            let trashed_to = TrashedTo::of(trashed_to.clone(), mode);
            let note = DeletedNote::new(path.clone(), text, trashed_to);
            workspace.remember_deleted(note, cx);
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
    workspace.on_command("sync.resolve-conflicts", open_resolver);
}

/// A vault in iCloud Drive shows iCloud's state where git's indicator goes.
fn install_icloud_status(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut gpui::Context<Workspace>,
) {
    let root = workspace.vault().to_path_buf();
    let status = cx.new(|cx| ICloudStatus::new(root, cx));
    let events = cx.subscribe_in(
        &status,
        window,
        |workspace, _, event: &ICloudStatusEvent, window, cx| {
            let ICloudStatusEvent::Compare { original, copy } = event;
            open_note(workspace, original, OpenIn::ActiveTab, window, cx);
            open_note(workspace, copy, OpenIn::SplitRight, window, cx);
        },
    );
    features(cx).subscriptions.push(events);
    workspace.set_sync_widget(status.into(), cx);
}

fn workspace_key(service: &Entity<SyncService>) -> EntityId {
    service.entity_id()
}

/// `sync.now`: syncs, or shows what's in the way (signing in, a vault on
/// the wrong branch) in the sync popover. Notes waiting on a conflict
/// aren't in the way: everything else syncs. In a vault that doesn't
/// sync, where Mod+S is pressed out of habit, it saves every note.
fn sync_now(workspace: &mut Workspace, window: &mut Window, cx: &mut gpui::Context<Workspace>) {
    let phase = workspace
        .sync()
        .map_or(SyncPhase::Hidden, |service| service.read(cx).phase());
    match phase {
        SyncPhase::Hidden => return save_everything(workspace, cx),
        SyncPhase::Starting => return,
        _ => {}
    }
    let Some(service) = workspace.sync().cloned() else {
        return;
    };
    let blocked = matches!(phase, SyncPhase::Setup(_) | SyncPhase::SignIn { .. });
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

/// Saves every note with unsaved edits and says so in a notice that
/// leaves by itself. The first time in a session it also offers to set
/// sync up. A new one takes the last one's place rather than stacking.
fn save_everything(workspace: &mut Workspace, cx: &mut gpui::Context<Workspace>) {
    let saved = workspace.save_all_and_list(cx);
    let mut notice = Notice::done(saved_message(&saved));
    let in_icloud = crate::sync::icloud::is_icloud_vault(workspace.vault(), cx);
    let state = features(cx);
    if !state.set_up_offered && !in_icloud {
        state.set_up_offered = true;
        notice = notice.with_action("Set up sync", "sync.set-up");
    }
    if let Some(last) = state.save_notice.take() {
        crate::notices::dismiss(last, cx);
    }
    let shown = crate::notices::show(notice, cx);
    features(cx).save_notice = Some(shown);
}

/// "Everything’s saved.", "Saved “Plan”." or "Saved 3 notes."
pub fn saved_message(saved: &[PathBuf]) -> String {
    match saved {
        [] => "Everything’s saved.".to_owned(),
        [one] => format!("Saved “{}”.", crate::workspace::files::note_title(one)),
        many => format!("Saved {} notes.", many.len()),
    }
}

/// `sync.set-up`: the choice between iCloud and GitHub for a vault that
/// doesn't sync. A vault that already syncs, with git or iCloud, gets
/// the Sync settings instead.
fn open_sync_setup(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut gpui::Context<Workspace>,
) {
    open_sync_start(workspace, StartAt::Choice, window, cx);
}

/// Opens "Set up sync" at `start`, unless the vault syncs already.
pub fn open_sync_start(
    workspace: &mut Workspace,
    start: StartAt,
    window: &mut Window,
    cx: &mut gpui::Context<Workspace>,
) {
    let syncs = workspace
        .sync()
        .is_some_and(|sync| sync.read(cx).is_present());
    if syncs || crate::sync::icloud::is_icloud_vault(workspace.vault(), cx) {
        open_settings_at(workspace, "sync", window, cx);
        return;
    }
    let host = cx.weak_entity();
    let root = workspace.vault().to_path_buf();
    let settings = workspace.config().settings.sync.clone();
    workspace.toggle_modal(window, cx, |window, cx| {
        SyncStart::new(host, root, settings, start, window, cx)
    });
    let Some(dialog) = workspace.active_modal::<SyncStart>() else {
        return;
    };
    let requests = cx.subscribe_in(
        &dialog,
        window,
        |_, _, request: &SyncStartEvent, window, cx| match request {
            SyncStartEvent::OpenForm => cx.defer_in(window, |workspace, window, cx| {
                open_sync_form(workspace, window, cx)
            }),
            SyncStartEvent::RunCommand(id) => run_after_modal_closes(id.clone(), window, cx),
        },
    );
    features(cx).subscriptions.push(requests);
}

/// The form for a repository address and a token, for people who have both.
fn open_sync_form(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut gpui::Context<Workspace>,
) {
    let host = cx.weak_entity();
    let root = workspace.vault().to_path_buf();
    let settings = workspace.config().settings.sync.clone();
    workspace.toggle_modal(window, cx, |window, cx| {
        SyncSetup::new(host, root, settings, window, cx)
    });
    let Some(setup) = workspace.active_modal::<SyncSetup>() else {
        return;
    };
    let requests = cx.subscribe_in(
        &setup,
        window,
        |_, _, request: &SyncSetupEvent, window, cx| {
            let SyncSetupEvent::RunCommand(id) = request;
            run_after_modal_closes(id.clone(), window, cx);
        },
    );
    features(cx).subscriptions.push(requests);
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
            let open_in = workspace.open_in_for(*new_tab, cx);
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
        let open_in = workspace.open_in_for(false, cx);
        open_note(workspace, &path, open_in, window, cx);
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
    offer_agent_apps(&settings, cx);
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

/// Lists the AI apps on this Mac on the General page, set up to start this
/// copy of the app. A snapshot run leaves them out: it never reaches
/// outside its own folders.
fn offer_agent_apps(settings: &gpui::Entity<SettingsView>, cx: &mut gpui::Context<Workspace>) {
    if !crate::sandbox::reaches_outside() {
        return;
    }
    let (Some(home), Ok(binary)) = (
        gasp_mcp::clients::ClientHome::current(),
        std::env::current_exe(),
    ) else {
        return;
    };
    settings.update(cx, |settings, cx| settings.set_agent_apps(home, binary, cx));
}

/// Applies a settings change everywhere it shows: every open note gets the
/// new config, new shortcuts are bound, and the file tree follows the files
/// settings.
pub(crate) fn config_files_changed(workspace: &mut Workspace, cx: &mut gpui::Context<Workspace>) {
    on_setting_changed(workspace, "rules", cx);
}

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
        let opens_in_new_tab = workspace.config().settings.files.open_in_new_tab;
        tree.update(cx, |tree, cx| {
            tree.set_options(options);
            tree.set_opens_in_new_tab(opens_in_new_tab, cx);
        });
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
