use std::process::ExitCode;

use editor_desktop::app::{has_display, launch, launch_bench};
use editor_desktop::bench::BenchConfig;
use editor_desktop::cli::{self, Command, USAGE};
use editor_desktop::note::{self, LONG_NOTE_LINES};
use editor_desktop::workspace::state::AppState;
use editor_desktop::workspace::window::LaunchTarget;

fn main() -> ExitCode {
    env_logger::init();
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
        Command::Open(path) => open(path.as_deref()),
        Command::Bench { path, config } => bench(&path, config),
        Command::BenchIndex(vault) => {
            println!("{}", editor_desktop::knowledge::bench::run(&vault));
            ExitCode::SUCCESS
        }
    }
}

fn open(path: Option<&std::path::Path>) -> ExitCode {
    let target = match LaunchTarget::resolve(path, AppState::last_vault()) {
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
