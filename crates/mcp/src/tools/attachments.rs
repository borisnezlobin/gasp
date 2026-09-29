//! Tools on attachments: every file in the vault that isn't a note, such
//! as images and PDFs. Contents travel as base64.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use editor_vault::files::atomic_write_bytes;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use super::files::{self, Kind, make_parent, move_entry, trash_entry};
use super::notes::{ListArgs, MoveArgs, PathArgs};
use crate::context::Context;
use crate::tool::{Output, ToolError, ToolResult, ToolSpec};

/// The largest attachment `read_attachment` returns, so one call can't
/// flood the model's context.
pub const MAX_READ_BYTES: u64 = 5 * 1024 * 1024;

pub fn tools() -> Vec<ToolSpec> {
    vec![
        ToolSpec::reads(
            "list_attachments",
            "List the vault's files that aren't notes (images, PDFs and the rest), sorted by \
             path, with size and modification time (Unix seconds).",
            list_attachments,
        ),
        ToolSpec::reads(
            "read_attachment",
            "Read an attachment. Images come back as images; anything else as base64. Files \
             over 5 MB are refused.",
            read_attachment,
        ),
        ToolSpec::writes(
            "write_attachment",
            "Write an attachment from base64, replacing any file at the path only when \
             `overwrite` is true. Folders are made as needed.",
            write_attachment,
        ),
        ToolSpec::writes(
            "move_attachment",
            "Move or rename an attachment. Links and embeds pointing at it are rewritten when \
             the files.update-links-on-rename setting is on.",
            move_attachment,
        ),
        ToolSpec::writes(
            "delete_attachment",
            "Delete an attachment to the system trash or the vault's .trash folder, as the \
             files.trash setting says. Nothing is ever deleted for good.",
            delete_attachment,
        ),
    ]
}

fn list_attachments(context: &Context, args: ListArgs) -> ToolResult {
    let listing = files::list(
        context,
        Kind::Attachments,
        args.folder.as_deref(),
        args.glob.as_deref(),
        args.limit,
    )?;
    Ok(Output::Json(listing))
}

/// Image types a client can show, by extension.
const IMAGE_TYPES: &[(&str, &str)] = &[
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
];

fn image_type(path: &str) -> Option<&'static str> {
    let extension = path.rsplit_once('.')?.1.to_ascii_lowercase();
    IMAGE_TYPES
        .iter()
        .find(|(known, _)| *known == extension)
        .map(|(_, mime)| *mime)
}

