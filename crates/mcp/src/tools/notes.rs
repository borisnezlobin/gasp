//! Tools on notes: listing, reading, searching, creating, writing,
//! patching, moving and deleting.

use std::path::Path;
use std::sync::atomic::AtomicUsize;

use gasp_search::engine::{self, NoteResult};
use gasp_search::tags::{search_tagged, tag_query};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::files::{self, Kind, change_note, move_entry, trash_entry, write_file};
use crate::context::Context;
use crate::frontmatter;
use crate::patch::{self, Patch};
use crate::tool::{Output, ToolError, ToolResult, ToolSpec};

pub fn tools() -> Vec<ToolSpec> {
    vec![
        ToolSpec::reads(
            "list_notes",
            concat!(
                "List the vault's notes (Markdown files), sorted by path, with size and \
                 modification time (Unix seconds). Hidden folders such as .git, ",
                gasp_config::config_dir!(),
                " and .trash are never listed."
            ),
            list_notes,
        ),
        ToolSpec::reads(
            "read_note",
            "Read a note: its text (or a range of lines) and its frontmatter parsed as \
             JSON. If the app has the note open with unsaved edits, this is the editor's \
             text.",
            read_note,
        ),
        ToolSpec::reads(
            "search",
            "Search every note's text, ignoring case and accents, ranked as the app's vault \
             search ranks (file name, folder, headings, tags, then body). `tag:name` or \
             `#name` finds notes tagged name or a tag under it. Returns paths with the \
             matching lines.",
            search,
        ),
        ToolSpec::writes(
            "create_note",
            "Create a note. Fails if one already exists at the path. Folders are made as \
             needed.",
            create_note,
        ),
        ToolSpec::writes(
            "write_note",
            "Replace a note's whole text, creating the note if it doesn't exist. Prefer \
             patch_note for small changes. If the app has the note open with unsaved edits, \
             the change goes into its editor as one undoable edit.",
            write_note,
        ),
        ToolSpec::writes(
            "patch_note",
            "Change part of a note. `replace` swaps exact text (`find`) for `content`, and \
             fails unless `find` matches exactly `expected_count` times (default 1). \
             `append` and `prepend` add `content` at the end or start of the note (after \
             frontmatter), or of a heading's section when `heading` names one; name a \
             repeated heading through its parents, as `Parent::Child`.",
            patch_note,
        ),
        ToolSpec::writes(
            "move_note",
            "Move or rename a note. Links to it across the vault are rewritten, as a rename \
             in the app does, when the files.update-links-on-rename setting is on.",
            move_note,
        ),
        ToolSpec::writes(
            "delete_note",
            "Delete a note to the system trash or the vault's .trash folder, as the \
             files.trash setting says. Nothing is ever deleted for good.",
            delete_note,
        ),
    ]
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListArgs {
    /// A folder to list, relative to the vault; the whole vault if left out.
    #[serde(default)]
    pub folder: Option<String>,
    /// A glob on vault-relative paths, such as `Projects/**` or `*Plan*` (`*` also
    /// matches across folders).
    #[serde(default)]
    pub glob: Option<String>,
    /// The most paths to return; 1000 if left out.
    #[serde(default)]
    pub limit: Option<usize>,
}

fn list_notes(context: &Context, args: ListArgs) -> ToolResult {
    let listing = files::list(
        context,
        Kind::Notes,
        args.folder.as_deref(),
        args.glob.as_deref(),
        args.limit,
    )?;
    Ok(Output::Json(listing))
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ReadNote {
    /// The note, relative to the vault; `.md` may be left off.
    path: String,
    /// The first line to return, counting from 1.
    #[serde(default)]
    start_line: Option<usize>,
    /// The last line to return, inclusive.
    #[serde(default)]
    end_line: Option<usize>,
}

fn read_note(context: &Context, args: ReadNote) -> ToolResult {
    let note = context.resolve_note(&args.path)?;
    let buffer = context.app().buffer(&note.relative).ok().flatten();
    let (text, source) = match buffer.filter(|b| b.dirty).and_then(|b| b.text) {
        Some(text) => (text, "app"),
        None => (
            std::fs::read_to_string(&note.absolute)
                .map_err(|error| ToolError::io(&note.relative, &error))?,
            "disk",
        ),
    };
    let total = text.lines().count();
    let start = args.start_line.unwrap_or(1).max(1);
    let end = args.end_line.unwrap_or(total).min(total);
    if start > end && total > 0 {
        return Err(ToolError::new(format!(
            "lines {start} to {end} are outside the note, which has {total} lines"
        )));
    }
    let shown = if args.start_line.is_none() && args.end_line.is_none() {
        text.clone()
    } else {
        lines(&text, start, end)
    };
    let mut result = json!({
        "path": note.relative,
        "source": source,
        "lines": total,
        "start_line": start,
        "end_line": end,
        "text": shown,
    });
    match frontmatter::parse(&text) {
        Ok(parsed) => result["frontmatter"] = parsed.unwrap_or(Value::Null),
        Err(message) => result["frontmatter_error"] = Value::from(message),
    }
    Ok(Output::Json(result))
}

/// Lines `start..=end` (1-based) of `text`, with their line breaks.
fn lines(text: &str, start: usize, end: usize) -> String {
    text.split_inclusive('\n')
        .skip(start - 1)
        .take(end + 1 - start)
        .collect()
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Search {
    /// What to look for: words matched as typed (not split), or `tag:name`.
    query: String,
    /// The most notes to return; 20 if left out.
    #[serde(default)]
    limit: Option<usize>,
    /// Only notes in this folder, relative to the vault.
    #[serde(default)]
    folder: Option<String>,
}

const DEFAULT_SEARCH_LIMIT: usize = 20;

fn search(context: &Context, args: Search) -> ToolResult {
    let query = args.query.trim();
    if query.is_empty() {
        return Err(ToolError::new("the query is empty"));
    }
    let mut notes = context.search_notes();
    if let Some(folder) = args.folder.as_deref() {
        let folder = crate::paths::resolve_folder(context.root(), Some(folder))?;
        notes.retain(|note| note.path.starts_with(Path::new(&folder.relative)));
    }
    // The app's search stops early when a newer one starts; here there's
    // only ever one.
    let generation = AtomicUsize::new(0);
    let results = match tag_query(query) {
        Some(tag) => {
            let tagged = context.link_index().notes_tagged(tag);
            search_tagged(&notes, tag, &tagged, &generation, 0)
        }
        None => engine::search(&notes, query, &generation, 0),
    };
    let total = results.len();
    let limit = args.limit.unwrap_or(DEFAULT_SEARCH_LIMIT);
    let shown: Vec<Value> = results.iter().take(limit).map(result_json).collect();
    Ok(Output::Json(json!({
        "query": query,
        "total_notes": total,
        "results": shown,
    })))
}

fn result_json(result: &NoteResult) -> Value {
    let lines: Vec<Value> = result
        .hits
        .iter()
        .map(|hit| json!({ "line": hit.line + 1, "text": hit.excerpt }))
        .collect();
    json!({
        "path": gasp_vault::ops::slash_path(&result.path),
        "score": result.score,
        "matches": result.match_count,
        "lines": lines,
    })
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct CreateNote {
    /// Where the note goes, relative to the vault; `.md` is added if left off.
    path: String,
    /// Its text; empty if left out.
    #[serde(default)]
    text: String,
}

fn create_note(context: &Context, args: CreateNote) -> ToolResult {
    let note = context.resolve_note(&args.path)?;
    if note.absolute.symlink_metadata().is_ok() {
        return Err(ToolError::new(format!(
            "{} already exists; use write_note or patch_note to change it",
            note.relative
        )));
    }
    write_file(&note, &args.text)?;
    Ok(Output::Text(format!("Created {}.", note.relative)))
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct WriteNote {
    /// The note, relative to the vault; `.md` may be left off.
    path: String,
    /// The note's whole new text.
    text: String,
}

fn write_note(context: &Context, args: WriteNote) -> ToolResult {
    let note = context.resolve_note(&args.path)?;
    let existed = note.exists();
    let written = change_note(context, &note, |_| Ok(args.text))?;
    let verb = if existed { "Wrote" } else { "Created" };
    Ok(Output::Text(format!(
        "{verb} {} {}.",
        note.relative,
        written.describe()
    )))
}

/// What `patch_note` does.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Operation {
    Replace,
    Append,
    Prepend,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PatchNote {
    /// The note, relative to the vault; `.md` may be left off.
    path: String,
    operation: Operation,
    /// The text to add, or for `replace`, what replaces `find`.
    content: String,
    /// For `replace`: the exact text to find.
    #[serde(default)]
    find: Option<String>,
    /// For `replace`: how many times `find` must match; 1 if left out.
    #[serde(default)]
    expected_count: Option<usize>,
    /// For `append` and `prepend`: the heading whose section gets the text, such as
    /// `Tasks`, `## Tasks` or `Projects::Tasks`.
    #[serde(default)]
    heading: Option<String>,
}

fn patch_note(context: &Context, args: PatchNote) -> ToolResult {
    let note = context.resolve_note(&args.path)?;
    if !note.exists() {
        return Err(ToolError::new(format!(
            "{} doesn't exist; create it with create_note",
            note.relative
        )));
    }
    let patch = patch_for(&args)?;
    let written = change_note(context, &note, |text| patch::apply(text, patch))?;
    Ok(Output::Text(format!(
        "Patched {} {}.",
        note.relative,
        written.describe()
    )))
}

fn patch_for(args: &PatchNote) -> Result<Patch<'_>, ToolError> {
    let heading = args.heading.as_deref();
    match args.operation {
        Operation::Replace => {
            if heading.is_some() {
                return Err(ToolError::new(
                    "`heading` only goes with append and prepend",
                ));
            }
            let find = args
                .find
                .as_deref()
                .ok_or_else(|| ToolError::new("`replace` needs `find`"))?;
            Ok(Patch::Replace {
                find,
                with: &args.content,
                expected: args.expected_count.unwrap_or(1),
            })
        }
        Operation::Append => Ok(Patch::Append {
            content: &args.content,
            heading,
        }),
        Operation::Prepend => Ok(Patch::Prepend {
            content: &args.content,
            heading,
        }),
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MoveArgs {
    /// Where it is now, relative to the vault.
    pub from: String,
    /// Where it goes, relative to the vault.
    pub to: String,
}

fn move_note(context: &Context, args: MoveArgs) -> ToolResult {
    let from = context.resolve_note(&args.from)?;
    let to = context.resolve_note(&args.to)?;
    Ok(Output::Json(move_entry(context, &from, &to)?))
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PathArgs {
    /// Relative to the vault.
    pub path: String,
}

fn delete_note(context: &Context, args: PathArgs) -> ToolResult {
    let note = context.resolve_note(&args.path)?;
    Ok(Output::Json(trash_entry(context, &note)?))
}

#[cfg(test)]
mod tests {
    use gasp_config::CONFIG_DIR;
    use gasp_config::store::{SETTINGS_FILE, settings_path};

    use super::*;
    use crate::tools::testing::{call, call_err, text, vault};

    #[test]
    fn list_and_filter_notes() {
        let settings_file = format!("{CONFIG_DIR}/{SETTINGS_FILE}");
        let (_dir, context) = vault(&[
            ("A.md", "a"),
            ("Projects/Plan.md", "p"),
            ("Projects/Deep/Plan 2.md", "p"),
            ("Projects/image.png", "x"),
            (settings_file.as_str(), ""),
        ]);
        let listing = call(&context, "list_notes", json!({}));
        let paths: Vec<&str> = listing["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|file| file["path"].as_str().unwrap())
            .collect();
        assert_eq!(
            paths,
            ["A.md", "Projects/Deep/Plan 2.md", "Projects/Plan.md"]
        );
        let listing = call(&context, "list_notes", json!({"folder": "Projects/Deep"}));
        assert_eq!(listing["files"][0]["path"], "Projects/Deep/Plan 2.md");
        let listing = call(
            &context,
            "list_notes",
            json!({"glob": "*Plan*", "limit": 1}),
        );
        assert_eq!(listing["total"], 2);
        assert_eq!(listing["truncated"], true);
        let error = call_err(&context, "list_notes", json!({"folder": "../x"}));
        assert!(error.contains("outside the vault"));
        let error = call_err(&context, "list_notes", json!({"glob": "a[b"}));
        assert!(error.contains("isn't a glob"));
    }

    #[test]
    fn read_returns_text_lines_and_frontmatter() {
        let (_dir, context) = vault(&[("Plan.md", "---\ntags: [a]\n---\nOne\nTwo\nThree\n")]);
        let note = call(&context, "read_note", json!({"path": "Plan"}));
        assert_eq!(note["frontmatter"], json!({"tags": ["a"]}));
        assert_eq!(note["lines"], 6);
        assert_eq!(note["source"], "disk");
        let range = call(
            &context,
            "read_note",
            json!({"path": "Plan.md", "start_line": 5, "end_line": 9}),
        );
        assert_eq!(range["text"], "Two\nThree\n");
        assert_eq!(range["end_line"], 6);
        let error = call_err(&context, "read_note", json!({"path": "Missing"}));
        assert!(error.contains("Missing.md") && error.contains("doesn't exist"));
    }

    #[test]
    fn search_ranks_like_the_app_and_finds_tags() {
        let (_dir, context) = vault(&[
            ("Waves.md", "Nothing here.\n"),
            ("Notes.md", "About waves.\nMore waves.\n#physics\n"),
            ("Other.md", "---\ntags: [physics/optics]\n---\nLight.\n"),
        ]);
        let found = call(&context, "search", json!({"query": "WAVES"}));
        assert_eq!(found["results"][0]["path"], "Waves.md");
        assert_eq!(found["results"][1]["path"], "Notes.md");
        assert_eq!(found["results"][1]["matches"], 2);
        assert_eq!(found["results"][1]["lines"][1]["line"], 2);
        let tagged = call(&context, "search", json!({"query": "tag:physics"}));
        assert_eq!(tagged["total_notes"], 2);
        assert!(call_err(&context, "search", json!({"query": " "})).contains("empty"));
    }

    #[test]
    fn create_write_and_patch() {
        let (dir, context) = vault(&[]);
        text(
            &context,
            "create_note",
            json!({"path": "New/Plan", "text": "# Plan\n"}),
        );
        let error = call_err(&context, "create_note", json!({"path": "New/Plan.md"}));
        assert!(error.contains("already exists"));
        let read = |name: &str| std::fs::read_to_string(dir.path().join(name)).unwrap();
        assert_eq!(read("New/Plan.md"), "# Plan\n");
        let said = text(
            &context,
            "patch_note",
            json!({"path": "New/Plan", "operation": "append", "content": "- one", "heading": "Plan"}),
        );
        assert!(said.contains("to the file"), "{said}");
        assert_eq!(read("New/Plan.md"), "# Plan\n- one\n");
        text(
            &context,
            "patch_note",
            json!({"path": "New/Plan", "operation": "replace", "find": "one", "content": "two"}),
        );
        assert_eq!(read("New/Plan.md"), "# Plan\n- two\n");
        let error = call_err(
            &context,
            "patch_note",
            json!({"path": "New/Plan", "operation": "replace", "find": "zzz", "content": "x"}),
        );
        assert!(error.contains("isn't in the note"));
        let error = call_err(
            &context,
            "patch_note",
            json!({"path": "New/Plan", "operation": "replace", "content": "x"}),
        );
        assert!(error.contains("needs `find`"));
        text(
            &context,
            "write_note",
            json!({"path": "New/Plan", "text": "all new"}),
        );
        assert_eq!(read("New/Plan.md"), "all new");
        let error = call_err(
            &context,
            "patch_note",
            json!({"path": "Nope", "operation": "append", "content": "x"}),
        );
        assert!(error.contains("doesn't exist"));
        // Writes never land outside the vault or in hidden folders.
        assert!(
            call_err(&context, "write_note", json!({"path": "../x", "text": ""}))
                .contains("outside")
        );
        assert!(
            call_err(
                &context,
                "write_note",
                json!({"path": ".git/x", "text": ""})
            )
            .contains("hidden")
        );
    }

    #[test]
    fn moving_updates_links_unless_the_setting_is_off() {
        let (dir, context) = vault(&[
            ("Plan.md", "plan"),
            ("Index.md", "See [[Plan]] and [p](Plan.md).\n"),
        ]);
        let moved = call(
            &context,
            "move_note",
            json!({"from": "Plan", "to": "Projects/Big plan"}),
        );
        assert_eq!(moved["updated_notes"], json!(["Index.md"]));
        let index = std::fs::read_to_string(dir.path().join("Index.md")).unwrap();
        assert_eq!(index, "See [[Big plan]] and [p](Projects/Big%20plan.md).\n");
        let error = call_err(&context, "move_note", json!({"from": "Plan", "to": "X"}));
        assert!(error.contains("doesn't exist"));
        let settings = settings_path(dir.path());
        std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
        std::fs::write(settings, "[files]\nupdate-links-on-rename = false\n").unwrap();
        let moved = call(
            &context,
            "move_note",
            json!({"from": "Projects/Big plan", "to": "Plan"}),
        );
        assert_eq!(moved["updated_notes"], json!([]));
        let error = call_err(
            &context,
            "move_note",
            json!({"from": "Plan", "to": "Index"}),
        );
        assert!(error.contains("already here"), "{error}");
    }

    #[test]
    fn deleting_never_deletes_for_good() {
        let (dir, context) = vault(&[("Old.md", "old"), ("Keep.md", "keep")]);
        let settings = settings_path(dir.path());
        std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
        std::fs::write(&settings, "[files]\ntrash = \"delete\"\n").unwrap();
        let deleted = call(&context, "delete_note", json!({"path": "Old"}));
        assert_eq!(deleted["moved_to"], "the vault's .trash folder");
        assert!(!dir.path().join("Old.md").exists());
        assert_eq!(
            std::fs::read_to_string(dir.path().join(".trash/Old.md")).unwrap(),
            "old"
        );
        let error = call_err(&context, "delete_note", json!({"path": "Old"}));
        assert!(error.contains("doesn't exist"));
    }
}
