//! `render_note`: a page of a note as PDF export lays it out, as a PNG,
//! so an agent can see what an export or a change to a note looks like.

use editor_export::pdf::{PageImage, PdfOptions, compile_note, evict_memory, fonts_for};
use editor_export::pdf::{render_page, typst_source};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::context::Context;
use crate::tool::{Output, ToolError, ToolResult, ToolSpec};

/// How wide a page is drawn when no width is asked for.
const DEFAULT_WIDTH: u32 = 1000;
const MAX_WIDTH: u32 = 2400;

pub fn tools() -> Vec<ToolSpec> {
    vec![ToolSpec::reads(
        "render_note",
        "Render one page of a note as the app's PDF export lays it out (Typst: math, \
         footnotes, images, tables), as a PNG image. Pages count from 1.",
        render_note,
    )]
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RenderNote {
    /// The note, relative to the vault; `.md` may be left off.
    path: String,
    /// The page, counting from 1; the first if left out.
    #[serde(default)]
    page: Option<usize>,
    /// The image's width in pixels, up to 2400; 1000 if left out.
    #[serde(default)]
    width: Option<u32>,
}

fn render_note(context: &Context, args: RenderNote) -> ToolResult {
    let note = context.resolve_note(&args.path)?;
    let text = std::fs::read_to_string(&note.absolute)
        .map_err(|error| ToolError::io(&note.relative, &error))?;
    let options = PdfOptions::default();
    let source = typst_source(&text, Some(&note.absolute), Some(context.root()), &options);
    let compiled = compile_note(&source, &fonts_for(&options))
        .map_err(|error| ToolError::new(format!("{} didn't lay out: {error}", note.relative)));
    // Typst memoizes every layout; a long-lived server keeps none of it.
    evict_memory(0);
    let compiled = compiled?;
    let pages = compiled.document.pages();
    let number = args.page.unwrap_or(1);
    let page = number
        .checked_sub(1)
        .and_then(|index| pages.get(index))
        .ok_or_else(|| {
            ToolError::new(format!(
                "{} has {} pages, not a page {number}",
                note.relative,
                pages.len()
            ))
        })?;
    let width = args.width.unwrap_or(DEFAULT_WIDTH).clamp(100, MAX_WIDTH);
    let data = encode_png(&render_page(page, width))?;
    let mut caption = format!("{}, page {number} of {}", note.relative, pages.len());
    if !compiled.replaced_math.is_empty() {
        let count = compiled.replaced_math.len();
        caption.push_str(&format!(
            "; {count} equations didn't typeset and show as LaTeX"
        ));
    }
    Ok(Output::Image {
        data,
        mime: "image/png",
        caption,
    })
}

fn encode_png(image: &PageImage) -> Result<Vec<u8>, ToolError> {
    let mut data = Vec::new();
    let mut encoder = png::Encoder::new(&mut data, image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut writer| writer.write_image_data(&image.rgba))
        .map_err(|error| ToolError::new(format!("couldn't encode the page as PNG: {error}")))?;
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::testing::{call_err, run, vault};
    use serde_json::json;

    #[test]
    fn a_page_renders_as_png() {
        let (_dir, context) =
            vault(&[("Plan.md", "# Plan\n\nSome $x^2$ math.[^1]\n\n[^1]: Note.\n")]);
        let output = run(
            &context,
            "render_note",
            json!({"path": "Plan", "width": 300}),
        );
        let Ok(Output::Image {
            data,
            mime,
            caption,
        }) = output
        else {
            panic!("expected an image, got {output:?}");
        };
        assert_eq!(mime, "image/png");
        assert_eq!(&data[1..4], b"PNG");
        assert_eq!(caption, "Plan.md, page 1 of 1");
        let error = call_err(&context, "render_note", json!({"path": "Plan", "page": 3}));
        assert!(error.contains("has 1 pages"), "{error}");
        let error = call_err(&context, "render_note", json!({"path": "Gone"}));
        assert!(error.contains("doesn't exist"));
    }
}
