//! The script a whole-window snapshot follows: one step per line.
//!
//! ```text
//! size 1200x800
//! open "Notes/Lemma.md"
//! command settings.open            # any registry command
//! move 320 540                     # the pointer, in window points
//! wait 300ms
//! click 320 540 cmd                # modifiers are optional
//! keys cmd-shift-p "abc" enter
//! hover-element "settings-control-accent"
//! snap accent-before
//! bounds "settings-text-accent"
//! ```
//!
//! Words are split at spaces; a double-quoted word may hold spaces, `\"`
//! and `\\`, and `#` outside quotes starts a comment. [`STEPS`] lists
//! every step with what it takes.

use std::time::Duration;

use gpui::Modifiers;

/// Every step and what it takes, for errors and the documentation.
pub const STEPS: &str = "\
size WxH
theme light|dark
open NOTE
command ID
move X Y
click X Y [MODS]
double-click X Y [MODS]
right-click X Y [MODS]
drag X1 Y1 X2 Y2 [MODS]
scroll X Y DY
hover-element SELECTOR
click-element SELECTOR [MODS]
double-click-element SELECTOR [MODS]
right-click-element SELECTOR [MODS]
keys KEY|\"TEXT\"...
wait DURATION
settle
snap NAME
snap-now NAME
bounds SELECTOR
selectors [PREFIX]";

/// Where a pointer step happens.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// A point in window points from the window's top-left corner.
    At(f32, f32),
    /// The centre of the element with this debug selector.
    Element(String),
}

/// What the pointer does at a target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerAction {
    Move,
    Click,
    DoubleClick,
    RightClick,
}

/// One key press or a run of typed text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyInput {
    /// A key with modifiers, as the keymap writes it: `cmd-shift-p`.
    Stroke(String),
    /// Text typed a character at a time.
    Text(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Size {
        width: u32,
        height: u32,
    },
    Theme {
        dark: bool,
    },
    Open(String),
    Command(String),
    Pointer {
        action: PointerAction,
        target: Target,
        modifiers: Modifiers,
    },
    Drag {
        from: (f32, f32),
        to: (f32, f32),
        modifiers: Modifiers,
    },
    /// A wheel scroll at a point; a positive `dy` scrolls toward the end.
    Scroll {
        at: (f32, f32),
        dy: f32,
    },
    Keys(Vec<KeyInput>),
    Wait(Duration),
    Settle,
    Snap(String),
    /// The frame on screen now, without settling, for a view that keeps
    /// moving.
    SnapNow(String),
    Bounds(String),
    Selectors(Option<String>),
}

/// A step and the line it came from, counted from one.
#[derive(Clone, Debug, PartialEq)]
pub struct ScriptLine {
    pub line: usize,
    pub step: Step,
}

/// A word on a line, and whether it was quoted.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Word {
    text: String,
    quoted: bool,
}

/// Reads a whole script, or says which line is wrong and why.
pub fn parse(source: &str) -> Result<Vec<ScriptLine>, String> {
    let mut lines = Vec::new();
    for (index, text) in source.lines().enumerate() {
        let line = index + 1;
        let words = split_words(text).map_err(|error| format!("line {line}: {error}"))?;
        let Some((name, arguments)) = words.split_first() else {
            continue;
        };
        let step =
            parse_step(&name.text, arguments).map_err(|error| format!("line {line}: {error}"))?;
        lines.push(ScriptLine { line, step });
    }
    Ok(lines)
}

fn split_words(text: &str) -> Result<Vec<Word>, String> {
    let mut words = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(&next) = chars.peek() {
        if next.is_whitespace() {
            chars.next();
        } else if next == '#' {
            break;
        } else if next == '"' {
            chars.next();
            words.push(quoted_word(&mut chars)?);
        } else {
            let mut word = String::new();
            while let Some(c) = chars.next_if(|c| !c.is_whitespace()) {
                word.push(c);
            }
            words.push(Word {
                text: word,
                quoted: false,
            });
        }
    }
    Ok(words)
}

