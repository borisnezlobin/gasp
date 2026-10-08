//! Drives the server as a client would, over in-memory pipes: the
//! handshake, listing the tools and calling one.

use gasp_mcp::Context;
use gasp_mcp::server::Server;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

struct Client {
    to_server: tokio::io::DuplexStream,
    from_server: BufReader<tokio::io::DuplexStream>,
}

impl Client {
    async fn send(&mut self, message: Value) {
        let mut line = serde_json::to_vec(&message).unwrap();
        line.push(b'\n');
        self.to_server.write_all(&line).await.unwrap();
    }

    async fn receive(&mut self) -> Value {
        let mut line = String::new();
        self.from_server.read_line(&mut line).await.unwrap();
        serde_json::from_str(&line).unwrap_or_else(|_| panic!("not JSON: {line:?}"))
    }

    async fn request(&mut self, id: u64, method: &str, params: Value) -> Value {
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
            .await;
        let reply = self.receive().await;
        assert_eq!(reply["id"], id, "{reply}");
        reply
    }

    async fn initialize(&mut self) -> Value {
        let init = self
            .request(
                0,
                "initialize",
                json!({
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": {"name": "test", "version": "1"},
                }),
            )
            .await;
        self.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
            .await;
        init
    }
}

fn start(vault: &std::path::Path) -> Client {
    let context = Context::with_endpoint(vault, None).unwrap();
    let (client_writer, server_reader) = tokio::io::duplex(1 << 20);
    let (server_writer, client_reader) = tokio::io::duplex(1 << 20);
    tokio::spawn(Server::new(context).serve(server_reader, server_writer));
    Client {
        to_server: client_writer,
        from_server: BufReader::new(client_reader),
    }
}

#[tokio::test]
async fn initialize_list_and_call() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Plan.md"), "# Plan\n\nWaves.\n").unwrap();
    let mut client = start(dir.path());

    let init = client.initialize().await;
    assert_eq!(
        init["result"]["serverInfo"]["name"],
        gasp_config::COMMAND_NAME
    );
    assert!(
        init["result"]["capabilities"]["tools"].is_object(),
        "{init}"
    );

    let pong = client.request(2, "ping", json!({})).await;
    assert!(pong["result"].is_object(), "{pong}");

    let list = client.request(3, "tools/list", json!({})).await;
    let tools = list["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    for expected in [
        "list_notes",
        "read_note",
        "patch_note",
        "set_setting",
        "render_note",
        "get_toolbars",
        "set_toolbars",
    ] {
        assert!(
            names.contains(&expected),
            "{expected} missing from {names:?}"
        );
    }
    let read = tools
        .iter()
        .find(|tool| tool["name"] == "read_note")
        .unwrap();
    assert_eq!(read["inputSchema"]["required"], json!(["path"]));
    assert_eq!(read["annotations"]["readOnlyHint"], true);

    let call = client
        .request(
            4,
            "tools/call",
            json!({"name": "read_note", "arguments": {"path": "Plan"}}),
        )
        .await;
    let result = &call["result"];
    assert_ne!(result["isError"], true, "{call}");
    let text: Value = serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(text["text"], "# Plan\n\nWaves.\n");

    // A tool's failure is a result the model reads, not a protocol error.
    let failed = client
        .request(
            5,
            "tools/call",
            json!({"name": "read_note", "arguments": {"path": "../etc/passwd"}}),
        )
        .await;
    assert_eq!(failed["result"]["isError"], true, "{failed}");
    let message = failed["result"]["content"][0]["text"].as_str().unwrap();
    assert!(message.contains("outside the vault"), "{message}");

    let unknown = client
        .request(6, "tools/call", json!({"name": "nope", "arguments": {}}))
        .await;
    assert!(
        unknown["error"]["message"]
            .as_str()
            .unwrap()
            .contains("nope")
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_vault_it_cant_read_is_an_error_not_an_empty_list() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Plan.md"), "# Plan\n").unwrap();
    let mut client = start(dir.path());
    client.initialize().await;
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o000)).unwrap();

    let call = client
        .request(
            1,
            "tools/call",
            json!({"name": "list_notes", "arguments": {}}),
        )
        .await;
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(call["result"]["isError"], true, "{call}");
    let message = call["result"]["content"][0]["text"].as_str().unwrap();
    assert!(message.contains("isn't allowed to read"), "{message}");
}
