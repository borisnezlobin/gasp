//! Command-line arguments.

use std::path::PathBuf;

use crate::bench::BenchConfig;

pub const USAGE: &str = "\
usage: editor [PATH]
       editor --bench-layout PATH [--keystrokes N] [--scroll-pages N] [--in-code]
       editor --bench-index VAULT

PATH is a folder of notes (a vault) or a note, which opens its vault
with that note showing. With no PATH, the last vault opens again.

--bench-layout opens a lone editor on PATH (a note, or a folder whose
notes are joined into one long note), types into the middle and scrolls
through it, then prints frame timings and quits. --in-code types in the
first code block after the middle instead. On Linux without a display,
run it under xvfb-run.

--bench-index builds VAULT's link index and prints how long that, a
save, a backlinks list, an unlinked-mentions search and a rename take.";

/// What the binary was asked to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Open(Option<PathBuf>),
    Bench { path: PathBuf, config: BenchConfig },
    BenchIndex(PathBuf),
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
        Some(flag) if flag.starts_with("--") => Err(format!("unknown option {flag}")),
        Some(path) if args.len() == 1 => Ok(Command::Open(Some(PathBuf::from(path)))),
        Some(_) => Err("expected one path".to_owned()),
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
