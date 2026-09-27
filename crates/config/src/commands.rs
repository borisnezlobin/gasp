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
            CommandError::NotFound(id) => write!(f, "there's no command called `{id}`"),
            CommandError::NoHandler(id) => write!(f, "`{id}` isn't available here"),
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
}

const fn spec(id: &'static str, title: &'static str, category: &'static str) -> CommandSpec {
    CommandSpec {
        id,
        title,
        category,
        palette: true,
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
    }
}

/// Every built-in command.
pub const BUILTIN_COMMANDS: &[CommandSpec] = &[
    spec("format.bold", "Toggle bold", "Formatting"),
    spec("format.italic", "Toggle italic", "Formatting"),
    spec("format.underline", "Toggle underline", "Formatting"),
    spec("format.link", "Insert or edit link", "Formatting"),
    spec("format.code", "Toggle inline code", "Formatting"),
    spec("format.strikethrough", "Toggle strikethrough", "Formatting"),
    spec("format.highlight", "Toggle highlight", "Formatting"),
    spec("format.math-inline", "Toggle inline math", "Formatting"),
    spec("format.comment", "Toggle comment", "Formatting"),
    spec(
        "markdown.cycle-symbols",
        "Cycle Markdown symbols",
        "Formatting",
    ),
    spec(
        "footnote.insert-or-jump",
        "Insert or jump to footnote",
        "Formatting",
    ),
    spec(
        "prose.toggle-sentence-highlighting",
        "Toggle sentence-length highlighting",
        "Formatting",
    ),
    spec("find.open", "Find in note", "Find and search"),
    spec("find.next", "Next match", "Find and search"),
    spec("find.previous", "Previous match", "Find and search"),
    spec(
        "find.replace",
        "Find and replace in note",
        "Find and search",
    ),
    spec("search.open", "Search all notes", "Find and search"),
    spec(
        "switcher.open",
        "Open quick switcher",
        "Notes and navigation",
    ),
    CommandSpec {
        palette: false,
        ..spec(
            "palette.open",
            "Open command palette",
            "Notes and navigation",
        )
    },
    spec("note.new", "New note", "Notes and navigation"),
    spec(
        "outline.jump-to-heading",
        "Jump to heading",
        "Notes and navigation",
    ),
    spec(
        "link.follow",
        "Follow link under cursor",
        "Notes and navigation",
    ),
    spec("history.back", "Go back", "Notes and navigation"),
    spec("history.forward", "Go forward", "Notes and navigation"),
    spec("tab.new", "New tab", "Tabs and panels"),
    spec("tab.close", "Close tab", "Tabs and panels"),
    spec("tab.reopen", "Reopen closed tab", "Tabs and panels"),
    spec("tab.go-1", "Go to tab 1", "Tabs and panels"),
    spec("tab.go-2", "Go to tab 2", "Tabs and panels"),
    spec("tab.go-3", "Go to tab 3", "Tabs and panels"),
    spec("tab.go-4", "Go to tab 4", "Tabs and panels"),
    spec("tab.go-5", "Go to tab 5", "Tabs and panels"),
    spec("tab.go-6", "Go to tab 6", "Tabs and panels"),
    spec("tab.go-7", "Go to tab 7", "Tabs and panels"),
    spec("tab.go-8", "Go to tab 8", "Tabs and panels"),
    spec("tab.go-9", "Go to tab 9", "Tabs and panels"),
    spec("tab.next", "Next tab", "Tabs and panels"),
    spec("tab.previous", "Previous tab", "Tabs and panels"),
    spec(
        "sidebar.files.toggle",
        "Toggle file sidebar",
        "Tabs and panels",
    ),
    spec("sidebar.files.show", "Show file sidebar", "Tabs and panels"),
    spec("sidebar.files.hide", "Hide file sidebar", "Tabs and panels"),
    spec("file-tree.focus", "Focus file tree", "Tabs and panels"),
    spec(
        "pane.focus-left",
        "Focus pane on the left",
        "Tabs and panels",
    ),
    spec(
        "pane.focus-right",
        "Focus pane on the right",
        "Tabs and panels",
    ),
    spec("pane.focus-up", "Focus pane above", "Tabs and panels"),
    spec("pane.focus-down", "Focus pane below", "Tabs and panels"),
    spec(
        "pane.move-tab-left",
        "Move tab to the pane on the left",
        "Tabs and panels",
    ),
    spec(
        "pane.move-tab-right",
        "Move tab to the pane on the right",
        "Tabs and panels",
    ),
    spec(
        "pane.move-tab-up",
        "Move tab to the pane above",
        "Tabs and panels",
    ),
    spec(
        "pane.move-tab-down",
        "Move tab to the pane below",
        "Tabs and panels",
    ),
    spec("tab.close-others", "Close other tabs", "Tabs and panels"),
    spec(
        "tab.close-right",
        "Close tabs to the right",
        "Tabs and panels",
    ),
    spec("app.print", "Print", "App"),
    spec("app.export", "Export", "App"),
    spec("sync.now", "Sync now", "App"),
    spec("sync.resolve-conflicts", "Resolve sync conflicts", "App"),
    spec("settings.open", "Open settings", "App"),
    spec("vault.open", "Open another vault", "App"),
    spec("pane.split-right", "Split right", "Tabs and panels"),
    spec("pane.split-down", "Split down", "Tabs and panels"),
    spec("pane.close", "Close pane", "Tabs and panels"),
    spec("view.zoom-in", "Make text bigger", "View"),
    spec("view.zoom-out", "Make text smaller", "View"),
    spec("view.zoom-reset", "Reset text size", "View"),
    spec(
        "view.toggle-readable-width",
        "Toggle readable line length",
        "View",
    ),
    spec(
        "file-tree.reveal-active",
        "Show the current note in the file tree",
        "Notes and navigation",
    ),
    spec("note.rename", "Rename note", "Notes and navigation"),
    spec("note.delete", "Move note to trash", "Notes and navigation"),
    spec("note.import-image", "Insert image from file", "Editing"),
    spec("edit.paste-plain", "Paste as plain text", "Editing"),
    key_only("cursor.left", "Move left", "Cursor"),
    key_only("cursor.right", "Move right", "Cursor"),
    key_only("cursor.up", "Move up", "Cursor"),
    key_only("cursor.down", "Move down", "Cursor"),
    key_only("cursor.word-left", "Move to the previous word", "Cursor"),
    key_only("cursor.word-right", "Move to the next word", "Cursor"),
    key_only(
        "cursor.line-start",
        "Move to the start of the line",
        "Cursor",
    ),
    key_only("cursor.line-end", "Move to the end of the line", "Cursor"),
    key_only(
        "cursor.doc-start",
        "Move to the start of the note",
        "Cursor",
    ),
    key_only("cursor.doc-end", "Move to the end of the note", "Cursor"),
    key_only("cursor.page-up", "Move up a page", "Cursor"),
    key_only("cursor.page-down", "Move down a page", "Cursor"),
    key_only("select.left", "Select left", "Cursor"),
    key_only("select.right", "Select right", "Cursor"),
    key_only("select.up", "Select up", "Cursor"),
    key_only("select.down", "Select down", "Cursor"),
    key_only("select.word-left", "Select to the previous word", "Cursor"),
    key_only("select.word-right", "Select to the next word", "Cursor"),
    key_only(
        "select.line-start",
        "Select to the start of the line",
        "Cursor",
    ),
    key_only("select.line-end", "Select to the end of the line", "Cursor"),
    key_only(
        "select.doc-start",
        "Select to the start of the note",
        "Cursor",
    ),
    key_only("select.doc-end", "Select to the end of the note", "Cursor"),
    key_only("select.page-up", "Select up a page", "Cursor"),
    key_only("select.page-down", "Select down a page", "Cursor"),
    spec("select.all", "Select all", "Editing"),
    key_only(
        "edit.delete-backward",
        "Delete the previous character",
        "Editing",
    ),
    key_only(
        "edit.delete-forward",
        "Delete the next character",
        "Editing",
    ),
    key_only(
        "edit.delete-word-backward",
        "Delete the previous word",
        "Editing",
    ),
    key_only(
        "edit.delete-word-forward",
        "Delete the next word",
        "Editing",
    ),
    spec(
        "edit.delete-to-line-start",
        "Delete to the start of the line",
        "Editing",
    ),
    spec(
        "edit.delete-to-line-end",
        "Delete to the end of the line",
        "Editing",
    ),
    key_only("edit.newline", "New line", "Editing"),
    spec("edit.indent", "Indent", "Editing"),
    spec("edit.move-line-up", "Move line up", "Editing"),
    spec("edit.move-line-down", "Move line down", "Editing"),
    spec("edit.duplicate-line", "Duplicate line", "Editing"),
    spec("edit.toggle-task", "Toggle task", "Editing"),
    spec("edit.outdent", "Outdent", "Editing"),
    spec("edit.undo", "Undo", "Editing"),
    spec("edit.redo", "Redo", "Editing"),
    spec("edit.copy", "Copy", "Editing"),
    spec("edit.cut", "Cut", "Editing"),
    spec("edit.paste", "Paste", "Editing"),
];

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
