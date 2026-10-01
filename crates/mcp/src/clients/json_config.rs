//! The JSON config Claude and Cursor share: servers by name under
//! `mcpServers`, each with a `command` and its `args`. Claude Code's
//! `~/.claude.json` keeps its user servers the same way.

use serde_json::{Map, Value};

use super::{ClientError, Connection, SERVER_NAME, ServerLaunch};

const SERVERS_KEY: &str = "mcpServers";

/// Whether the config `text` (`None` for no file) starts `launch`.
pub fn connection(text: Option<&str>, launch: &ServerLaunch) -> Connection {
    let Ok(root) = parse(text) else {
        return Connection::Unreadable;
    };
    let Some(entry) = root
        .get(SERVERS_KEY)
        .and_then(|servers| servers.get(SERVER_NAME))
    else {
        return Connection::NotConnected;
    };
    if launch.is_started_by(entry_command(entry), entry_args(entry)) {
        Connection::Connected
    } else {
        Connection::Stale
    }
}

/// `text` with the `gasp` server set to `launch`. Every other key keeps
/// its value and place, and so do any other keys of an existing `gasp`
/// entry, such as its `env`. The result is indented by two spaces.
pub fn with_server(text: Option<&str>, launch: &ServerLaunch) -> Result<String, ClientError> {
    let mut root = parse(text)?;
    let servers = root
        .entry(SERVERS_KEY)
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or(ClientError::Unreadable)?;
    let entry = servers
        .entry(SERVER_NAME)
        .or_insert_with(|| Value::Object(Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(Map::new());
    }
    if let Some(entry) = entry.as_object_mut() {
        entry.insert("command".to_string(), Value::from(launch.command.clone()));
        entry.insert("args".to_string(), Value::from(launch.args.clone()));
    }
    let mut pretty =
        serde_json::to_string_pretty(&Value::Object(root)).map_err(|_| ClientError::Unreadable)?;
    pretty.push('\n');
    Ok(pretty)
}

/// The config's top-level object: empty for no file or a blank one.
fn parse(text: Option<&str>) -> Result<Map<String, Value>, ClientError> {
    let text = text.unwrap_or_default();
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str(text) {
        Ok(Value::Object(root)) => Ok(root),
        _ => Err(ClientError::Unreadable),
    }
}

fn entry_command(entry: &Value) -> Option<&str> {
    entry.get("command").and_then(Value::as_str)
}

fn entry_args(entry: &Value) -> Option<Vec<&str>> {
    entry
        .get("args")
        .and_then(Value::as_array)?
        .iter()
        .map(Value::as_str)
        .collect()
}
