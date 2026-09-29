use std::process::ExitCode;

use gasp_config::COMMAND_NAME;
use gasp_desktop::app::{has_display, launch, launch_bench};
use gasp_desktop::bench::BenchConfig;
use gasp_desktop::cli::{self, Command, USAGE};
use gasp_desktop::note::{self, LONG_NOTE_LINES};
use gasp_desktop::trace;
use gasp_desktop::workspace::state::{AppState, migrate_app_folders};
use gasp_desktop::workspace::window::LaunchTarget;

fn main() -> ExitCode {
    trace::init();
    env_logger::init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = match cli::parse(&args) {
        Ok(command) => command,
        Err(message) => {
            eprintln!("{message}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    // A snapshot only looks, so it leaves the app's folders as they are.
    if !matches!(command, Command::Snapshot(_) | Command::Help) {
        migrate_app_folders();
    }
    match command {
        Command::Help => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Command::Open(path) => open(path.as_deref()),
        Command::Bench { path, config } => bench(&path, config),
        Command::BenchIndex(vault) => {
            println!("{}", gasp_desktop::knowledge::bench::run(&vault));
            ExitCode::SUCCESS
        }
        Command::Mcp(vault) => mcp(vault),
        Command::Snapshot(request) => snapshot(request),
    }
}

/// `gasp mcp`: serves the vault until the client hangs up. Nothing
/// here starts GPUI, so the server answers within milliseconds.
fn mcp(vault: Option<std::path::PathBuf>) -> ExitCode {
    let Some(vault) = vault.or_else(AppState::last_vault) else {
        eprintln!("{COMMAND_NAME} mcp: no vault given and none opened before; pass its folder");
        return ExitCode::from(2);
    };
    if !vault.is_dir() {
        eprintln!("{COMMAND_NAME} mcp: {} isn't a folder", vault.display());
        return ExitCode::from(2);
    }
    match gasp_mcp::serve_stdio(&vault) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{COMMAND_NAME} mcp: {error}");
            ExitCode::from(1)
        }
    }
}

/// `gasp --snapshot`: exits from inside the app once the PNG is written,
/// so it only returns when something went wrong first.
fn snapshot(request: gasp_desktop::snapshot::SnapshotRequest) -> ExitCode {
    match gasp_desktop::snapshot::run(request) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{COMMAND_NAME} --snapshot: {error}");
            ExitCode::from(1)
        }
    }
}

fn open(path: Option<&std::path::Path>) -> ExitCode {
    let resolved = {
        let _span = trace::span("resolve-target");
        LaunchTarget::resolve(path, AppState::last_vault())
    };
    let target = match resolved {
        Ok(target) => target,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };
    if !has_display() {
        eprintln!("no display: set DISPLAY or WAYLAND_DISPLAY, or run under xvfb-run");
        return ExitCode::from(2);
    }
    launch(target);
    ExitCode::SUCCESS
}

fn bench(path: &std::path::Path, config: BenchConfig) -> ExitCode {
    if !has_display() {
        eprintln!("no display: set DISPLAY or WAYLAND_DISPLAY, or run under xvfb-run");
        return ExitCode::from(2);
    }
    match note::load(path, LONG_NOTE_LINES) {
        Ok(note) => {
            launch_bench(note, config);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("could not read {}: {error}", path.display());
            ExitCode::from(1)
        }
    }
}