fn quoted_word(chars: &mut impl Iterator<Item = char>) -> Result<Word, String> {
    let mut text = String::new();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Ok(Word { text, quoted: true }),
            '\\' => text.push(chars.next().ok_or("a line ends with \\")?),
            c => text.push(c),
        }
    }
    Err("a quote isn't closed".to_owned())
}

/// Reads a step from its name, its words, and its words with their
/// quoting.
type StepParser = fn(&str, &[&str], &[Word]) -> Result<Step, String>;

/// Every step but the pointer's, by name.
const STEP_PARSERS: [(&str, StepParser); 13] = [
    ("size", |name, texts, _| parse_size(one(name, texts)?)),
    ("theme", |name, texts, _| parse_theme(one(name, texts)?)),
    ("open", |name, texts, _| {
        Ok(Step::Open(one(name, texts)?.to_owned()))
    }),
    ("command", |name, texts, _| {
        Ok(Step::Command(one(name, texts)?.to_owned()))
    }),
    ("drag", |_, texts, _| parse_drag(texts)),
    ("scroll", |_, texts, _| parse_scroll(texts)),
    ("keys", |_, _, words| parse_keys(words)),
    ("wait", |name, texts, _| {
        Ok(Step::Wait(parse_duration(one(name, texts)?)?))
    }),
    ("settle", |name, texts, _| {
        none(name, texts).map(|()| Step::Settle)
    }),
    ("snap", |name, texts, _| {
        Ok(Step::Snap(snap_name(one(name, texts)?)?))
    }),
    ("snap-now", |name, texts, _| {
        Ok(Step::SnapNow(snap_name(one(name, texts)?)?))
    }),
    ("bounds", |name, texts, _| {
        Ok(Step::Bounds(one(name, texts)?.to_owned()))
    }),
    ("selectors", |_, texts, _| parse_selectors(texts)),
];

/// The pointer's steps: what each does, and whether it takes a selector
/// rather than a point.
const POINTER_STEPS: [(&str, PointerAction, bool); 8] = [
    ("move", PointerAction::Move, false),
    ("hover-element", PointerAction::Move, true),
    ("click", PointerAction::Click, false),
    ("click-element", PointerAction::Click, true),
    ("double-click", PointerAction::DoubleClick, false),
    ("double-click-element", PointerAction::DoubleClick, true),
    ("right-click", PointerAction::RightClick, false),
    ("right-click-element", PointerAction::RightClick, true),
];

fn parse_step(name: &str, arguments: &[Word]) -> Result<Step, String> {
    let texts: Vec<&str> = arguments.iter().map(|word| word.text.as_str()).collect();
    match STEP_PARSERS.iter().find(|(step, _)| *step == name) {
        Some((_, parse)) => parse(name, &texts, arguments),
        None => parse_pointer(name, &texts),
    }
}

/// `move`, the clicks, and their `-element` forms.
fn parse_pointer(name: &str, texts: &[&str]) -> Result<Step, String> {
    let &(_, action, on_element) = POINTER_STEPS
        .iter()
        .find(|(step, _, _)| *step == name)
        .ok_or_else(|| format!("there's no step {name}; the steps are:\n{STEPS}"))?;
    let (target, rest) = match on_element {
        true => element_target(name, texts)?,
        false => point_target(name, texts)?,
    };
    let modifiers = optional_modifiers(name, rest)?;
    if action == PointerAction::Move && modifiers != Modifiers::none() {
        return Err(format!("{name} takes no modifiers"));
    }
    Ok(Step::Pointer {
        action,
        target,
        modifiers,
    })
}