fn read_attachment(context: &Context, args: PathArgs) -> ToolResult {
    let path = context.resolve_attachment(&args.path)?;
    let metadata =
        std::fs::metadata(&path.absolute).map_err(|error| ToolError::io(&path.relative, &error))?;
    if !metadata.is_file() {
        return Err(ToolError::new(format!("{} isn't a file", path.relative)));
    }
    if metadata.len() > MAX_READ_BYTES {
        return Err(ToolError::new(format!(
            "{} is {} bytes, over the {MAX_READ_BYTES}-byte limit for reading",
            path.relative,
            metadata.len()
        )));
    }
    let data =
        std::fs::read(&path.absolute).map_err(|error| ToolError::io(&path.relative, &error))?;
    if let Some(mime) = image_type(&path.relative) {
        let caption = format!("{} ({} bytes)", path.relative, data.len());
        return Ok(Output::Image {
            data,
            mime,
            caption,
        });
    }
    Ok(Output::Json(json!({
        "path": path.relative,
        "size": data.len(),
        "base64": STANDARD.encode(&data),
    })))
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct WriteAttachment {
    /// Where it goes, relative to the vault, such as `images/chart.png`.
    path: String,
    /// The file's bytes, base64-encoded.
    base64: String,
    /// Whether to replace a file already at `path`.
    #[serde(default)]
    overwrite: bool,
}

fn write_attachment(context: &Context, args: WriteAttachment) -> ToolResult {
    let path = context.resolve_attachment(&args.path)?;
    if path.absolute.symlink_metadata().is_ok() && !args.overwrite {
        return Err(ToolError::new(format!(
            "{} already exists; set `overwrite` to replace it",
            path.relative
        )));
    }
    let data = STANDARD
        .decode(args.base64.trim())
        .map_err(|error| ToolError::new(format!("`base64` isn't valid base64: {error}")))?;
    make_parent(&path)?;
    atomic_write_bytes(&path.absolute, &data)
        .map_err(|error| ToolError::io(&path.relative, &error))?;
    Ok(Output::Text(format!(
        "Wrote {} ({} bytes).",
        path.relative,
        data.len()
    )))
}

fn move_attachment(context: &Context, args: MoveArgs) -> ToolResult {
    let from = context.resolve_attachment(&args.from)?;
    let to = context.resolve_attachment(&args.to)?;
    Ok(Output::Json(move_entry(context, &from, &to)?))
}

fn delete_attachment(context: &Context, args: PathArgs) -> ToolResult {
    let path = context.resolve_attachment(&args.path)?;
    Ok(Output::Json(trash_entry(context, &path)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::testing::{call, call_err, run, text, vault};

    #[test]
    fn attachments_are_listed_apart_from_notes() {
        let (_dir, context) = vault(&[("A.md", "a"), ("images/b.png", "b"), ("doc.pdf", "c")]);
        let listing = call(&context, "list_attachments", json!({}));
        assert_eq!(listing["total"], 2);
        assert_eq!(listing["files"][0]["path"], "doc.pdf");
        assert_eq!(listing["files"][1]["size"], 1);
    }

    #[test]
    fn reading_gives_images_as_images_and_the_rest_as_base64() {
        let (_dir, context) = vault(&[("images/b.png", "png!"), ("doc.pdf", "pdf"), ("A.md", "")]);
        let Ok(Output::Image { data, mime, .. }) =
            run(&context, "read_attachment", json!({"path": "images/b.png"}))
        else {
            panic!("expected an image");
        };
        assert_eq!((data.as_slice(), mime), (&b"png!"[..], "image/png"));
        let pdf = call(&context, "read_attachment", json!({"path": "doc.pdf"}));
        assert_eq!(pdf["base64"], STANDARD.encode("pdf"));
        let error = call_err(&context, "read_attachment", json!({"path": "A.md"}));
        assert!(error.contains("is a note"));
        let error = call_err(&context, "read_attachment", json!({"path": "gone.png"}));
        assert!(error.contains("doesn't exist"));
    }

    #[test]
    fn large_files_are_refused() {
        let (dir, context) = vault(&[]);
        let file = std::fs::File::create(dir.path().join("big.bin")).unwrap();
        file.set_len(MAX_READ_BYTES + 1).unwrap();
        let error = call_err(&context, "read_attachment", json!({"path": "big.bin"}));
        assert!(error.contains("limit"), "{error}");
    }

    #[test]
    fn write_move_and_delete() {
        let (dir, context) = vault(&[("Note.md", "![[a.png]] and ![](img/a.png)\n")]);
        let encoded = STANDARD.encode([1u8, 2, 3]);
        text(
            &context,
            "write_attachment",
            json!({"path": "img/a.png", "base64": encoded}),
        );
        assert_eq!(
            std::fs::read(dir.path().join("img/a.png")).unwrap(),
            [1, 2, 3]
        );
        let error = call_err(
            &context,
            "write_attachment",
            json!({"path": "img/a.png", "base64": encoded}),
        );
        assert!(error.contains("overwrite"));
        let error = call_err(
            &context,
            "write_attachment",
            json!({"path": "x.png", "base64": "***"}),
        );
        assert!(error.contains("isn't valid base64"));
        let moved = call(
            &context,
            "move_attachment",
            json!({"from": "img/a.png", "to": "img/chart.png"}),
        );
        assert_eq!(moved["updated_notes"], json!(["Note.md"]));
        let note = std::fs::read_to_string(dir.path().join("Note.md")).unwrap();
        assert_eq!(note, "![[chart.png]] and ![](img/chart.png)\n");
        let settings = editor_config::store::settings_path(dir.path());
        std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
        std::fs::write(settings, "[files]\ntrash = \"vault\"\n").unwrap();
        call(
            &context,
            "delete_attachment",
            json!({"path": "img/chart.png"}),
        );
        assert!(dir.path().join(".trash/chart.png").exists());
    }
}
