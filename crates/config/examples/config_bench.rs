//! Times loading a vault's `.gasp/` config, which the desktop and the
//! phone do as they open a vault and again whenever a config file
//! changes: the built-in defaults alone, and a vault whose config the
//! migrator wrote from `reference/obsidian` (212 Latex Suite snippets,
//! the owner's replacements, hotkeys and settings).
//!
//! `cargo run --release -p gasp-config --example config_bench`
//!
//! With `GASP_BENCH_ENFORCE=1`, a result over its budget fails the run.

use std::path::Path;
use std::time::Duration;

use gasp_bench::corpus::ScratchDir;
use gasp_bench::{CountingAllocator, Report, Samples};
use gasp_config::{Config, ConfigLoader};
use gasp_snippets::SnippetEngine;

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn main() {
    let mut report = Report::new("gasp-config, loading a vault's config");
    let defaults = Samples::collect(20, Config::defaults);
    report.time(
        "built-in defaults, median",
        defaults.median(),
        Duration::from_millis(15),
    );
    let vault = ScratchDir::new("config");
    write_migrated_config(vault.path());
    let load = || {
        let mut loader = ConfigLoader::for_vault(vault.path());
        loader.load_all();
        loader
    };
    let loads = Samples::collect(20, load);
    report.time(
        "the owner's migrated config, median",
        loads.median(),
        Duration::from_millis(30),
    );
    let (loader, stats) = CountingAllocator::measure(load);
    report.note_count("allocations in a load", stats.allocations as f64);
    report.note_bytes("config kept in memory", stats.retained_bytes as f64);
    let snippets = &loader.config().typing.snippets.file;
    report.note_count("snippets", snippets.snippets().count() as f64);
    let compile = Samples::collect(5, || {
        let engine = SnippetEngine::lazy(snippets.snippets().cloned().collect());
        engine.warm();
        engine
    });
    report.time(
        "compiling the snippets (in the background), median",
        compile.median(),
        Duration::from_millis(40),
    );
    report.finish();
}

/// The config files the migrator writes for the owner's Obsidian settings.
fn write_migrated_config(vault: &Path) {
    let obsidian = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reference/obsidian");
    let migration = gasp_migrate::migrate_obsidian(&obsidian).expect("the settings migrate");
    migration
        .write_to(&vault.join(".gasp"))
        .expect("the config files write");
}