fn element_target<'a>(name: &str, texts: &'a [&'a str]) -> Result<(Target, &'a [&'a str]), String> {
    let (selector, rest) = texts
        .split_first()
        .ok_or_else(|| format!("{name} needs a selector"))?;
    Ok((Target::Element((*selector).to_owned()), rest))
}

fn point_target<'a>(name: &str, texts: &'a [&'a str]) -> Result<(Target, &'a [&'a str]), String> {
    let [x, y, rest @ ..] = texts else {
        return Err(format!("{name} needs X and Y"));
    };
    Ok((Target::At(number(x)?, number(y)?), rest))
}

fn optional_modifiers(name: &str, rest: &[&str]) -> Result<Modifiers, String> {
    match rest {
        [] => Ok(Modifiers::none()),
        [modifiers] => parse_modifiers(modifiers),
        _ => Err(format!("{name} has too many words")),
    }
}

fn parse_drag(texts: &[&str]) -> Result<Step, String> {
    let [x1, y1, x2, y2, rest @ ..] = texts else {
        return Err("drag needs X1 Y1 X2 Y2".to_owned());
    };
    Ok(Step::Drag {
        from: (number(x1)?, number(y1)?),
        to: (number(x2)?, number(y2)?),
        modifiers: optional_modifiers("drag", rest)?,
    })
}

fn parse_scroll(texts: &[&str]) -> Result<Step, String> {
    let [x, y, dy] = texts else {
        return Err("scroll needs X Y DY".to_owned());
    };
    Ok(Step::Scroll {
        at: (number(x)?, number(y)?),
        dy: number(dy)?,
    })
}

fn parse_keys(arguments: &[Word]) -> Result<Step, String> {
    if arguments.is_empty() {
        return Err("keys needs a key or some \"text\"".to_owned());
    }
    let keys = arguments
        .iter()
        .map(|word| match word.quoted {
            true => KeyInput::Text(word.text.clone()),
            false => KeyInput::Stroke(word.text.clone()),
        })
        .collect();
    Ok(Step::Keys(keys))
}

fn parse_selectors(texts: &[&str]) -> Result<Step, String> {
    match texts {
        [] => Ok(Step::Selectors(None)),
        [prefix] => Ok(Step::Selectors(Some((*prefix).to_owned()))),
        _ => Err("selectors takes at most a prefix".to_owned()),
    }
}

fn one<'a>(name: &str, texts: &[&'a str]) -> Result<&'a str, String> {
    match texts {
        [only] => Ok(only),
        _ => Err(format!("{name} takes one word")),
    }
}

fn none(name: &str, texts: &[&str]) -> Result<(), String> {
    match texts {
        [] => Ok(()),
        _ => Err(format!("{name} takes nothing")),
    }
}

/// `WxH`, such as `1200x800`.
pub fn parse_window_size(text: &str) -> Result<(u32, u32), String> {
    let wrong = || format!("a size is WxH, such as 1200x800, not {text}");
    let (width, height) = text.split_once('x').ok_or_else(wrong)?;
    let pixels = |part: &str| {
        part.parse::<u32>()
            .ok()
            .filter(|&n| n >= 1)
            .ok_or_else(wrong)
    };
    Ok((pixels(width)?, pixels(height)?))
}

fn parse_size(text: &str) -> Result<Step, String> {
    let (width, height) = parse_window_size(text)?;
    Ok(Step::Size { width, height })
}

fn parse_theme(text: &str) -> Result<Step, String> {
    match text {
        "light" => Ok(Step::Theme { dark: false }),
        "dark" => Ok(Step::Theme { dark: true }),
        _ => Err(format!("theme is light or dark, not {text}")),
    }
}

/// `300ms`, `2s` or `1.5s`; a bare number is milliseconds.
fn parse_duration(text: &str) -> Result<Duration, String> {
    let wrong = || format!("a duration is like 300ms or 2s, not {text}");
    let (amount, scale) = if let Some(ms) = text.strip_suffix("ms") {
        (ms, 0.001)
    } else if let Some(seconds) = text.strip_suffix('s') {
        (seconds, 1.)
    } else {
        (text, 0.001)
    };
    let amount: f64 = amount.parse().map_err(|_| wrong())?;
    Duration::try_from_secs_f64(amount * scale).map_err(|_| wrong())
}

/// A file name for `snap`: letters, digits, `-`, `_` and `.`, with `.png`
/// added when it's missing.
fn snap_name(text: &str) -> Result<String, String> {
    let allowed = |c: char| c.is_alphanumeric() || matches!(c, '-' | '_' | '.');
    if text.is_empty() || text.starts_with('.') || !text.chars().all(allowed) {
        return Err(format!(
            "a snap name is letters, digits, -, _ and ., not {text}"
        ));
    }
    Ok(match text.ends_with(".png") {
        true => text.to_owned(),
        false => format!("{text}.png"),
    })
}

