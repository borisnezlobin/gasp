//! LaTeX to Typst conversion and math layout for the editor.
//!
//! LaTeX is converted with mitex, evaluated against the vendored mitex Typst
//! scope and laid out by Typst in an in-memory world with embedded fonts.

mod bars;
mod cache;
mod convert;
mod error;
mod placeholders;
mod raster;
mod render;
mod scan;
mod scope;
mod world;

pub use cache::MathCache;
pub use convert::latex_to_typst;
pub use error::MathError;
pub use placeholders::fill_empty_arguments;
pub use raster::{MathCoverage, rasterize_latex};
pub use render::{RenderedMath, evict_layout_memory, render_latex, render_typst, warm_up};
pub use scan::{MathSnippet, find_math};
pub use scope::mitex_scope_for;
pub use world::{MITEX_SOURCES, SCOPE_MODULE_PATH, embedded_fonts};
