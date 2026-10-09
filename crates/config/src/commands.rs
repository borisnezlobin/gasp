//! The command registry: every action in the app is a named command.

use std::collections::BTreeMap;
use std::fmt;

/// Arguments passed to a command, from a rule's `args` table or from code.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Args(toml::Table);

impl Args {
    pub fn new() -> Args {
        Args::default()
    }

    pub fn with(mut self, key: &str, value: impl Into<toml::Value>) -> Args {
        self.0.insert(key.to_string(), value.into());
        self
    }

    pub fn get(&self, key: &str) -> Option<&toml::Value> {
        self.0.get(key)
    }

    pub fn str(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(toml::Value::as_str)
    }

    pub fn int(&self, key: &str) -> Option<i64> {
        self.get(key).and_then(toml::Value::as_integer)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<toml::Table> for Args {
    fn from(table: toml::Table) -> Args {
        Args(table)
    }
}

/// Why running a command failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandError {
    NotFound(String),
    NoHandler(String),
    /// A `before` hook stopped the command.
    Cancelled(String),
    Failed(String),
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandError::NotFound(id) => write!(f, "there’s no command called `{id}`"),
            CommandError::NoHandler(id) => write!(f, "`{id}` isn’t available here"),
            CommandError::Cancelled(id) => write!(f, "`{id}` was cancelled"),
            CommandError::Failed(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for CommandError {}

pub type CommandResult = Result<(), CommandError>;

/// Runs a command. Apps supply these; `Ctx` is whatever state the app passes in.
pub type Handler<Ctx> = Box<dyn Fn(&mut Ctx, &Args) -> CommandResult>;

/// Runs before or after a command. An error from a `before` hook stops the command.
pub type Hook<Ctx> = Box<dyn Fn(&mut Ctx, &Args) -> CommandResult>;

/// The next layer an `instead` hook can call to run the original behaviour.
pub type Next<'a, Ctx> = &'a dyn Fn(&mut Ctx, &Args) -> CommandResult;

/// Replaces a command. It may call `next` to fall through to what it wraps.
pub type InsteadHook<Ctx> = Box<dyn Fn(&mut Ctx, &Args, Next<'_, Ctx>) -> CommandResult>;

/// A built-in command's description.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandSpec {
    pub id: &'static str,
    /// A plain sentence-case title for the palette.
    pub title: &'static str,
    pub category: &'static str,
    /// Whether the palette lists it.
    pub palette: bool,
    /// The Phosphor icon toolbars show it with, by name, such as `text-b`.
    pub icon: &'static str,
}

/// The icon a command shows until it's given its own.
pub const DEFAULT_ICON: &str = "lightning";

impl CommandSpec {
    /// The same command shown with the Phosphor icon `name`.
    pub const fn icon(self, name: &'static str) -> CommandSpec {
        CommandSpec { icon: name, ..self }
    }
}

const fn spec(id: &'static str, title: &'static str, category: &'static str) -> CommandSpec {
    CommandSpec {
        id,
        title,
        category,
        palette: true,
        icon: DEFAULT_ICON,
    }
}

/// A command that only makes sense from its key, such as moving the cursor,
/// so the palette leaves it out.
const fn key_only(id: &'static str, title: &'static str, category: &'static str) -> CommandSpec {
    CommandSpec {
        id,
        title,
        category,
        palette: false,
        icon: DEFAULT_ICON,
    }
}

/// The system dictionary's popover for the selection or the word at the
/// caret, which the Apple platforms have.
const LOOK_UP: CommandSpec = spec("edit.look-up", "Look up", "Editing").icon("book-open");

/// Puts the iPhone's software keyboard away.
const HIDE_KEYBOARD: CommandSpec =
    spec("keyboard.hide", "Hide the keyboard", "Editing").icon("keyboard");

/// The iPhone's grid of open tabs, which the desktop shows as its tab bar.
const TAB_OVERVIEW: CommandSpec =
    spec("tab.overview", "Show all tabs", "Tabs and panels").icon("tabs");

/// Folding headings, and the foldable callout the cursor is in.
const FOLD_TOGGLE: CommandSpec =
    spec("fold.toggle", "Fold or unfold heading", "View").icon("caret-down");
const FOLD_ALL: CommandSpec =
    spec("fold.all", "Fold every heading", "View").icon("arrows-in-line-vertical");
const UNFOLD_ALL: CommandSpec =
    spec("fold.unfold-all", "Unfold every heading", "View").icon("arrows-out-line-vertical");

/// Brings back the note most recently moved to the trash, which the
/// desktop keeps the text of for the session.
const RESTORE_DELETED: CommandSpec = spec(
    "note.restore-deleted",
    "Restore the last deleted note",
    "Notes and navigation",
)
.icon("arrow-counter-clockwise");

/// Reads the vault's `.obsidian` folder into its `.gasp` config, which
/// the desktop does.
const IMPORT_OBSIDIAN: CommandSpec = spec(
    "vault.import-obsidian",
    "Import settings from Obsidian",
    "App",
)
.icon("arrow-square-in");

/// The selection or note as formatted text on the clipboard, which the
/// macOS pasteboard and the Linux clipboard can hold beside its plain
/// text.
const COPY_RICH_TEXT: CommandSpec =
    spec("export.copy-rich-text", "Copy as rich text", "App").icon("clipboard");

/// Light or dark, whichever isn't showing, which the desktop writes as
/// the theme setting.
const TOGGLE_DARK_MODE: CommandSpec = spec(
    "view.toggle-dark-mode",
    "Switch light and dark mode",
    "View",
)
.icon("palette");

/// Moves the open note into a folder picked by name, which the desktop
/// does; the phone moves notes from its file list.
const MOVE_NOTE: CommandSpec =
    spec("note.move", "Move note to a folder", "Notes and navigation").icon("folder");

/// Connects a vault that isn't a git clone to a repository in place,
/// which the desktop does; the phone sets up sync by cloning, from
/// `sync.now`.
const SET_UP_SYNC: CommandSpec = spec("sync.set-up", "Set up sync", "App").icon("cloud-arrow-up");

/// Asks gaspmd.com for the newest version and says what it found, which
/// the Mac app does.
const CHECK_FOR_UPDATES: CommandSpec =
    spec("app.check-for-updates", "Check for updates", "App").icon("cloud-arrow-down");

/// Downloads, checks and stages the version a check found, or checks
/// first when none has been found.
const INSTALL_UPDATE: CommandSpec =
    spec("app.install-update", "Install the update", "App").icon("arrow-down");

/// Quits, swaps in the staged version and opens it, or checks first when
/// none is staged.
const RESTART_TO_UPDATE: CommandSpec =
    spec("app.restart-to-update", "Restart to update", "App").icon("arrow-clockwise");

/// The release page of the version a check found, or a check first when
/// none has been found.
const SHOW_RELEASE_NOTES: CommandSpec =
    spec("app.release-notes", "Show what’s new in the update", "App").icon("article");

/// Commands only some platforms have. The registry leaves them out
/// elsewhere, but rules may still name them, as a vault's config goes
/// from machine to machine.
pub const PLATFORM_COMMANDS: &[&str] = &[
    LOOK_UP.id,
    HIDE_KEYBOARD.id,
    TAB_OVERVIEW.id,
    RESTORE_DELETED.id,
    IMPORT_OBSIDIAN.id,
    COPY_RICH_TEXT.id,
    TOGGLE_DARK_MODE.id,
    MOVE_NOTE.id,
    SET_UP_SYNC.id,
    CHECK_FOR_UPDATES.id,
    INSTALL_UPDATE.id,
    RESTART_TO_UPDATE.id,
    SHOW_RELEASE_NOTES.id,
];

/// Every built-in command on this platform.
pub const BUILTIN_COMMANDS: &[CommandSpec] = &[
    spec("format.bold", "Toggle bold", "Formatting").icon("text-b"),
    spec("format.italic", "Toggle italic", "Formatting").icon("text-italic"),
    spec("format.underline", "Toggle underline", "Formatting").icon("text-underline"),
    spec("format.link", "Insert or edit link", "Formatting").icon("link"),
    spec("format.code", "Toggle inline code", "Formatting").icon("code"),
    spec("format.strikethrough", "Toggle strikethrough", "Formatting").icon("text-strikethrough"),
    spec("format.highlight", "Toggle highlight", "Formatting").icon("highlighter-circle"),
    spec("format.math-inline", "Toggle inline math", "Formatting").icon("sigma"),
    spec("format.comment", "Toggle comment", "Formatting").icon("chat-text"),
    spec("format.callout", "Insert callout", "Formatting").icon("quotes"),
    spec(
        "format.horizontal-rule",
        "Insert horizontal rule",
        "Formatting",
    )
    .icon("minus"),
    spec("format.code-block", "Insert code block", "Formatting").icon("code-block"),
    spec("format.math-block", "Insert math block", "Formatting").icon("function"),
    spec("format.bullet-list", "Toggle bulleted list", "Formatting").icon("list-bullets"),
    spec("format.numbered-list", "Toggle numbered list", "Formatting").icon("list-numbers"),
    spec(
        "markdown.cycle-symbols",
        "Cycle Markdown symbols",
        "Formatting",
    )
    .icon("hash"),
    spec(
        "footnote.insert-or-jump",
        "Insert or jump to footnote",
        "Formatting",
    )
    .icon("text-superscript"),
    spec("footnote.tidy", "Renumber footnotes", "Formatting").icon("list-numbers"),
    spec(
        "footnote.fix-typos",
        "Convert inline footnote typos",
        "Formatting",
    )
    .icon("checks"),
    spec(
        "prose.toggle-sentence-highlighting",
        "Toggle sentence-length highlighting",
        "Formatting",
    )
    .icon("article"),
    spec("find.open", "Find in note", "Find and search").icon("magnifying-glass"),
    spec("find.next", "Next match", "Find and search").icon("arrow-down"),
    spec("find.previous", "Previous match", "Find and search").icon("arrow-up"),
    spec(
        "find.replace",
        "Find and replace in note",
        "Find and search",
    )
    .icon("swap"),
    spec("search.open", "Search all notes", "Find and search").icon("magnifying-glass-plus"),
    spec(
        "switcher.open",
        "Open quick switcher",
        "Notes and navigation",
    )
    .icon("compass"),
    CommandSpec {
        palette: false,
        ..spec(
            "palette.open",
            "Open command palette",
            "Notes and navigation",
        )
        .icon("command")
    },
    spec("note.new", "New note", "Notes and navigation").icon("file-plus"),
    spec(
        "outline.jump-to-heading",
        "Jump to heading",
        "Notes and navigation",
    )
    .icon("list-dashes"),
    spec(
        "link.follow",
        "Follow link under cursor",
        "Notes and navigation",
    )
    .icon("arrow-square-out"),
    spec(
        "link.make-card",
        "Turn the link on this line into a card",
        "Notes and navigation",
    )
    .icon("cards"),
    spec("history.back", "Go back", "Notes and navigation").icon("arrow-left"),
    spec("history.forward", "Go forward", "Notes and navigation").icon("arrow-right"),
    spec("tab.new", "New tab", "Tabs and panels").icon("plus"),
    spec("tab.close", "Close tab", "Tabs and panels").icon("x"),
    spec("tab.reopen", "Reopen closed tab", "Tabs and panels").icon("arrow-counter-clockwise"),
    spec("tab.go-1", "Go to tab 1", "Tabs and panels").icon("tabs"),
    spec("tab.go-2", "Go to tab 2", "Tabs and panels").icon("tabs"),
    spec("tab.go-3", "Go to tab 3", "Tabs and panels").icon("tabs"),
    spec("tab.go-4", "Go to tab 4", "Tabs and panels").icon("tabs"),
    spec("tab.go-5", "Go to tab 5", "Tabs and panels").icon("tabs"),
    spec("tab.go-6", "Go to tab 6", "Tabs and panels").icon("tabs"),
    spec("tab.go-7", "Go to tab 7", "Tabs and panels").icon("tabs"),
    spec("tab.go-8", "Go to tab 8", "Tabs and panels").icon("tabs"),
    spec("tab.go-9", "Go to tab 9", "Tabs and panels").icon("tabs"),
    spec("tab.next", "Next tab", "Tabs and panels").icon("caret-right"),
    spec("tab.previous", "Previous tab", "Tabs and panels").icon("caret-left"),
    spec(
        "sidebar.files.toggle",
        "Toggle file sidebar",
        "Tabs and panels",
    )
    .icon("sidebar-simple"),
    spec("sidebar.files.show", "Show file sidebar", "Tabs and panels").icon("sidebar-simple"),
    spec("sidebar.files.hide", "Hide file sidebar", "Tabs and panels").icon("sidebar-simple"),
    spec(
        "sidebar.right.toggle",
        "Toggle right sidebar",
        "Tabs and panels",
    )
    .icon("sidebar-simple-right"),
    spec(
        "sidebar.right.focus",
        "Focus right sidebar",
        "Tabs and panels",
    )
    .icon("sidebar-simple-right"),
    spec("sidebar.backlinks", "Show backlinks", "Tabs and panels").icon("arrow-u-up-left"),
    spec(
        "sidebar.outgoing-links",
        "Show outgoing links",
        "Tabs and panels",
    )
    .icon("link-simple"),
    spec("sidebar.outline", "Show outline", "Tabs and panels").icon("list-dashes"),
    spec("sidebar.tags", "Show tags", "Tabs and panels").icon("hash"),
    spec("file-tree.focus", "Focus file tree", "Tabs and panels").icon("folder"),
    spec(
        "pane.focus-left",
        "Focus pane on the left",
        "Tabs and panels",
    )
    .icon("arrow-left"),
    spec(
        "pane.focus-right",
        "Focus pane on the right",
        "Tabs and panels",
    )
    .icon("arrow-right"),
    spec("pane.focus-up", "Focus pane above", "Tabs and panels").icon("arrow-up"),
    spec("pane.focus-down", "Focus pane below", "Tabs and panels").icon("arrow-down"),
    spec(
        "pane.move-tab-left",
        "Move tab to the pane on the left",
        "Tabs and panels",
    )
    .icon("arrow-left"),
    spec(
        "pane.move-tab-right",
        "Move tab to the pane on the right",
        "Tabs and panels",
    )
    .icon("arrow-right"),
    spec(
        "pane.move-tab-up",
        "Move tab to the pane above",
        "Tabs and panels",
    )
    .icon("arrow-up"),
    spec(
        "pane.move-tab-down",
        "Move tab to the pane below",
        "Tabs and panels",
    )
    .icon("arrow-down"),
    spec("tab.close-others", "Close other tabs", "Tabs and panels").icon("x-circle"),
    spec(
        "tab.close-right",
        "Close tabs to the right",
        "Tabs and panels",
    )
    .icon("x-circle"),
    spec("app.print", "Print", "App").icon("printer"),
    spec("app.export", "Export", "App").icon("export"),
    spec("export.html", "Export as HTML", "App").icon("file-html"),
    spec("export.pdf", "Export as PDF", "App").icon("file-pdf"),
    spec("sync.now", "Sync now", "App").icon("arrows-clockwise"),
    spec("sync.resolve-conflicts", "Resolve sync conflicts", "App").icon("git-merge"),
    #[cfg(not(target_os = "ios"))]
    SET_UP_SYNC,
    spec("settings.open", "Open settings", "App").icon("gear-six"),
    #[cfg(target_os = "macos")]
    CHECK_FOR_UPDATES,
    #[cfg(target_os = "macos")]
    INSTALL_UPDATE,
    #[cfg(target_os = "macos")]
    RESTART_TO_UPDATE,
    #[cfg(target_os = "macos")]
    SHOW_RELEASE_NOTES,
    spec("toolbar.focus", "Focus toolbars", "Tabs and panels").icon("app-window"),
    spec("toolbar.customize", "Customize toolbars", "App").icon("sliders-horizontal"),
    spec("vault.open", "Open another vault", "App").icon("vault"),
    spec("vault.switch", "Switch vault", "App").icon("caret-up-down"),
    spec("help.shortcuts", "Show keyboard shortcuts", "App").icon("question"),
    #[cfg(not(target_os = "ios"))]
    spec("help.tour", "Take the welcome tour", "App").icon("compass"),
    spec("file-tree.new-folder", "New folder", "Notes and navigation").icon("folder-plus"),
    spec(
        "file-tree.sort",
        "Change file sort order",
        "Tabs and panels",
    )
    .icon("sort-ascending"),
    spec(
        "file-tree.collapse-all",
        "Collapse all folders",
        "Tabs and panels",
    )
    .icon("arrows-in-line-vertical"),
    spec("pane.split-right", "Split right", "Tabs and panels").icon("square-split-horizontal"),
    spec("pane.split-down", "Split down", "Tabs and panels").icon("square-split-vertical"),
    spec("pane.close", "Close pane", "Tabs and panels").icon("x"),
    spec("view.zoom-in", "Make text bigger", "View").icon("magnifying-glass-plus"),
    spec("view.zoom-out", "Make text smaller", "View").icon("magnifying-glass-minus"),
    spec("view.zoom-reset", "Reset text size", "View").icon("text-aa"),
    spec(
        "view.toggle-readable-width",
        "Toggle readable line length",
        "View",
    )
    .icon("arrows-in-line-horizontal"),
    spec(
        "file-tree.reveal-active",
        "Show the current note in the file tree",
        "Notes and navigation",
    )
    .icon("folder-open"),
    spec(
        "daily.open",
        "Open today’s daily note",
        "Notes and navigation",
    )
    .icon("calendar-blank"),
    spec("template.insert", "Insert template", "Editing").icon("stamp"),
    spec("note.rename", "Rename note", "Notes and navigation").icon("pencil-simple"),
    spec("note.delete", "Move note to trash", "Notes and navigation").icon("trash"),
    spec(
        "note.recover",
        "Recover a previous version",
        "Notes and navigation",
    )
    .icon("clock-counter-clockwise"),
    #[cfg(not(target_os = "ios"))]
    RESTORE_DELETED,
    #[cfg(not(target_os = "ios"))]
    IMPORT_OBSIDIAN,
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    COPY_RICH_TEXT,
    #[cfg(not(target_os = "ios"))]
    TOGGLE_DARK_MODE,
    #[cfg(not(target_os = "ios"))]
    MOVE_NOTE,
    spec("note.import-image", "Insert image", "Editing").icon("image"),
    spec("edit.paste-plain", "Paste as plain text", "Editing").icon("clipboard-text"),
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    LOOK_UP,
    #[cfg(target_os = "ios")]
    HIDE_KEYBOARD,
    #[cfg(target_os = "ios")]
    TAB_OVERVIEW,
    FOLD_TOGGLE,
    FOLD_ALL,
    UNFOLD_ALL,
    spec("code.copy-block", "Copy code block", "Editing").icon("copy"),
    spec("table.insert", "Insert table", "Tables").icon("table"),
    spec("table.insert-row-above", "Insert table row above", "Tables").icon("rows-plus-top"),
    spec("table.insert-row-below", "Insert table row below", "Tables").icon("rows-plus-bottom"),
    spec(
        "table.insert-column-left",
        "Insert table column to the left",
        "Tables",
    )
    .icon("columns-plus-left"),
    spec(
        "table.insert-column-right",
        "Insert table column to the right",
        "Tables",
    )
    .icon("columns-plus-right"),
    spec("table.delete-row", "Delete table row", "Tables").icon("rows"),
    spec("table.delete-column", "Delete table column", "Tables").icon("columns"),
    spec("table.move-row-up", "Move table row up", "Tables").icon("arrow-up"),
    spec("table.move-row-down", "Move table row down", "Tables").icon("arrow-down"),
    spec("table.move-column-left", "Move table column left", "Tables").icon("arrow-left"),
    spec(
        "table.move-column-right",
        "Move table column right",
        "Tables",
    )
    .icon("arrow-right"),
    spec("table.align-left", "Align table column left", "Tables").icon("text-align-left"),
    spec("table.align-center", "Align table column center", "Tables").icon("text-align-center"),
    spec("table.align-right", "Align table column right", "Tables").icon("text-align-right"),
    spec(
        "table.sort-ascending",
        "Sort table by this column, ascending",
        "Tables",
    )
    .icon("sort-ascending"),
    spec(
        "table.sort-descending",
        "Sort table by this column, descending",
        "Tables",
    )
    .icon("sort-descending"),
    spec("table.delete", "Delete table", "Tables").icon("trash"),
    spec("table.copy-markdown", "Copy table as Markdown", "Tables").icon("copy"),
    spec(
        "table.copy-tsv",
        "Copy table as tab-separated text",
        "Tables",
    )
    .icon("clipboard"),
    spec("table.edit-as-markdown", "Edit table as Markdown", "Tables").icon("code"),
    key_only("cursor.left", "Move left", "Cursor").icon("arrow-left"),
    key_only("cursor.right", "Move right", "Cursor").icon("arrow-right"),
    key_only("cursor.up", "Move up", "Cursor").icon("arrow-up"),
    key_only("cursor.down", "Move down", "Cursor").icon("arrow-down"),
    key_only("cursor.word-left", "Move to the previous word", "Cursor").icon("cursor-text"),
    key_only("cursor.word-right", "Move to the next word", "Cursor").icon("cursor-text"),
    key_only(
        "cursor.line-start",
        "Move to the start of the line",
        "Cursor",
    )
    .icon("cursor-text"),
    key_only("cursor.line-end", "Move to the end of the line", "Cursor").icon("cursor-text"),
    key_only(
        "cursor.doc-start",
        "Move to the start of the note",
        "Cursor",
    )
    .icon("cursor-text"),
    key_only("cursor.doc-end", "Move to the end of the note", "Cursor").icon("cursor-text"),
    key_only("cursor.page-up", "Move up a page", "Cursor").icon("cursor-text"),
    key_only("cursor.page-down", "Move down a page", "Cursor").icon("cursor-text"),
    key_only("select.left", "Select left", "Cursor").icon("selection"),
    key_only("select.right", "Select right", "Cursor").icon("selection"),
    key_only("select.up", "Select up", "Cursor").icon("selection"),
    key_only("select.down", "Select down", "Cursor").icon("selection"),
    key_only("select.word-left", "Select to the previous word", "Cursor").icon("selection"),
    key_only("select.word-right", "Select to the next word", "Cursor").icon("selection"),
    key_only(
        "select.line-start",
        "Select to the start of the line",
        "Cursor",
    )
    .icon("selection"),
    key_only("select.line-end", "Select to the end of the line", "Cursor").icon("selection"),
    key_only(
        "select.doc-start",
        "Select to the start of the note",
        "Cursor",
    )
    .icon("selection"),
    key_only("select.doc-end", "Select to the end of the note", "Cursor").icon("selection"),
    key_only("select.page-up", "Select up a page", "Cursor").icon("selection"),
    key_only("select.page-down", "Select down a page", "Cursor").icon("selection"),
    spec("select.all", "Select all", "Editing").icon("selection-all"),
    key_only(
        "edit.delete-backward",
        "Delete the previous character",
        "Editing",
    )
    .icon("backspace"),
    key_only(
        "edit.delete-forward",
        "Delete the next character",
        "Editing",
    )
    .icon("backspace"),
    key_only(
        "edit.delete-word-backward",
        "Delete the previous word",
        "Editing",
    )
    .icon("backspace"),
    key_only(
        "edit.delete-word-forward",
        "Delete the next word",
        "Editing",
    )
    .icon("backspace"),
    spec(
        "edit.delete-to-line-start",
        "Delete to the start of the line",
        "Editing",
    )
    .icon("backspace"),
    spec(
        "edit.delete-to-line-end",
        "Delete to the end of the line",
        "Editing",
    )
    .icon("backspace"),
    key_only("edit.newline", "New line", "Editing").icon("key-return"),
    spec("edit.indent", "Indent", "Editing").icon("text-indent"),
    spec("edit.move-line-up", "Move line up", "Editing").icon("arrow-up"),
    spec("edit.move-line-down", "Move line down", "Editing").icon("arrow-down"),
    spec("edit.duplicate-line", "Duplicate line", "Editing").icon("copy"),
    spec("edit.toggle-task", "Toggle task", "Editing").icon("check-square"),
    spec("edit.outdent", "Outdent", "Editing").icon("text-outdent"),
    spec("edit.undo", "Undo", "Editing").icon("arrow-counter-clockwise"),
    spec("edit.redo", "Redo", "Editing").icon("arrow-clockwise"),
    spec("edit.copy", "Copy", "Editing").icon("copy"),
    spec("edit.cut", "Cut", "Editing").icon("scissors"),
    spec("edit.paste", "Paste", "Editing").icon("clipboard"),
];

/// The commands in [`PLATFORM_COMMANDS`], described on every platform so
/// a toolbar synced from another machine can still name them.
const PLATFORM_SPECS: [CommandSpec; 13] = [
    LOOK_UP,
    HIDE_KEYBOARD,
    TAB_OVERVIEW,
    RESTORE_DELETED,
    IMPORT_OBSIDIAN,
    COPY_RICH_TEXT,
    TOGGLE_DARK_MODE,
    MOVE_NOTE,
    SET_UP_SYNC,
    CHECK_FOR_UPDATES,
    INSTALL_UPDATE,
    RESTART_TO_UPDATE,
    SHOW_RELEASE_NOTES,
];

/// A built-in command's description by id, on any platform.
pub fn command_spec(id: &str) -> Option<&'static CommandSpec> {
    BUILTIN_COMMANDS
        .iter()
        .chain(PLATFORM_SPECS.iter())
        .find(|spec| spec.id == id)
}

/// The Phosphor icon a command shows, or [`DEFAULT_ICON`] for one the
/// registry doesn't describe, such as a plugin's.
pub fn command_icon(id: &str) -> &'static str {
    command_spec(id).map_or(DEFAULT_ICON, |spec| spec.icon)
}

/// A registered command's description.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandInfo {
    pub id: String,
    pub title: String,
    pub category: String,
    pub palette: bool,
}

