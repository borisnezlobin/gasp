use std::process::ExitCode;

use editor_desktop::app::{has_display, launch};
use editor_desktop::cli::{self, Command, USAGE};
use editor_desktop::demo::DEMO_NOTE;
use editor_desktop::note::{self, LONG_NOTE_LINES, LoadedNote};

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
        Command::Help => println!("{USAGE}"),
        Command::Open(path) => return open(path.as_deref(), None),
        Command::Bench { path, config } => return open(Some(&path), Some(config)),
    }
    ExitCode::SUCCESS
}

fn open(
    path: Option<&std::path::Path>,
    bench: Option<editor_desktop::bench::BenchConfig>,
) -> ExitCode {
    if !has_display() {
        eprintln!("no display: set DISPLAY or WAYLAND_DISPLAY, or run under xvfb-run");
        return ExitCode::from(2);
    }
    let note = match path {
        None => LoadedNote {
            text: DEMO_NOTE.to_owned(),
            image_dirs: Vec::new(),
        },
        Some(path) => match note::load(path, LONG_NOTE_LINES) {
            Ok(note) => note,
            Err(error) => {
                eprintln!("could not read {}: {error}", path.display());
                return ExitCode::from(1);
            }
        },
    };
    launch(note, bench);
    ExitCode::SUCCESS
}
