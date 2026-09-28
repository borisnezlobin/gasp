//! What the note and attachment tools share: listing files, writing a
//! note through the app when it has unsaved edits, moving with link
//! updates, and deleting to a trash.

use std::path::Path;
use std::time::UNIX_EPOCH;

use editor_config::settings::TrashMode;
use editor_vault::files::atomic_write;
use editor_vault::ops;
use globset::{Glob, GlobMatcher};
use serde_json::{Value, json};

use crate::bridge::Request;
use crate::bridge::client::AppError;
use crate::context::Context;
use crate::paths::{VaultPath, is_note_name, resolve_folder};
use crate::tool::ToolError;

/// Files a listing returns when no limit is given.
pub const DEFAULT_LIST_LIMIT: usize = 1000;

/// Which files a listing wants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Notes,
    Attachments,
}

/// Visible files under `folder` matching `glob`, by path, with their size
/// and modification time.
pub fn list(
    context: &Context,
    kind: Kind,
    folder: Option<&str>,
    glob: Option<&str>,
    limit: Option<usize>,
) -> Result<Value, ToolError> {
    let folder = resolve_folder(context.root(), folder)?;
    if !folder.absolute.is_dir() {
        return Err(ToolError::new(format!(
            "{} isn't a folder in the vault",
            folder.relative
        )));
    }
    let matcher = glob.map(compile_glob).transpose()?;
    let prefix = match folder.relative.as_str() {
        "" => String::new(),
        relative => format!("{relative}/"),
    };
    let mut paths: Vec<String> = ops::vault_files(&folder.absolute)
        .into_iter()
        .filter(|path| is_note_name(path) == (kind == Kind::Notes))
        .map(|path| format!("{prefix}{path}"))
        .filter(|path| matcher.as_ref().is_none_or(|glob| glob.is_match(path)))
        .collect();
    paths.sort();
    let total = paths.len();
    let limit = limit.unwrap_or(DEFAULT_LIST_LIMIT);
    let files: Vec<Value> = paths
        .iter()
        .take(limit)
        .map(|path| file_entry(context.root(), path))
        .collect();
    Ok(json!({ "total": total, "truncated": total > limit, "files": files }))
}

fn compile_glob(pattern: &str) -> Result<GlobMatcher, ToolError> {
    Glob::new(pattern)
        .map(|glob| glob.compile_matcher())
        .map_err(|error| ToolError::new(format!("{pattern:?} isn't a glob: {error}")))
}

fn file_entry(root: &Path, path: &str) -> Value {
    let metadata = std::fs::metadata(root.join(path)).ok();
    let modified = metadata
        .as_ref()
        .and_then(|meta| meta.modified().ok())
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|since| since.as_secs());
    json!({
        "path": path,
        "size": metadata.map(|meta| meta.len()),
        "modified": modified,
    })
}

/// Where a write went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Written {
    /// Into the app's open editor, as one undoable edit, then saved.
    InApp,
    /// Straight to the file; an app with it open reloads it.
    OnDisk,
}

impl Written {
    pub fn describe(self) -> &'static str {
        match self {
            Written::InApp => "in the open editor as one undoable edit, and saved",
            Written::OnDisk => "to the file",
        }
    }
}

/// Changes a note's text. When the app has it open with unsaved edits,
/// `change` sees the editor's text and the result goes back through the
/// app, so neither the edits nor undo are lost. Otherwise it sees the
/// file (empty when there's none yet) and the result is written to it.
pub fn change_note(
    context: &Context,
    note: &VaultPath,
    change: impl FnOnce(&str) -> Result<String, String>,
) -> Result<Written, ToolError> {
    let buffer = context
        .app()
        .buffer(&note.relative)
        .map_err(|error| ToolError::new(error.message()))?;
    if let Some(old) = buffer.filter(|b| b.open && b.dirty).and_then(|b| b.text) {
        let new = change(&old).map_err(ToolError::new)?;
        let request = Request::Edit {
            path: note.relative.clone(),
            old,
            new,
        };
        context.app().call(request).map_err(app_error)?;
        return Ok(Written::InApp);
    }
    let old = read_or_empty(note)?;
    let new = change(&old).map_err(ToolError::new)?;
    write_file(note, &new)?;
    Ok(Written::OnDisk)
}

