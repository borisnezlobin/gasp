//! Codex's `config.toml`: servers by name under `[mcp_servers.<name>]`,
//! each with a `command` and its `args`. Edited with `toml_edit`, so the
//! file's comments and layout survive.

use toml_edit::{Array, DocumentMut, Item, Table, TableLike, Value};

use super::{ClientError, Connection, SERVER_NAME, ServerLaunch};

const SERVERS_KEY: &str = "mcp_servers";

/// Whether the config `text` (`None` for no file) starts `launch`.
pub fn connection(text: Option<&str>, launch: &ServerLaunch) -> Connection {
    let Ok(document) = parse(text) else {
        return Connection::Unreadable;
    };
    let Some(entry) = document
        .get(SERVERS_KEY)
        .and_then(|servers| servers.get(SERVER_NAME))
    else {
        return Connection::NotConnected;
    };
    let command = entry.get("command").and_then(Item::as_str);
    if launch.is_started_by(command, entry_args(entry)) {
        Connection::Connected
    } else {
        Connection::Stale
    }
}

/// `text` with `[mcp_servers.gasp]` set to `launch`. Everything else,
/// comments included, stays as it was, and so do other keys of an
/// existing `gasp` table, such as its `env`.
pub fn with_server(text: Option<&str>, launch: &ServerLaunch) -> Result<String, ClientError> {
    let mut document = parse(text)?;
    let servers = table_like_entry(document.as_table_mut(), SERVERS_KEY, true)?;
    if servers
        .get(SERVER_NAME)
        .is_some_and(|entry| !entry.is_table_like())
    {
        servers.remove(SERVER_NAME);
    }
    let entry = table_like_entry(servers, SERVER_NAME, false)?;
    set_value(entry, "command", Value::from(launch.command.as_str()));
    set_value(
        entry,
        "args",
        Value::from(launch.args.iter().collect::<Array>()),
    );
    Ok(document.to_string())
}

/// Sets `key` to `new`, keeping the comments and spacing around an
/// existing value.
fn set_value(table: &mut dyn TableLike, key: &str, mut new: Value) {
    match table.get_mut(key).and_then(Item::as_value_mut) {
        Some(old) => {
            *new.decor_mut() = old.decor().clone();
            *old = new;
        }
        None => {
            table.insert(key, Item::Value(new));
        }
    }
}

fn parse(text: Option<&str>) -> Result<DocumentMut, ClientError> {
    text.unwrap_or_default()
        .parse::<DocumentMut>()
        .map_err(|_| ClientError::Unreadable)
}

/// The table under `key` in `parent`, made when it's missing. An
/// `implicit` table gets no header of its own, as `[mcp_servers]` needs
/// none when only its subtables are written.
fn table_like_entry<'a>(
    parent: &'a mut dyn TableLike,
    key: &str,
    implicit: bool,
) -> Result<&'a mut dyn TableLike, ClientError> {
    let item = parent.entry(key).or_insert_with(|| {
        let mut table = Table::new();
        table.set_implicit(implicit);
        Item::Table(table)
    });
    item.as_table_like_mut().ok_or(ClientError::Unreadable)
}

fn entry_args(entry: &Item) -> Option<Vec<&str>> {
    entry
        .get("args")
        .and_then(Item::as_array)?
        .iter()
        .map(|arg| arg.as_str())
        .collect()
}