fn number(text: &str) -> Result<f32, String> {
    text.parse::<f32>()
        .ok()
        .filter(|value| value.is_finite())
        .ok_or_else(|| format!("expected a number, not {text}"))
}

/// Modifiers as the keymap writes them: `cmd`, `cmd-shift`, `ctrl-alt`.
pub fn parse_modifiers(text: &str) -> Result<Modifiers, String> {
    let mut modifiers = Modifiers::none();
    for part in text.split('-') {
        let flag = match part {
            "cmd" | "super" | "win" => &mut modifiers.platform,
            "ctrl" => &mut modifiers.control,
            "alt" | "option" => &mut modifiers.alt,
            "shift" => &mut modifiers.shift,
            "fn" => &mut modifiers.function,
            _ => {
                return Err(format!(
                    "{part} isn't a modifier (cmd, ctrl, alt, shift, fn)"
                ));
            }
        };
        *flag = true;
    }
    Ok(modifiers)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(text: &str) -> Step {
        parse(text).unwrap().remove(0).step
    }

    #[test]
    fn reads_the_steps() {
        let script = parse(
            "size 1200x800\n\n# a comment\nopen \"Notes/Lemma note.md\"\n\
             command settings.open  # trailing comment\nwait 300ms\nsnap before",
        )
        .unwrap();
        let steps: Vec<_> = script.iter().map(|line| &line.step).collect();
        assert_eq!(
            steps,
            [
                &Step::Size {
                    width: 1200,
                    height: 800
                },
                &Step::Open("Notes/Lemma note.md".into()),
                &Step::Command("settings.open".into()),
                &Step::Wait(Duration::from_millis(300)),
                &Step::Snap("before.png".into()),
            ]
        );
        assert_eq!(script[1].line, 4);
    }

    #[test]
    fn pointer_steps_take_points_or_elements() {
        assert_eq!(
            step("click 320 540.5 cmd-shift"),
            Step::Pointer {
                action: PointerAction::Click,
                target: Target::At(320., 540.5),
                modifiers: Modifiers {
                    platform: true,
                    shift: true,
                    ..Modifiers::none()
                },
            }
        );
        assert_eq!(
            step("hover-element \"settings-control-accent\""),
            Step::Pointer {
                action: PointerAction::Move,
                target: Target::Element("settings-control-accent".into()),
                modifiers: Modifiers::none(),
            }
        );
        assert_eq!(
            step("drag 1 2 3 4"),
            Step::Drag {
                from: (1., 2.),
                to: (3., 4.),
                modifiers: Modifiers::none(),
            }
        );
        assert_eq!(
            step("scroll 5 6 -120"),
            Step::Scroll {
                at: (5., 6.),
                dy: -120.
            }
        );
    }

    #[test]
    fn keys_keep_quoted_text_apart_from_keys() {
        assert_eq!(
            step("keys cmd-shift-p \"a \\\"b\\\"\" enter"),
            Step::Keys(vec![
                KeyInput::Stroke("cmd-shift-p".into()),
                KeyInput::Text("a \"b\"".into()),
                KeyInput::Stroke("enter".into()),
            ])
        );
    }

    #[test]
    fn durations_and_names() {
        assert_eq!(parse_duration("2s"), Ok(Duration::from_secs(2)));
        assert_eq!(parse_duration("1.5s"), Ok(Duration::from_millis(1500)));
        assert_eq!(parse_duration("40"), Ok(Duration::from_millis(40)));
        assert!(parse_duration("soon").is_err());
        assert_eq!(snap_name("a.png"), Ok("a.png".into()));
        assert!(snap_name("../a").is_err());
        assert!(snap_name(".hidden").is_err());
    }

    #[test]
    fn errors_name_the_line() {
        let error = parse("size 10x10\nclick 1").unwrap_err();
        assert!(error.starts_with("line 2: "), "{error}");
        assert!(parse("jump 1 2").unwrap_err().contains("hover-element"));
        assert!(parse("open \"unclosed").is_err());
        assert!(parse("move 1 2 cmd").is_err());
        assert!(parse("settle now").is_err());
        assert!(parse("click 1 2 hyper").is_err());
    }
}
