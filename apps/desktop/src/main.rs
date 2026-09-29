use std::process::ExitCode;

use gasp_config::COMMAND_NAME;
use gasp_desktop::app::{has_display, launch, launch_bench};
use gasp_desktop::bench::BenchConfig;
use gasp_desktop::cli::{self, Command, USAGE};
use gasp_desktop::note::{self, LONG_NOTE_LINES};
use gasp_desktop::prose::worker_process::{WORKER_ARG, use_worker_process};
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
    // A snapshot only looks, so it leaves the app's folders as they are:
    // never migrate them for one.
    let only_looks = matches!(
        command,
        Command::Snapshot(_)
            | Command::WindowSnapshot(_)
            | Command::BenchOpen { .. }
            | Command::GrammarWorker
            | Command::Help
    );
    if !only_looks {
        migrate_app_folders();
    }
    if draws_editors(&command)
        && let Ok(exe) = std::env::current_exe()
    {
        use_worker_process(exe);
    }
    match command {
        Command::GrammarWorker => grammar_worker(),
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
        Command::BenchOpen { vault, config } => exit_status(
            "--bench-open",
            gasp_desktop::open_bench::run(&vault, config),
        ),
        Command::Mcp(vault) => mcp(vault),
        Command::Snapshot(request) => {
            exit_status("--snapshot", gasp_desktop::snapshot::run(request))
        }
        Command::WindowSnapshot(request) => {
            exit_status("--snapshot", gasp_desktop::snapshot::run_window(request))
        }
    }
}

/// Whether the command opens editors, whose grammar checks then run in a
/// child process.
fn draws_editors(command: &Command) -> bool {
    matches!(
        command,
        Command::Open(_)
            | Command::Bench { .. }
            | Command::BenchOpen { .. }
            | Command::Snapshot(_)
            | Command::WindowSnapshot(_)
    )
}

/// `gasp grammar-worker`: checks what the app sends until it hangs up.
fn grammar_worker() -> ExitCode {
    let input = std::io::stdin().lock();
    let output = std::io::stdout().lock();
    match gasp_desktop::prose::worker_process::serve(input, output) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{COMMAND_NAME} {WORKER_ARG}: {error}");
            ExitCode::from(1)
        }
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

/// `gasp --snapshot` and `--bench-open` exit from inside the app once
/// they're done, so they only return when something went wrong first.
fn exit_status(mode: &str, outcome: Result<(), String>) -> ExitCode {
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{COMMAND_NAME} {mode}: {error}");
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
