//! Dates in the Moment.js formats Obsidian's daily notes and templates
//! use, such as `YYYY-MM-DD` or `dddd, D MMMM YYYY`.

use jiff::civil::DateTime;

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

const WEEKDAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

type Render = fn(DateTime) -> String;

/// Each token and how it writes a time, longest first so `MMMM` wins
/// over `MM`.
const TOKENS: [(&str, Render); 20] = [
    ("YYYY", |t| format!("{:04}", t.year())),
    ("MMMM", |t| month(t).to_string()),
    ("dddd", |t| weekday(t).to_string()),
    ("MMM", |t| month(t)[..3].to_string()),
    ("ddd", |t| weekday(t)[..3].to_string()),
    ("YY", |t| format!("{:02}", t.year().rem_euclid(100))),
    ("MM", |t| format!("{:02}", t.month())),
    ("DD", |t| format!("{:02}", t.day())),
    ("Do", |t| ordinal(t.day())),
    ("HH", |t| format!("{:02}", t.hour())),
    ("hh", |t| format!("{:02}", hour12(t))),
    ("mm", |t| format!("{:02}", t.minute())),
    ("ss", |t| format!("{:02}", t.second())),
    ("M", |t| t.month().to_string()),
    ("D", |t| t.day().to_string()),
    ("H", |t| t.hour().to_string()),
    ("h", |t| hour12(t).to_string()),
    ("m", |t| t.minute().to_string()),
    ("s", |t| t.second().to_string()),
    ("A", |t| if t.hour() < 12 { "AM" } else { "PM" }.to_string()),
];

/// The local date and time now.
pub fn now() -> DateTime {
    jiff::Zoned::now().datetime()
}

/// `time` written in the Moment.js `format`. Text in `[brackets]` is
/// kept as it is, as are characters that aren't tokens.
pub fn format(time: DateTime, format: &str) -> String {
    let mut out = String::new();
    let mut rest = format;
    while let Some(ch) = rest.chars().next() {
        if ch == '[' {
            let end = rest.find(']').unwrap_or(rest.len());
            out.push_str(&rest[1..end]);
            rest = rest.get(end + 1..).unwrap_or("");
            continue;
        }
        match TOKENS.iter().find(|(token, _)| rest.starts_with(token)) {
            Some((token, render)) => {
                out.push_str(&render(time));
                rest = &rest[token.len()..];
            }
            None => {
                out.push(ch);
                rest = &rest[ch.len_utf8()..];
            }
        }
    }
    out
}

fn month(time: DateTime) -> &'static str {
    MONTHS[time.month() as usize - 1]
}

fn weekday(time: DateTime) -> &'static str {
    WEEKDAYS[time.weekday().to_monday_zero_offset() as usize]
}

fn hour12(time: DateTime) -> i8 {
    match time.hour() % 12 {
        0 => 12,
        hour => hour,
    }
}

fn ordinal(day: i8) -> String {
    let suffix = match (day % 10, day % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{day}{suffix}")
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::*;

    #[test]
    fn formats_moment_tokens() {
        let time = date(2026, 9, 7).at(14, 5, 9, 0);
        assert_eq!(format(time, "YYYY-MM-DD"), "2026-09-07");
        assert_eq!(format(time, "dddd, Do MMMM YY"), "Monday, 7th September 26");
        assert_eq!(format(time, "ddd D MMM, h:mm A"), "Mon 7 Sep, 2:05 PM");
        assert_eq!(format(time, "HH:mm:ss"), "14:05:09");
        assert_eq!(format(time, "YYYY/MM/[Week] M"), "2026/09/Week 9");
    }

    #[test]
    fn ordinals() {
        let days: Vec<String> = [1, 2, 3, 4, 11, 12, 13, 21, 22, 31]
            .into_iter()
            .map(ordinal)
            .collect();
        assert_eq!(
            days,
            [
                "1st", "2nd", "3rd", "4th", "11th", "12th", "13th", "21st", "22nd", "31st"
            ]
        );
    }
}