impl From<&CommandSpec> for CommandInfo {
    fn from(spec: &CommandSpec) -> CommandInfo {
        CommandInfo {
            id: spec.id.to_string(),
            title: spec.title.to_string(),
            category: spec.category.to_string(),
            palette: spec.palette,
        }
    }
}

/// Identifies a hook so it can be removed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HookId(u64);

struct Tagged<T> {
    id: HookId,
    hook: T,
}

struct Entry<Ctx> {
    info: CommandInfo,
    handler: Option<Handler<Ctx>>,
    before: Vec<Tagged<Hook<Ctx>>>,
    instead: Vec<Tagged<InsteadHook<Ctx>>>,
    after: Vec<Tagged<Hook<Ctx>>>,
}

impl<Ctx> Entry<Ctx> {
    fn new(info: CommandInfo) -> Entry<Ctx> {
        Entry {
            info,
            handler: None,
            before: Vec::new(),
            instead: Vec::new(),
            after: Vec::new(),
        }
    }
}

/// Where a hook runs relative to the command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HookPoint {
    Before,
    After,
}

/// Commands by id, with the handlers and hooks an app attaches.
pub struct CommandRegistry<Ctx> {
    entries: BTreeMap<String, Entry<Ctx>>,
    next_hook: u64,
}

impl<Ctx> Default for CommandRegistry<Ctx> {
    fn default() -> Self {
        CommandRegistry {
            entries: BTreeMap::new(),
            next_hook: 0,
        }
    }
}

