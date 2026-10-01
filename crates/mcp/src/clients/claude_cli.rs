//! Claude Code's `claude` command: adding Gasp's server for the user with
//! `claude mcp add --scope user`, after removing a stale one, and finding
//! the command when the app was opened without the shell's `PATH`.

use std::ffi::OsString;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use super::{ClientError, ClientHome, SERVER_NAME, ServerLaunch};

/// The command's name.
pub const TOOL: &str = "claude";

/// How long `claude mcp add` or `remove` may take.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

/// How long a login shell may take to say what its `PATH` is.
const SHELL_TIMEOUT: Duration = Duration::from_secs(3);

/// How often a running command is checked on.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// Folders every Mac has, for the command's own `PATH`.
const BASE_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";

/// `claude mcp add --scope user gasp -- <binary> mcp <vault>`, without
/// the command itself.
pub fn add_args(launch: &ServerLaunch) -> Vec<String> {
    ["mcp", "add", "--scope", "user", SERVER_NAME, "--"]
        .into_iter()
        .map(str::to_string)
        .chain(std::iter::once(launch.command.clone()))
        .chain(launch.args.iter().cloned())
        .collect()
}

/// `claude mcp remove --scope user gasp`, without the command itself.
pub fn remove_args() -> Vec<String> {
    ["mcp", "remove", "--scope", "user", SERVER_NAME]
        .into_iter()
        .map(str::to_string)
        .collect()
}

/// Adds Gasp's server to Claude Code, removing the old one first when
/// `replacing`.
pub fn connect(
    home: &ClientHome,
    launch: &ServerLaunch,
    replacing: bool,
) -> Result<(), ClientError> {
    let tool = home.find_tool(TOOL).ok_or(ClientError::CliMissing)?;
    let path = search_path(home);
    if replacing {
        run(&tool, &remove_args(), &path)?;
    }
    run(&tool, &add_args(launch), &path)
}

/// A `PATH` holding the folders tools are looked for in, so a `claude`
/// that's a Node script finds `node`.
fn search_path(home: &ClientHome) -> OsString {
    let mut folders = home.tool_dirs();
    folders.extend(std::env::split_paths(BASE_PATH));
    std::env::join_paths(folders).unwrap_or_else(|_| OsString::from(BASE_PATH))
}

fn run(tool: &Path, args: &[String], path: &OsString) -> Result<(), ClientError> {
    let child = Command::new(tool)
        .args(args)
        .env("PATH", path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| ClientError::CliMissing)?;
    match wait_for(child, COMMAND_TIMEOUT) {
        Some(status) if status.success() => Ok(()),
        _ => Err(ClientError::CliFailed),
    }
}

/// The child's exit status, or `None` once `timeout` passes, when it's
/// killed.
fn wait_for(mut child: Child, timeout: Duration) -> Option<ExitStatus> {
    let started = Instant::now();
    while started.elapsed() < timeout {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) => std::thread::sleep(POLL_INTERVAL),
            Err(_) => break,
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    None
}

/// The `PATH` a login shell sets up, for finding tools installed where
/// Gasp doesn't look. Blocks for up to a few seconds, so it runs off the
/// main thread.
pub fn login_shell_path() -> Option<String> {
    let mut child = Command::new("/bin/zsh")
        .args(["-lc", "printf %s \"$PATH\""])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || std::io::read_to_string(stdout).ok());
    let status = wait_for(child, SHELL_TIMEOUT)?;
    let path = reader.join().ok().flatten()?;
    (status.success() && !path.is_empty()).then_some(path)
}
