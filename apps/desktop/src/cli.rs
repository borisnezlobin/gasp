//! Command-line arguments.

use std::path::PathBuf;

use gasp_config::command_name;

use crate::bench::BenchConfig;
use crate::snapshot::SnapshotRequest;

pub const USAGE: &str = concat!(
    "usage: ",
    command_name!(),
    " [PATH]\n       ",
    command_name!(),
    " --bench-layout PATH [--keystrokes N] [--scroll-pages N] [--in-code] [--in-math] [--in-table] [--no-prose]\n       ",
    command_name!(),
    " --bench-index VAULT\n       ",
    command_name!(),
    " --snapshot NOTE OUT.png [--width N] [--height N] [--theme light|dark] [--cursor LINE:COL]\n       ",
    command_name!(),
    " mcp [VAULT]

PATH is a folder of notes (a vault) or a note, which opens its vault
with that note showing. With no PATH, the last vault opens again.

--bench-layout opens a lone editor on PATH (a note, or a folder whose
notes are joined into one long note), types into the middle and scrolls
through it, then prints frame timings and quits. --in-code types in the
first code block after the middle instead, --in-math in the first
math block, where snippets and the math helpers do the most work, and
--in-table in the first body cell of the first table. --no-prose turns sentence
tints and grammar flags off, to measure what they cost. With
EDITOR_TRACE_KEYS=1 it also lists where each keystroke's time went. On
Linux without a display, run it under xvfb-run.

--snapshot draws NOTE's editor, as its vault's theme and settings
show it, into OUT.png without showing a window or taking focus. The
window is 900 by 700 unless --width and --height say otherwise, and the
image is at the display's scale. --cursor puts the caret at a line and
column, counted from 1, with the editor focused. macOS only for now.

--bench-index builds VAULT's link index and prints how long that, a
save, a backlinks list, an unlinked-mentions search and a rename take.

mcp serves VAULT (the last vault when left out) to an agent over MCP on
stdin and stdout. Its tools read and change notes, attachments and the
vault's config whether or not the app is running; with the app open on
the vault, they also see its tabs and cursor and run its commands."
);

/// What the binary was asked to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Open(Option<PathBuf>),
    Bench {
        path: PathBuf,
        config: BenchConfig,
    },
    BenchIndex(PathBuf),
    /// `gasp --snapshot`: a note drawn to a PNG with no window shown.
    Snapshot(SnapshotRequest),
    /// `gasp mcp`: the MCP server on stdio for a vault, or the last one.
    Mcp(Option<PathBuf>),
    Help,
}

/// Parses arguments after the program name.
pub fn parse(args: &[String]) -> Result<Command, String> {
    match args.first().map(String::as_str) {
        None => Ok(Command::Open(None)),
        Some("-h" | "--help") => Ok(Command::Help),
        Some("--bench-layout") => parse_bench(&args[1..]),
        Some("--bench-index") => match &args[1..] {
            [vault] => Ok(Command::BenchIndex(PathBuf::from(vault))),
            _ => Err("--bench-index needs one vault".to_owned()),
        },
        Some("--snapshot") => parse_snapshot(&args[1..]),
        Some("mcp") => parse_mcp(&args[1..]),
        Some(flag) if flag.starts_with("--") => Err(format!("unknown option {flag}")),
        Some(path) if args.len() == 1 => Ok(Command::Open(Some(PathBuf::from(path)))),
        Some(_) => Err("expected one path".to_owned()),
    }
}

fn parse_mcp(args: &[String]) -> Result<Command, String> {
    match args {
        [] => Ok(Command::Mcp(None)),
        [vault] => Ok(Command::Mcp(Some(PathBuf::from(vault)))),
        _ => Err("mcp takes at most one vault".to_owned()),
    }
}

fn parse_snapshot(args: &[String]) -> Result<Command, String> {
    let [note, out, options @ ..] = args else {
        return Err("--snapshot needs a note and a PNG to write".to_owned());
    };
    let mut request = SnapshotRequest::new(PathBuf::from(note), PathBuf::from(out));
    for pair in options.chunks(2) {
        let [flag, value] = pair else {
            return Err(format!("{} needs a value", pair[0]));
        };
        apply_snapshot_option(&mut request, flag, value)?;
    }
    Ok(Command::Snapshot(request))
}

fn apply_snapshot_option(
    request: &mut SnapshotRequest,
    flag: &str,
    value: &str,
) -> Result<(), String> {
    let pixels = || match value.parse::<u32>() {
        Ok(pixels) if pixels >= 1 => Ok(pixels),
        _ => Err(format!("{flag} needs a size in pixels, not {value}")),
    };
    match flag {
        "--width" => request.width = pixels()?,
        "--height" => request.height = pixels()?,
        "--theme" => request.dark = parse_theme(value)?,
        "--cursor" => request.cursor = Some(parse_line_column(value)?),
        _ => return Err(format!("unknown option {flag}")),
    }
    Ok(())
}

/// Whether `--theme` asks for dark.
fn parse_theme(value: &str) -> Result<bool, String> {
    match value {
        "light" => Ok(false),
        "dark" => Ok(true),
        _ => Err(format!("--theme is light or dark, not {value}")),
    }
}