impl<Ctx> CommandRegistry<Ctx> {
    /// A registry with every built-in command and no handlers yet.
    pub fn with_builtins() -> CommandRegistry<Ctx> {
        let mut registry = CommandRegistry::default();
        for spec in BUILTIN_COMMANDS {
            registry.declare(CommandInfo::from(spec));
        }
        registry
    }

    /// Adds a command, or replaces its description if it exists.
    pub fn declare(&mut self, info: CommandInfo) {
        match self.entries.get_mut(&info.id) {
            Some(entry) => entry.info = info,
            None => {
                self.entries.insert(info.id.clone(), Entry::new(info));
            }
        }
    }

    /// Attaches the code that runs a command.
    pub fn set_handler(&mut self, id: &str, handler: Handler<Ctx>) -> CommandResult {
        self.entry_mut(id)?.handler = Some(handler);
        Ok(())
    }

    pub fn add_hook(
        &mut self,
        id: &str,
        point: HookPoint,
        hook: Hook<Ctx>,
    ) -> Result<HookId, CommandError> {
        let hook_id = self.next_hook_id();
        let entry = self.entry_mut(id)?;
        let list = match point {
            HookPoint::Before => &mut entry.before,
            HookPoint::After => &mut entry.after,
        };
        list.push(Tagged { id: hook_id, hook });
        Ok(hook_id)
    }

