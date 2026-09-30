use std::process::ExitCode;

use gasp_config::COMMAND_NAME;
use gasp_desktop::app::{has_display, launch, launch_bench, launch_install};
use gasp_desktop::bench::BenchConfig;
use gasp_desktop::cli::{self, Command, USAGE};
use gasp_desktop::install;
use gasp_desktop::note::{self, LONG_NOTE_LINES};
use gasp_desktop::trace;
use gasp_desktop::workspace::state::{AppState, migrate_app_folders};
use gasp_desktop::workspace::window::LaunchTarget;

fn main() -> ExitCode {
    trace::init();
    env_logger::init();
    migrate_app_folders();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = match cli::parse(&args) {
        Ok(command) => command,
        Err(message) => {
            eprintln!("{message}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match command {
        Command::Help => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Command::Open {
            path,
            just_installed,
        } => open(path.as_deref(), just_installed),
        Command::ShowInstall { dark } => show_install(dark),
        Command::Bench { path, config } => bench(&path, config),
        Command::BenchIndex(vault) => {
            println!("{}", gasp_desktop::knowledge::bench::run(&vault));
            ExitCode::SUCCESS
        }
        Command::Mcp(vault) => mcp(vault),
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

fn open(path: Option<&std::path::Path>, just_installed: bool) -> ExitCode {
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
    // Opened plainly from outside Applications, as from the disk image,
    // it offers to move there first.
    let offer = (path.is_none() && !just_installed)
        .then(install::offer_at_launch)
        .flatten();
    match offer {
        Some(placement) => launch_install(placement, target, None),
        None => launch(target),
    }
    ExitCode::SUCCESS
}

/// `--show-install`: the install window wherever the app is.
fn show_install(dark: Option<bool>) -> ExitCode {
    if !has_display() {
        eprintln!("no display: set DISPLAY or WAYLAND_DISPLAY, or run under xvfb-run");
        return ExitCode::from(2);
    }
    let target =
        LaunchTarget::resolve(None, AppState::last_vault()).unwrap_or(LaunchTarget::Welcome);
    launch_install(install::preview_placement(), target, dark);
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
