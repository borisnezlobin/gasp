//! Command-line arguments.

use std::path::PathBuf;

use gasp_config::command_name;

use crate::bench::BenchConfig;
use crate::install::{JUST_INSTALLED_FLAG, SHOW_INSTALL_FLAG};

pub const USAGE: &str = concat!(
    "usage: ",
    command_name!(),
    " [PATH]\n       ",
    command_name!(),
    " --bench-layout PATH [--keystrokes N] [--scroll-pages N] [--in-code] [--in-math] [--in-table] [--no-prose]\n       ",
    command_name!(),
    " --bench-index VAULT\n       ",
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
    /// Opens a vault, a note or the last vault. `just_installed` is set
    /// when the install window opened this copy, which then doesn't ask
    /// to move again.
    Open {
        path: Option<PathBuf>,
        just_installed: bool,
    },
    /// `--show-install`, hidden: the install window wherever the app
    /// is, for testing and screenshots, in light or dark when asked.
    ShowInstall {
        dark: Option<bool>,
    },
    Bench {
        path: PathBuf,
        config: BenchConfig,
    },
    BenchIndex(PathBuf),
    /// `gasp mcp`: the MCP server on stdio for a vault, or the last one.
    Mcp(Option<PathBuf>),
    Help,
}

/// Parses arguments after the program name.
pub fn parse(args: &[String]) -> Result<Command, String> {
    match args.first().map(String::as_str) {
        None => Ok(Command::open(None)),
        Some("-h" | "--help") => Ok(Command::Help),
        Some(JUST_INSTALLED_FLAG) => parse_just_installed(&args[1..]),
        Some(SHOW_INSTALL_FLAG) => parse_show_install(&args[1..]),
        Some("--bench-layout") => parse_bench(&args[1..]),
        Some("--bench-index") => match &args[1..] {
            [vault] => Ok(Command::BenchIndex(PathBuf::from(vault))),
            _ => Err("--bench-index needs one vault".to_owned()),
        },
        Some("mcp") => parse_mcp(&args[1..]),
        Some(flag) if flag.starts_with("--") => Err(format!("unknown option {flag}")),
        Some(path) if args.len() == 1 => Ok(Command::open(Some(PathBuf::from(path)))),
        Some(_) => Err("expected one path".to_owned()),
    }
}

impl Command {
    fn open(path: Option<PathBuf>) -> Command {
        Command::Open {
            path,
            just_installed: false,
        }
    }
}

fn parse_just_installed(args: &[String]) -> Result<Command, String> {
    match parse(args)? {
        Command::Open { path, .. } => Ok(Command::Open {
            path,
            just_installed: true,
        }),
        _ => Err(format!("{JUST_INSTALLED_FLAG} only goes with opening")),
    }
}

fn parse_show_install(args: &[String]) -> Result<Command, String> {
    let dark = match args {
        [] => None,
        [mode] if mode == "light" => Some(false),
        [mode] if mode == "dark" => Some(true),
        _ => return Err(format!("{SHOW_INSTALL_FLAG} takes light or dark")),
    };
    Ok(Command::ShowInstall { dark })
}

fn parse_mcp(args: &[String]) -> Result<Command, String> {
    match args {
        [] => Ok(Command::Mcp(None)),
        [vault] => Ok(Command::Mcp(Some(PathBuf::from(vault)))),
        _ => Err("mcp takes at most one vault".to_owned()),
    }
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
        assert_eq!(parse(&[]), Ok(Command::open(None)));
    }

    #[test]
    fn a_path_opens_it() {
        assert_eq!(
            parse(&args(&["notes/a.md"])),
            Ok(Command::open(Some(PathBuf::from("notes/a.md"))))
        );
    }

    #[test]
    fn the_moved_copy_opens_without_asking_again() {
        assert_eq!(
            parse(&args(&["--installed"])),
            Ok(Command::Open {
                path: None,
                just_installed: true
            })
        );
        assert!(parse(&args(&["--installed", "mcp"])).is_err());
    }

    #[test]
    fn show_install_takes_an_appearance() {
        assert_eq!(
            parse(&args(&["--show-install"])),
            Ok(Command::ShowInstall { dark: None })
        );
        assert_eq!(
            parse(&args(&["--show-install", "dark"])),
            Ok(Command::ShowInstall { dark: Some(true) })
        );
        assert!(parse(&args(&["--show-install", "dim"])).is_err());
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
    fn rejects_bad_arguments() {
        assert!(parse(&args(&["--bench-layout"])).is_err());
        assert!(parse(&args(&["--bench-layout", "x", "--keystrokes"])).is_err());
        assert!(parse(&args(&["--bench-layout", "x", "--keystrokes", "many"])).is_err());
        assert!(parse(&args(&["--frobnicate"])).is_err());
        assert!(parse(&args(&["a", "b"])).is_err());
        assert_eq!(parse(&args(&["--help"])), Ok(Command::Help));
    }
}