    /// Wraps a command. The latest `instead` hook runs first and can call `next`.
    pub fn add_instead(
        &mut self,
        id: &str,
        hook: InsteadHook<Ctx>,
    ) -> Result<HookId, CommandError> {
        let hook_id = self.next_hook_id();
        self.entry_mut(id)?
            .instead
            .push(Tagged { id: hook_id, hook });
        Ok(hook_id)
    }

    /// Removes a hook from whichever command has it. Returns whether it was found.
    pub fn remove_hook(&mut self, hook: HookId) -> bool {
        let mut removed = false;
        for entry in self.entries.values_mut() {
            removed |= retain_without(&mut entry.before, hook);
            removed |= retain_without(&mut entry.after, hook);
            removed |= retain_without(&mut entry.instead, hook);
        }
        removed
    }

    /// Runs `before` hooks, then the outermost `instead` hook or the handler, then `after` hooks.
    pub fn run(&self, id: &str, ctx: &mut Ctx, args: &Args) -> CommandResult {
        let entry = self.entry(id)?;
        for before in &entry.before {
            (before.hook)(ctx, args)?;
        }
        run_layer(entry, entry.instead.len(), ctx, args)?;
        for after in &entry.after {
            (after.hook)(ctx, args)?;
        }
        Ok(())
    }