/// `LINE:COL`, both counted from one.
fn parse_line_column(value: &str) -> Result<(usize, usize), String> {
    let wrong = || format!("--cursor needs LINE:COL, such as 12:3, not {value}");
    let (line, column) = value.split_once(':').ok_or_else(wrong)?;
    let number = |text: &str| {
        text.parse::<usize>()
            .ok()
            .filter(|&n| n >= 1)
            .ok_or_else(wrong)
    };
    Ok((number(line)?, number(column)?))
}

fn parse_bench(args: &[String]) -> Result<Command, String> {
    let path = args.first().ok_or("--bench-layout needs a path")?;
    let mut config = BenchConfig::default();
    let mut rest: Vec<String> = args[1..].to_vec();
    if let Some(at) = rest.iter().position(|arg| arg == "--in-code") {
        rest.remove(at);
        config.in_code = true;
    }
    if let Some(at) = rest.iter().position(|arg| arg == "--in-math") {
        rest.remove(at);
        config.in_math = true;
    }
    if let Some(at) = rest.iter().position(|arg| arg == "--in-table") {
        rest.remove(at);
        config.in_table = true;
    }
    if let Some(at) = rest.iter().position(|arg| arg == "--no-prose") {
        rest.remove(at);
        config.prose = false;
    }
    for pair in rest.chunks(2) {
        let [flag, value] = pair else {
            return Err(format!("{} needs a value", pair[0]));
        };
        let count: usize = value
            .parse()
            .map_err(|_| format!("{flag} needs a number, not {value}"))?;
        match flag.as_str() {
            "--keystrokes" => config.keystrokes = count.max(1),
            "--scroll-pages" => config.scroll_pages = count,
            _ => return Err(format!("unknown option {flag}")),
        }
    }
    Ok(Command::Bench {
        path: PathBuf::from(path),
        config,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| (*arg).to_owned()).collect()
    }

    #[test]
    fn no_arguments_reopens_the_last_vault() {
        assert_eq!(parse(&[]), Ok(Command::Open(None)));
    }

    #[test]
    fn a_path_opens_it() {
        assert_eq!(
            parse(&args(&["notes/a.md"])),
            Ok(Command::Open(Some(PathBuf::from("notes/a.md"))))
        );
    }

    #[test]
    fn bench_takes_a_path_and_counts() {
        let parsed = parse(&args(&["--bench-layout", "corpus", "--keystrokes", "50"])).unwrap();
        let Command::Bench { path, config } = parsed else {
            panic!("expected a bench command");
        };
        assert_eq!(path, PathBuf::from("corpus"));
        assert_eq!(config.keystrokes, 50);
        assert_eq!(config.scroll_pages, BenchConfig::default().scroll_pages);
        assert!(!config.in_code);
        let parsed = parse(&args(&[
            "--bench-layout",
            "c",
            "--in-code",
            "--keystrokes",
            "5",
        ]));
        let Ok(Command::Bench { config, .. }) = parsed else {
            panic!("expected a bench command");
        };
        assert!(config.in_code);
        assert_eq!(config.keystrokes, 5);
        assert!(config.prose);
        let parsed = parse(&args(&["--bench-layout", "c", "--no-prose"]));
        let Ok(Command::Bench { config, .. }) = parsed else {
            panic!("expected a bench command");
        };
        assert!(!config.prose);
    }

    #[test]
    fn mcp_takes_an_optional_vault() {
        assert_eq!(parse(&args(&["mcp"])), Ok(Command::Mcp(None)));
        assert_eq!(
            parse(&args(&["mcp", "notes"])),
            Ok(Command::Mcp(Some(PathBuf::from("notes"))))
        );
        assert!(parse(&args(&["mcp", "a", "b"])).is_err());
    }

    #[test]
    fn snapshot_takes_a_note_an_image_and_options() {
        let parsed = parse(&args(&["--snapshot", "a.md", "a.png"]));
        let Ok(Command::Snapshot(request)) = parsed else {
            panic!("expected a snapshot command");
        };
        assert_eq!(request, SnapshotRequest::new("a.md".into(), "a.png".into()));
        let parsed = parse(&args(&[
            "--snapshot",
            "a.md",
            "a.png",
            "--width",
            "600",
            "--theme",
            "dark",
            "--cursor",
            "12:3",
        ]));
        let Ok(Command::Snapshot(request)) = parsed else {
            panic!("expected a snapshot command");
        };
        assert_eq!(request.width, 600);
        assert!(request.dark);
        assert_eq!(request.cursor, Some((12, 3)));
        assert!(parse(&args(&["--snapshot", "a.md"])).is_err());
        assert!(parse(&args(&["--snapshot", "a.md", "a.png", "--cursor", "0:1"])).is_err());
        assert!(parse(&args(&["--snapshot", "a.md", "a.png", "--theme", "sepia"])).is_err());
        assert!(parse(&args(&["--snapshot", "a.md", "a.png", "--height"])).is_err());
    }

    #[test]
    fn rejects_bad_arguments() {
        assert!(parse(&args(&["--bench-layout"])).is_err());
        assert!(parse(&args(&["--bench-layout", "x", "--keystrokes"])).is_err());
        assert!(parse(&args(&["--bench-layout", "x", "--keystrokes", "many"])).is_err());
        assert!(parse(&args(&["--frobnicate"])).is_err());
        assert!(parse(&args(&["a", "b"])).is_err());
        assert_eq!(parse(&args(&["--help"])), Ok(Command::Help));
    }
}