fn read_or_empty(note: &VaultPath) -> Result<String, ToolError> {
    match std::fs::read_to_string(&note.absolute) {
        Ok(text) => Ok(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(ToolError::io(&note.relative, &error)),
    }
}

/// Writes a file atomically, making its folders first.
pub fn write_file(path: &VaultPath, text: &str) -> Result<(), ToolError> {
    make_parent(path)?;
    atomic_write(&path.absolute, text).map_err(|error| ToolError::io(&path.relative, &error))
}

pub fn make_parent(path: &VaultPath) -> Result<(), ToolError> {
    match path.absolute.parent() {
        Some(parent) => {
            std::fs::create_dir_all(parent).map_err(|error| ToolError::io(&path.relative, &error))
        }
        None => Ok(()),
    }
}

pub fn app_error(error: AppError) -> ToolError {
    ToolError::new(error.message())
}

/// Moves a note or file, and rewrites links to it across the vault when
/// `files.update-links-on-rename` is on. With the app running the move
/// goes through it, so open editors change as one undoable edit each.
pub fn move_entry(context: &Context, from: &VaultPath, to: &VaultPath) -> Result<Value, ToolError> {
    if !from.exists() {
        return Err(ToolError::new(format!("{} doesn't exist", from.relative)));
    }
    let request = Request::Move {
        from: from.relative.clone(),
        to: to.relative.clone(),
    };
    match context.app().call(request) {
        Ok(result) => return Ok(json!({ "moved_in_app": true, "result": result })),
        Err(AppError::NotRunning) => {}
        Err(error) => return Err(app_error(error)),
    }
    let update_links = context.settings().files.update_links_on_rename;
    let renamed = ops::rename(
        context.root(),
        Path::new(&from.relative),
        Path::new(&to.relative),
        update_links,
    )
    .map_err(|error| ToolError::new(format!("couldn't move {}: {error}", from.relative)))?;
    let updated: Vec<String> = renamed
        .updated_notes
        .iter()
        .map(|path| ops::slash_path(path))
        .collect();
    Ok(json!({
        "from": from.relative,
        "to": to.relative,
        "links_updated": update_links,
        "updated_notes": updated,
    }))
}

/// Deletes a note or file the way `files.trash` says, except that it's
/// never deleted for good: "delete" puts it in the vault's `.trash`.
/// A note the app has open with unsaved edits is left alone.
pub fn trash_entry(context: &Context, path: &VaultPath) -> Result<Value, ToolError> {
    if !path.exists() {
        return Err(ToolError::new(format!("{} doesn't exist", path.relative)));
    }
    let buffer = context.app().buffer(&path.relative).map_err(app_error)?;
    if buffer.is_some_and(|buffer| buffer.dirty) {
        return Err(ToolError::new(format!(
            "{} has unsaved edits in the app; save or close it first",
            path.relative
        )));
    }
    let mode = match context.settings().files.trash {
        TrashMode::System => TrashMode::System,
        TrashMode::Vault | TrashMode::Delete => TrashMode::Vault,
    };
    let relative = Path::new(&path.relative);
    let used = match ops::trash(context.root(), relative, mode) {
        Ok(()) => mode,
        // No system trash (a server without a desktop): keep it in the vault.
        Err(_) if mode == TrashMode::System => {
            ops::trash(context.root(), relative, TrashMode::Vault)
                .map_err(|error| ToolError::io(&path.relative, &error))?;
            TrashMode::Vault
        }
        Err(error) => return Err(ToolError::io(&path.relative, &error)),
    };
    let place = match used {
        TrashMode::System => "the system trash",
        _ => "the vault's .trash folder",
    };
    Ok(json!({ "deleted": path.relative, "moved_to": place }))
}