    pub fn info(&self, id: &str) -> Option<&CommandInfo> {
        self.entries.get(id).map(|entry| &entry.info)
    }

    pub fn contains(&self, id: &str) -> bool {
        self.entries.contains_key(id)
    }

    /// Every command, sorted by id.
    pub fn commands(&self) -> impl Iterator<Item = &CommandInfo> {
        self.entries.values().map(|entry| &entry.info)
    }

    pub fn ids(&self) -> Vec<&str> {
        self.entries.keys().map(String::as_str).collect()
    }

    fn entry(&self, id: &str) -> Result<&Entry<Ctx>, CommandError> {
        self.entries
            .get(id)
            .ok_or_else(|| CommandError::NotFound(id.to_string()))
    }

    fn entry_mut(&mut self, id: &str) -> Result<&mut Entry<Ctx>, CommandError> {
        self.entries
            .get_mut(id)
            .ok_or_else(|| CommandError::NotFound(id.to_string()))
    }

    fn next_hook_id(&mut self) -> HookId {
        self.next_hook += 1;
        HookId(self.next_hook)
    }
}

fn run_layer<Ctx>(entry: &Entry<Ctx>, depth: usize, ctx: &mut Ctx, args: &Args) -> CommandResult {
    if depth == 0 {
        let handler = entry
            .handler
            .as_ref()
            .ok_or_else(|| CommandError::NoHandler(entry.info.id.clone()))?;
        return handler(ctx, args);
    }
    let next = |ctx: &mut Ctx, args: &Args| run_layer(entry, depth - 1, ctx, args);
    (entry.instead[depth - 1].hook)(ctx, args, &next)
}

