//! `gasp-corpus`: writes the synthetic vault to a folder.

use std::path::PathBuf;
use std::process::ExitCode;

use gasp_corpus::{DEFAULT_SEED, Options, generate};

const USAGE: &str = "usage: gasp-corpus [--out <dir>] [--seed <u64>] [--notes <n>]\n\
defaults: --out fixtures/corpus --seed 20260927 --notes 204";

struct Args {
    out: PathBuf,
    seed: u64,
    notes: usize,
}

fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut parsed = Args {
        out: PathBuf::from("fixtures/corpus"),
        seed: DEFAULT_SEED,
        notes: Options::default().notes,
    };
    while let Some(flag) = args.next() {
        if flag == "--help" || flag == "-h" {
            return Err(USAGE.to_string());
        }
        let value = args
            .next()
            .ok_or_else(|| format!("{flag} needs a value\n{USAGE}"))?;
        match flag.as_str() {
            "--out" => parsed.out = PathBuf::from(value),
            "--seed" => parsed.seed = value.parse().map_err(|_| format!("bad --seed {value}"))?,
            "--notes" => {
                parsed.notes = value.parse().map_err(|_| format!("bad --notes {value}"))?
            }
            _ => return Err(format!("unknown argument {flag}\n{USAGE}")),
        }
    }
    Ok(parsed)
}

fn main() -> ExitCode {
    let args = match parse_args(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    let vault = generate(args.seed, &Options { notes: args.notes });
    if let Err(error) = vault.write_to(&args.out) {
        eprintln!("gasp-corpus: {error}");
        return ExitCode::FAILURE;
    }
    println!(
        "wrote {} notes and {} attachments to {}",
        vault.notes.len(),
        vault.attachments.len(),
        args.out.display()
    );
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(items: &[&str]) -> Result<Args, String> {
        parse_args(items.iter().map(|s| s.to_string()))
    }

    #[test]
    fn defaults() {
        let args = parse(&[]).unwrap();
        assert_eq!(args.seed, DEFAULT_SEED);
        assert_eq!(args.notes, 204);
    }

    #[test]
    fn flags() {
        let args = parse(&["--out", "x", "--seed", "7", "--notes", "10"]).unwrap();
        assert_eq!(
            (args.out, args.seed, args.notes),
            (PathBuf::from("x"), 7, 10)
        );
    }

    #[test]
    fn errors() {
        assert!(parse(&["--seed"]).is_err());
        assert!(parse(&["--seed", "x"]).is_err());
        assert!(parse(&["--what", "1"]).is_err());
    }
}
