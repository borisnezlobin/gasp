//! Errors returned by conversion and rendering.

use std::fmt;

/// Why an equation could not be converted or rendered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MathError {
    /// mitex could not convert the LaTeX source to Typst.
    Convert(String),
    /// Typst rejected the converted source or failed to lay it out.
    Render(String),
}

impl MathError {
    /// The error message without its category.
    pub fn message(&self) -> &str {
        match self {
            Self::Convert(message) | Self::Render(message) => message,
        }
    }

    /// A short category name, useful for grouping failures.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Convert(_) => "convert",
            Self::Render(_) => "render",
        }
    }
}

impl fmt::Display for MathError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} error: {}", self.kind(), self.message())
    }
}

impl std::error::Error for MathError {}