fn retain_without<T>(list: &mut Vec<Tagged<T>>, hook: HookId) -> bool {
    let before = list.len();
    list.retain(|tagged| tagged.id != hook);
    before != list.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    type Log = Vec<String>;

    fn logging(label: &'static str) -> Hook<Log> {
        Box::new(move |log: &mut Log, _: &Args| {
            log.push(label.to_string());
            Ok(())
        })
    }

    fn registry() -> CommandRegistry<Log> {
        let mut registry = CommandRegistry::with_builtins();
        registry
            .set_handler("format.bold", logging("bold"))
            .unwrap();
        registry
    }

    #[test]
    fn runs_the_handler() {
        let mut log = Log::new();
        registry()
            .run("format.bold", &mut log, &Args::new())
            .unwrap();
        assert_eq!(log, ["bold"]);
    }

    #[test]
    fn unknown_and_unhandled_commands_fail() {
        let registry = registry();
        let mut log = Log::new();
        assert_eq!(
            registry.run("nope", &mut log, &Args::new()),
            Err(CommandError::NotFound("nope".into()))
        );
        assert_eq!(
            registry.run("format.italic", &mut log, &Args::new()),
            Err(CommandError::NoHandler("format.italic".into()))
        );
    }

    #[test]
    fn hooks_run_in_order() {
        let mut registry = registry();
        registry
            .add_hook("format.bold", HookPoint::Before, logging("before"))
            .unwrap();
        registry
            .add_hook("format.bold", HookPoint::After, logging("after"))
            .unwrap();
        let mut log = Log::new();
        registry.run("format.bold", &mut log, &Args::new()).unwrap();
        assert_eq!(log, ["before", "bold", "after"]);
    }

    #[test]
    fn before_hook_can_cancel() {
        let mut registry = registry();
        let cancel: Hook<Log> = Box::new(|_, _| Err(CommandError::Cancelled("format.bold".into())));
        registry
            .add_hook("format.bold", HookPoint::Before, cancel)
            .unwrap();
        registry
            .add_hook("format.bold", HookPoint::After, logging("after"))
            .unwrap();
        let mut log = Log::new();
        assert!(registry.run("format.bold", &mut log, &Args::new()).is_err());
        assert!(log.is_empty());
    }

    #[test]
    fn instead_hooks_nest_and_can_fall_through() {
        let mut registry = registry();
        registry
            .add_instead(
                "format.bold",
                Box::new(|log: &mut Log, args: &Args, next: Next<'_, Log>| {
                    log.push("inner".into());
                    next(log, args)
                }),
            )
            .unwrap();
        registry
            .add_instead(
                "format.bold",
                Box::new(|log: &mut Log, args: &Args, next: Next<'_, Log>| {
                    if args.str("mode") == Some("replace") {
                        log.push("replaced".into());
                        return Ok(());
                    }
                    log.push("outer".into());
                    next(log, args)
                }),
            )
            .unwrap();
        let mut log = Log::new();
        registry.run("format.bold", &mut log, &Args::new()).unwrap();
        assert_eq!(log, ["outer", "inner", "bold"]);
        let mut log = Log::new();
        let args = Args::new().with("mode", "replace");
        registry.run("format.bold", &mut log, &args).unwrap();
        assert_eq!(log, ["replaced"]);
    }

    #[test]
    fn instead_hook_works_without_a_handler() {
        let mut registry = CommandRegistry::<Log>::with_builtins();
        registry
            .add_instead(
                "sync.now",
                Box::new(|log: &mut Log, _: &Args, _: Next<'_, Log>| {
                    log.push("synced".into());
                    Ok(())
                }),
            )
            .unwrap();
        let mut log = Log::new();
        registry.run("sync.now", &mut log, &Args::new()).unwrap();
        assert_eq!(log, ["synced"]);
    }

    #[test]
    fn hooks_can_be_removed() {
        let mut registry = registry();
        let hook = registry
            .add_hook("format.bold", HookPoint::Before, logging("before"))
            .unwrap();
        assert!(registry.remove_hook(hook));
        assert!(!registry.remove_hook(hook));
        let mut log = Log::new();
        registry.run("format.bold", &mut log, &Args::new()).unwrap();
        assert_eq!(log, ["bold"]);
    }

    #[test]
    fn hooks_on_unknown_commands_fail() {
        let mut registry = registry();
        assert!(
            registry
                .add_hook("nope", HookPoint::After, logging("x"))
                .is_err()
        );
    }

    #[test]
    fn apps_can_declare_new_commands() {
        let mut registry = registry();
        registry.declare(CommandInfo {
            id: "plugin.hello".into(),
            title: "Say hello".into(),
            category: "Plugins".into(),
            palette: true,
        });
        assert!(registry.contains("plugin.hello"));
        assert_eq!(registry.info("plugin.hello").unwrap().title, "Say hello");
    }
}
