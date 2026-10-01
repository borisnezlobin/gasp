use std::fs;
use std::path::Path;

use serde_json::{Value, json};
use tempfile::TempDir;

use super::*;

fn launch() -> ServerLaunch {
    ServerLaunch::new(
        Path::new("/Applications/Gasp.app/Contents/MacOS/gasp"),
        Path::new("/Users/me/Notes"),
    )
}

fn other_vault() -> ServerLaunch {
    ServerLaunch::new(
        Path::new("/Applications/Gasp.app/Contents/MacOS/gasp"),
        Path::new("/Users/me/Work"),
    )
}

fn parsed(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

fn gasp_entry() -> Value {
    json!({
        "command": "/Applications/Gasp.app/Contents/MacOS/gasp",
        "args": ["mcp", "/Users/me/Notes"],
    })
}

mod json {
    use super::*;
    use crate::clients::json_config::{connection, with_server};

    #[test]
    fn a_missing_or_empty_file_gets_just_the_server() {
        for text in [None, Some(""), Some("  \n"), Some("{}")] {
            let written = with_server(text, &launch()).unwrap();
            assert_eq!(
                parsed(&written),
                json!({ "mcpServers": { "gasp": gasp_entry() } })
            );
            assert_eq!(connection(Some(&written), &launch()), Connection::Connected);
        }
    }

    #[test]
    fn other_servers_and_keys_keep_their_values_and_order() {
        let text = r#"{
  "zeta": true,
  "mcpServers": {
    "filesystem": { "command": "npx", "args": ["-y", "server"], "env": { "A": "1" } }
  },
  "alpha": [1, 2.5, "three"]
}"#;
        let written = with_server(Some(text), &launch()).unwrap();
        let mut expected = parsed(text);
        expected["mcpServers"]["gasp"] = gasp_entry();
        assert_eq!(parsed(&written), expected);
        let order: Vec<&str> = [
            "\"zeta\"",
            "\"mcpServers\"",
            "\"filesystem\"",
            "\"gasp\"",
            "\"alpha\"",
        ]
        .into_iter()
        .collect();
        let positions: Vec<usize> = order.iter().map(|key| written.find(key).unwrap()).collect();
        assert!(positions.is_sorted(), "keys moved:\n{written}");
    }

    #[test]
    fn writes_two_space_indents_and_a_final_newline() {
        let written = with_server(None, &launch()).unwrap();
        assert!(written.starts_with("{\n  \"mcpServers\": {\n    \"gasp\": {"));
        assert!(written.ends_with("}\n"));
    }

    #[test]
    fn a_stale_entry_is_repointed_and_keeps_its_other_keys() {
        let text = r#"{"mcpServers":{"gasp":{"command":"/old/gasp","args":["mcp","/Users/me/Work"],"env":{"X":"y"}}}}"#;
        assert_eq!(connection(Some(text), &launch()), Connection::Stale);
        let written = with_server(Some(text), &launch()).unwrap();
        let entry = &parsed(&written)["mcpServers"]["gasp"];
        assert_eq!(entry["command"], gasp_entry()["command"]);
        assert_eq!(entry["args"], gasp_entry()["args"]);
        assert_eq!(entry["env"], json!({ "X": "y" }));
        assert_eq!(connection(Some(&written), &launch()), Connection::Connected);
    }

    #[test]
    fn another_vault_reads_as_stale() {
        let written = with_server(None, &other_vault()).unwrap();
        assert_eq!(connection(Some(&written), &launch()), Connection::Stale);
    }

    #[test]
    fn no_gasp_entry_reads_as_not_connected() {
        let text = r#"{"mcpServers":{"other":{"command":"x"}}}"#;
        assert_eq!(connection(Some(text), &launch()), Connection::NotConnected);
        assert_eq!(connection(None, &launch()), Connection::NotConnected);
    }

    #[test]
    fn invalid_or_misshapen_json_is_refused() {
        for text in [
            "{ \"mcpServers\": ",
            "[1, 2]",
            "{\"mcpServers\": []}",
            "// a comment\n{}",
        ] {
            assert_eq!(
                with_server(Some(text), &launch()),
                Err(ClientError::Unreadable)
            );
        }
        assert_eq!(
            connection(Some("{ nope"), &launch()),
            Connection::Unreadable
        );
    }
}

mod toml {
    use super::*;
    use crate::clients::toml_config::{connection, with_server};

