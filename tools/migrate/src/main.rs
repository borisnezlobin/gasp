//! `editor-migrate --obsidian <dir> --out <dir>`

use std::path::PathBuf;
use std::process::ExitCode;

use editor_migrate::{REPORT_FILE, migrate_obsidian, render_report};

const USAGE: &str = "usage: editor-migrate --obsidian <.obsidian folder> --out <output folder>";

struct Arguments {
    obsidian: PathBuf,
    out: PathBuf,
}

fn parse_arguments(args: impl Iterator<Item = String>) -> Result<Arguments, String> {
    let mut obsidian = None;
    let mut out = None;
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        let slot = match arg.as_str() {
            "--obsidian" => &mut obsidian,
            "--out" => &mut out,
            "-h" | "--help" => return Err(USAGE.to_string()),
            other => return Err(format!("unknown argument `{other}`\n{USAGE}")),
        };
        let value = args
            .next()
            .ok_or_else(|| format!("`{arg}` needs a folder\n{USAGE}"))?;
        *slot = Some(PathBuf::from(value));
    }
    match (obsidian, out) {
        (Some(obsidian), Some(out)) => Ok(Arguments { obsidian, out }),
        _ => Err(USAGE.to_string()),
    }
}

fn run() -> Result<(), String> {
    let arguments = parse_arguments(std::env::args().skip(1))?;
    let migration = migrate_obsidian(&arguments.obsidian)?;
    let files = migration.write_to(&arguments.out)?;
    for file in &files {
        println!("wrote {}", arguments.out.join(file.name).display());
    }
    println!();
    print!("{}", render_report(&migration));
    println!(
        "\nThe report is also in {}.",
        arguments.out.join(REPORT_FILE).display()
    );
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}
