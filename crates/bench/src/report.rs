use std::time::Duration;

use crate::alloc::format_bytes;
use crate::samples::format_duration;

/// Set to `1` to make a bench fail when a measurement is over its budget.
pub const ENFORCE_VARIABLE: &str = "GASP_BENCH_ENFORCE";

#[derive(Clone, Copy)]
enum Amount {
    Time(Duration),
    Bytes(f64),
    Count(f64),
}

impl Amount {
    fn value(self) -> f64 {
        match self {
            Amount::Time(duration) => duration.as_secs_f64(),
            Amount::Bytes(bytes) => bytes,
            Amount::Count(count) => count,
        }
    }

    fn describe(self) -> String {
        match self {
            Amount::Time(duration) => format_duration(duration),
            Amount::Bytes(bytes) => format_bytes(bytes),
            Amount::Count(count) if count.fract() == 0. => format!("{count:.0}"),
            Amount::Count(count) => format!("{count:.1}"),
        }
    }
}

struct Line {
    label: String,
    measured: Amount,
    budget: Option<Amount>,
}

impl Line {
    fn over_budget(&self) -> bool {
        self.budget
            .is_some_and(|budget| self.measured.value() > budget.value())
    }
}

/// The measurements of one bench run, each with an optional budget, printed
/// as a table by [`Report::finish`].
pub struct Report {
    title: String,
    lines: Vec<Line>,
}

impl Report {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            lines: Vec::new(),
        }
    }

    pub fn time(&mut self, label: impl Into<String>, measured: Duration, budget: Duration) {
        self.push(label, Amount::Time(measured), Some(Amount::Time(budget)));
    }

    pub fn bytes(&mut self, label: impl Into<String>, measured: f64, budget: f64) {
        self.push(label, Amount::Bytes(measured), Some(Amount::Bytes(budget)));
    }

    pub fn count(&mut self, label: impl Into<String>, measured: f64, budget: f64) {
        self.push(label, Amount::Count(measured), Some(Amount::Count(budget)));
    }

    /// A measurement shown for information, with no budget.
    pub fn note_time(&mut self, label: impl Into<String>, measured: Duration) {
        self.push(label, Amount::Time(measured), None);
    }

    pub fn note_bytes(&mut self, label: impl Into<String>, measured: f64) {
        self.push(label, Amount::Bytes(measured), None);
    }

    pub fn note_count(&mut self, label: impl Into<String>, measured: f64) {
        self.push(label, Amount::Count(measured), None);
    }

    fn push(&mut self, label: impl Into<String>, measured: Amount, budget: Option<Amount>) {
        self.lines.push(Line {
            label: label.into(),
            measured,
            budget,
        });
    }

    /// Whether any measurement is over its budget.
    pub fn any_over_budget(&self) -> bool {
        self.lines.iter().any(Line::over_budget)
    }

    /// The report as a table, one measurement a line.
    pub fn render(&self) -> String {
        let label_width = self.lines.iter().map(|line| line.label.len()).max();
        let label_width = label_width.unwrap_or(0);
        let mut out = format!("{}\n", self.title);
        for line in &self.lines {
            let budget = match line.budget {
                Some(budget) => format!("budget {}", budget.describe()),
                None => String::new(),
            };
            let verdict = if line.over_budget() {
                "  OVER BUDGET"
            } else {
                ""
            };
            out.push_str(&format!(
                "  {:label_width$}  {:>12}  {budget}{verdict}\n",
                line.label,
                line.measured.describe(),
            ));
        }
        out
    }

    /// Prints the report, and exits with an error when a measurement is over
    /// its budget and budgets are enforced.
    pub fn finish(self) {
        print!("{}", self.render());
        if self.any_over_budget() && enforced() {
            eprintln!("{}: over budget", self.title);
            std::process::exit(1);
        }
    }
}

fn enforced() -> bool {
    std::env::var(ENFORCE_VARIABLE).is_ok_and(|value| value == "1")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_over_budget_is_flagged() {
        let mut report = Report::new("parse");
        report.time(
            "full parse",
            Duration::from_millis(3),
            Duration::from_millis(2),
        );
        report.bytes("index", 10., 20.);
        assert!(report.any_over_budget());
        assert!(report.render().contains("OVER BUDGET"));
    }

    #[test]
    fn notes_have_no_budget() {
        let mut report = Report::new("sizes");
        report.note_count("notes", 204.);
        assert!(!report.any_over_budget());
        assert!(report.render().contains("204"));
    }
}