    const CODEX: &str = "\
# Codex settings
model = \"gpt-5\" # the default

[mcp_servers.context7]
command = \"npx\"
args = [\"-y\", \"@upstash/context7-mcp\"]

# Keep this one last.
[profiles.fast]
model = \"mini\"
";

    #[test]
    fn a_missing_or_empty_file_gets_just_the_server() {
        for text in [None, Some("")] {
            let written = with_server(text, &launch()).unwrap();
            assert_eq!(
                written,
                "[mcp_servers.gasp]\ncommand = \"/Applications/Gasp.app/Contents/MacOS/gasp\"\nargs = [\"mcp\", \"/Users/me/Notes\"]\n"
            );
            assert_eq!(connection(Some(&written), &launch()), Connection::Connected);
        }
    }

    #[test]
    fn comments_and_other_tables_survive() {
        let written = with_server(Some(CODEX), &launch()).unwrap();
        let block = "[mcp_servers.gasp]\ncommand = \"/Applications/Gasp.app/Contents/MacOS/gasp\"\nargs = [\"mcp\", \"/Users/me/Notes\"]\n";
        assert!(written.contains(block), "no server:\n{written}");
        assert_eq!(
            written.replacen(&format!("\n{block}"), "", 1),
            CODEX,
            "changed:\n{written}"
        );
        assert_eq!(connection(Some(&written), &launch()), Connection::Connected);
        assert_eq!(connection(Some(CODEX), &launch()), Connection::NotConnected);
    }

    #[test]
    fn a_stale_entry_is_repointed_in_place() {
        let text = "\
[mcp_servers.gasp]
# Pinned by hand.
command = \"/old/gasp\"
args = [\"mcp\", \"/Users/me/Work\"]
env = { X = \"y\" }
";
        assert_eq!(connection(Some(text), &launch()), Connection::Stale);
        let written = with_server(Some(text), &launch()).unwrap();
        assert!(written.contains("# Pinned by hand."));
        assert!(written.contains("env = { X = \"y\" }"));
        assert!(!written.contains("/old/gasp"));
        assert_eq!(connection(Some(&written), &launch()), Connection::Connected);
        assert_eq!(
            connection(Some(&written), &other_vault()),
            Connection::Stale
        );
    }

    #[test]
    fn invalid_or_misshapen_toml_is_refused() {
        for text in ["[mcp_servers", "mcp_servers = 3"] {
            assert_eq!(
                with_server(Some(text), &launch()),
                Err(ClientError::Unreadable)
            );
        }
        assert_eq!(
            connection(Some("[mcp_servers"), &launch()),
            Connection::Unreadable
        );
    }

    #[test]
    fn a_gasp_key_that_isnt_a_table_is_replaced() {
        let text = "[mcp_servers]\ngasp = \"x\"\n";
        assert_eq!(connection(Some(text), &launch()), Connection::Stale);
        let written = with_server(Some(text), &launch()).unwrap();
        assert_eq!(connection(Some(&written), &launch()), Connection::Connected);
    }
}

mod apps {
    use super::*;

    fn home() -> (TempDir, ClientHome) {
        let dir = tempfile::tempdir().unwrap();
        let home = ClientHome::in_folder(dir.path());
        (dir, home)
    }

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn only_installed_apps_are_found() {
        let (dir, home) = home();
        assert!(ClientApp::installed(&home).is_empty());
        fs::create_dir_all(dir.path().join("Applications/Claude.app")).unwrap();
        fs::create_dir_all(dir.path().join(".codex")).unwrap();
        assert_eq!(
            ClientApp::installed(&home),
            vec![ClientApp::ClaudeDesktop, ClientApp::Codex]
        );
        write(&dir.path().join(".local/bin/claude"), "");
        fs::create_dir_all(dir.path().join(".cursor")).unwrap();
        assert_eq!(ClientApp::installed(&home), ClientApp::ALL.to_vec());
    }

    #[test]
    fn only_apps_with_a_bundle_have_one() {
        let (dir, home) = home();
        fs::create_dir_all(dir.path().join(".cursor")).unwrap();
        assert_eq!(ClientApp::Cursor.app_bundle(&home), None);
        let claude = dir.path().join("Applications/Claude.app");
        fs::create_dir_all(&claude).unwrap();
        assert_eq!(ClientApp::ClaudeDesktop.app_bundle(&home), Some(claude));
        assert_eq!(ClientApp::ClaudeCode.app_bundle(&home), None);
    }

    #[test]
    fn a_tool_on_the_login_path_is_found() {
        let (dir, mut home) = home();
        write(&dir.path().join("elsewhere/codex"), "");
        assert!(!ClientApp::Codex.is_installed(&home));
        let path = format!("relative:{}", dir.path().join("elsewhere").display());
        home.add_search_path(&path);
        assert!(ClientApp::Codex.is_installed(&home));
    }

    #[test]
    fn connecting_claude_creates_its_folder_and_file() {
        let (_dir, home) = home();
        let app = ClientApp::ClaudeDesktop;
        assert_eq!(app.connection(&home, &launch()), Connection::NotConnected);
        app.connect(&home, &launch()).unwrap();
        assert_eq!(app.connection(&home, &launch()), Connection::Connected);
        assert_eq!(app.connection(&home, &other_vault()), Connection::Stale);
        let folder = app.config_path(&home).parent().unwrap().to_path_buf();
        assert_eq!(
            fs::read_dir(folder).unwrap().count(),
            1,
            "a temporary file stayed"
        );
    }

    #[test]
    fn an_invalid_file_is_left_exactly_as_it_was() {
        let (_dir, home) = home();
        let path = ClientApp::Cursor.config_path(&home);
        write(&path, "{ \"mcpServers\": { oops");
        assert_eq!(
            ClientApp::Cursor.connection(&home, &launch()),
            Connection::Unreadable
        );
        assert_eq!(
            ClientApp::Cursor.connect(&home, &launch()),
            Err(ClientError::Unreadable)
        );
        assert_eq!(
            fs::read_to_string(path).unwrap(),
            "{ \"mcpServers\": { oops"
        );
    }

    #[test]
    fn codex_keeps_its_comments_on_disk() {
        let (_dir, home) = home();
        let path = ClientApp::Codex.config_path(&home);
        write(&path, "# mine\nmodel = \"o3\"\n");
        ClientApp::Codex.connect(&home, &launch()).unwrap();
        let text = fs::read_to_string(path).unwrap();
        assert!(text.starts_with("# mine\nmodel = \"o3\"\n"));
        assert_eq!(
            ClientApp::Codex.connection(&home, &launch()),
            Connection::Connected
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_file_stays_linked_and_keeps_its_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, home) = home();
        let real = dir.path().join("dotfiles/mcp.json");
        write(&real, "{}");
        fs::set_permissions(&real, fs::Permissions::from_mode(0o600)).unwrap();
        let path = ClientApp::Cursor.config_path(&home);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&real, &path).unwrap();
        ClientApp::Cursor.connect(&home, &launch()).unwrap();
        assert!(
            fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(fs::read_to_string(&real).unwrap().contains("\"gasp\""));
        let mode = fs::metadata(&real).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn claude_code_is_read_from_its_user_servers() {
        let (_dir, home) = home();
        let path = ClientApp::ClaudeCode.config_path(&home);
        let entry = json!({ "type": "stdio", "command": launch().command, "args": launch().args, "env": {} });
        write(
            &path,
            &json!({ "projects": {}, "mcpServers": { "gasp": entry } }).to_string(),
        );
        assert_eq!(
            ClientApp::ClaudeCode.connection(&home, &launch()),
            Connection::Connected
        );
        assert_eq!(
            ClientApp::ClaudeCode.connection(&home, &other_vault()),
            Connection::Stale
        );
        write(&path, "not json");
        assert_eq!(
            ClientApp::ClaudeCode.connection(&home, &launch()),
            Connection::NotConnected
        );
    }

    #[test]
    fn claude_code_without_its_command_says_so() {
        let (_dir, home) = home();
        assert_eq!(
            ClientApp::ClaudeCode.connect(&home, &launch()),
            Err(ClientError::CliMissing)
        );
    }

    /// A stand-in `claude` that logs each call's arguments, one call per
    /// line, and fails when asked to.
    #[cfg(unix)]
    fn fake_claude(home: &Path, exit: i32) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let log = home.join("calls.log");
        let script = format!(
            "#!/bin/sh\necho \"$*\" >> '{}'\nexit {exit}\n",
            log.display()
        );
        let tool = home.join(".local/bin/claude");
        write(&tool, &script);
        fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).unwrap();
        log
    }

    #[cfg(unix)]
    #[test]
    fn claude_code_adds_for_the_user_and_removes_a_stale_entry_first() {
        let (dir, home) = home();
        let log = fake_claude(dir.path(), 0);
        ClientApp::ClaudeCode.connect(&home, &launch()).unwrap();
        let stale =
            json!({ "mcpServers": { "gasp": { "command": "/old/gasp", "args": ["mcp", "/x"] } } });
        write(
            &ClientApp::ClaudeCode.config_path(&home),
            &stale.to_string(),
        );
        ClientApp::ClaudeCode.connect(&home, &launch()).unwrap();
        let add = "mcp add --scope user gasp -- /Applications/Gasp.app/Contents/MacOS/gasp mcp /Users/me/Notes";
        let expected = format!("{add}\nmcp remove --scope user gasp\n{add}\n");
        assert_eq!(fs::read_to_string(log).unwrap(), expected);
    }

    #[cfg(unix)]
    #[test]
    fn claude_code_refusing_is_reported() {
        let (dir, home) = home();
        fake_claude(dir.path(), 1);
        assert_eq!(
            ClientApp::ClaudeCode.connect(&home, &launch()),
            Err(ClientError::CliFailed)
        );
    }
}
