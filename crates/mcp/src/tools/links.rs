//! Tools on the link graph: backlinks, a note's outgoing links and the
//! vault's tags, from the same index the app's sidebars use.

use editor_vault::index::LinkIndex;
use editor_vault::mentions::link_excerpt;
use editor_vault::parse::Link;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::context::Context;
use crate::paths::VaultPath;
use crate::tool::{Output, ToolError, ToolResult, ToolSpec};

pub fn tools() -> Vec<ToolSpec> {
    vec![
        ToolSpec::reads(
            "backlinks",
            "List the notes that link to a note or attachment, each with the linking lines \
             (1-based) and the link as written, as the app's backlinks sidebar shows them.",
            backlinks,
        ),
        ToolSpec::reads(
            "outgoing_links",
            "List a note's links and embeds in order: the target as written, where it \
             resolves in the vault (null when it goes nowhere), its line, and whether it's an \
             embed.",
            outgoing_links,
        ),
        ToolSpec::reads(
            "tags",
            "List the vault's tags (inline #tags and frontmatter tags) with how many notes \
             carry each; parents of nested tags are counted too. Give `tag` to list the notes \
             carrying it or a tag under it instead.",
            tags,
        ),
    ]
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct LinkTarget {
    /// A note or attachment, relative to the vault; a note's `.md` may be left off.
    path: String,
}

/// A path that exists as given, else as a note.
fn existing(context: &Context, path: &str) -> Result<VaultPath, ToolError> {
    let given = context.resolve(path)?;
    if given.absolute.is_file() {
        return Ok(given);
    }
    let note = context.resolve_note(path)?;
    if note.absolute.is_file() {
        return Ok(note);
    }
    Err(ToolError::new(format!("{} doesn't exist", given.relative)))
}

fn backlinks(context: &Context, args: LinkTarget) -> ToolResult {
    let target = existing(context, &args.path)?;
    let index = context.link_index();
    let sources: Vec<Value> = index
        .backlinks(&target.relative)
        .iter()
        .map(|backlink| {
            let text = index.note(backlink.source).map_or("", |note| &note.text);
            let links: Vec<Value> = backlink
                .links
                .iter()
                .map(|link| link_json(text, link))
                .collect();
            json!({ "path": backlink.source, "links": links })
        })
        .collect();
    Ok(Output::Json(json!({
        "path": target.relative,
        "total_notes": sources.len(),
        "notes": sources,
    })))
}

fn outgoing_links(context: &Context, args: LinkTarget) -> ToolResult {
    let note = context.resolve_note(&args.path)?;
    if !note.absolute.is_file() {
        return Err(ToolError::new(format!("{} doesn't exist", note.relative)));
    }
    let index = context.link_index();
    let entry = index
        .note(&note.relative)
        .ok_or_else(|| ToolError::new(format!("{} isn't a note the index reads", note.relative)))?;
    let links: Vec<Value> = entry
        .parsed
        .links
        .iter()
        .zip(&entry.resolved)
        .map(|(link, resolved)| {
            let mut value = link_json(&entry.text, link);
            value["target"] = Value::from(link.target.as_str());
            value["subpath"] = json!(link.subpath);
            value["embed"] = Value::from(link.embed);
            value["resolved"] = json!(resolved);
            value
        })
        .collect();
    Ok(Output::Json(
        json!({ "path": note.relative, "links": links }),
    ))
}

/// Where a link is and how it reads.
fn link_json(text: &str, link: &Link) -> Value {
    let start = link.range.start.min(text.len());
    let line = text[..start].matches('\n').count() + 1;
    json!({
        "line": line,
        "link": text.get(link.range.clone()).unwrap_or_default(),
        "context": link_excerpt(text, link).text,
    })
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Tags {
    /// A tag, without `#`, to list the notes carrying it or a tag under it.
    #[serde(default)]
    tag: Option<String>,
}

fn tags(context: &Context, args: Tags) -> ToolResult {
    let index = context.link_index();
    match args
        .tag
        .as_deref()
        .map(|tag| tag.trim().trim_start_matches('#'))
    {
        Some(tag) if !tag.is_empty() => Ok(Output::Json(tagged(&index, tag))),
        Some(_) => Err(ToolError::new("the tag is empty")),
        None => {
            let tags: Vec<Value> = index
                .tags()
                .iter()
                .map(|tag| json!({ "tag": tag.name, "notes": tag.notes }))
                .collect();
            Ok(Output::Json(json!({ "tags": tags })))
        }
    }
}

fn tagged(index: &LinkIndex, tag: &str) -> Value {
    let mut notes: Vec<String> = index.notes_tagged(tag).into_iter().collect();
    notes.sort();
    json!({ "tag": tag, "notes": notes })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::testing::{call, call_err, vault};

    fn linked_vault() -> (tempfile::TempDir, Context) {
        vault(&[
            ("Plan.md", "# Plan\n#project/big\n"),
            (
                "Index.md",
                "Intro\nSee [[Plan|the plan]] and ![[chart.png]].\n[[Missing]]\n",
            ),
            ("Daily.md", "---\ntags: [project]\n---\n[p](Plan.md)\n"),
            ("images/chart.png", "png"),
        ])
    }

    #[test]
    fn backlinks_list_the_linking_lines() {
        let (_dir, context) = linked_vault();
        let found = call(&context, "backlinks", json!({"path": "Plan"}));
        assert_eq!(found["total_notes"], 2);
        assert_eq!(found["notes"][0]["path"], "Daily.md");
        assert_eq!(found["notes"][0]["links"][0]["line"], 4);
        assert_eq!(found["notes"][1]["links"][0]["link"], "[[Plan|the plan]]");
        let chart = call(&context, "backlinks", json!({"path": "images/chart.png"}));
        assert_eq!(chart["notes"][0]["path"], "Index.md");
        let error = call_err(&context, "backlinks", json!({"path": "Nope"}));
        assert!(error.contains("doesn't exist"), "{error}");
    }

    #[test]
    fn outgoing_links_say_where_they_go() {
        let (_dir, context) = linked_vault();
        let links = call(&context, "outgoing_links", json!({"path": "Index"}));
        let links = links["links"].as_array().unwrap();
        assert_eq!(links.len(), 3);
        assert_eq!(links[0]["resolved"], "Plan.md");
        assert_eq!(links[0]["line"], 2);
        assert_eq!(links[1]["embed"], true);
        assert_eq!(links[1]["resolved"], "images/chart.png");
        assert_eq!(links[2]["target"], "Missing");
        assert_eq!(links[2]["resolved"], Value::Null);
        let error = call_err(&context, "outgoing_links", json!({"path": "../x"}));
        assert!(error.contains("outside the vault"));
    }

    #[test]
    fn tags_count_notes_and_list_them() {
        let (_dir, context) = linked_vault();
        let all = call(&context, "tags", json!({}));
        assert_eq!(all["tags"][0], json!({"tag": "project", "notes": 2}));
        assert_eq!(all["tags"][1], json!({"tag": "project/big", "notes": 1}));
        let tagged = call(&context, "tags", json!({"tag": "#project"}));
        assert_eq!(tagged["notes"], json!(["Daily.md", "Plan.md"]));
        assert!(call_err(&context, "tags", json!({"tag": " "})).contains("empty"));
    }
}
