//! Shared pieces for the crates' benches (`cargo run --release -p <crate>
//! --example <name>_bench`): timing samples, a counting allocator, budgets
//! that fail the run in CI, and fixtures built from the synthetic corpus.
//!
//! A bench records what it measured in a [`Report`] with a budget for each
//! line. With `GASP_BENCH_ENFORCE=1` set (as CI's bench job does), a line
//! over its budget makes the run exit with an error.

pub mod alloc;
pub mod clock;
pub mod corpus;
pub mod report;
pub mod samples;

pub use alloc::{AllocStats, CountingAllocator};
pub use clock::Stopwatch;
pub use report::Report;
pub use samples::Samples;
