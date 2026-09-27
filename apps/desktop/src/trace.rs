//! Startup tracing. With `EDITOR_TRACE_STARTUP=1` the app logs each phase
//! of launch to stderr — when it started, how long it took and how much
//! CPU its thread used — up to the first frame on screen. With
//! `EDITOR_TRACE_STARTUP=quit` it also quits after that frame, for timing
//! launches in a loop.
//!
//! Lines look like `startup  config.load  at 12.30  took 4.51  cpu 4.40`
//! (milliseconds since `main`). The last one, `first-frame`, also gives
//! the wall clock in Unix milliseconds so a script can add the time before
//! `main` (loading the binary) to the total.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use gpui::Window;

const ENV_VAR: &str = "EDITOR_TRACE_STARTUP";

static START: OnceLock<Instant> = OnceLock::new();
static MODE: OnceLock<Mode> = OnceLock::new();
/// Set once the first frame is on screen; tracing stops there.
static DONE: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Off,
    Log,
    Quit,
}

fn mode() -> Mode {
    *MODE.get_or_init(|| match std::env::var(ENV_VAR).as_deref() {
        Ok("quit") => Mode::Quit,
        Ok(value) if !value.is_empty() && value != "0" => Mode::Log,
        _ => Mode::Off,
    })
}

/// Whether startup tracing is on, and startup isn't over yet.
pub fn enabled() -> bool {
    mode() != Mode::Off && !DONE.load(Ordering::Relaxed)
}

/// Starts the clock. Call first thing in `main`.
pub fn init() {
    START.get_or_init(Instant::now);
    if enabled() {
        eprintln!("startup  main  at 0.00  unix {}", unix_millis());
    }
}

fn since_start() -> f64 {
    millis(START.get_or_init(Instant::now).elapsed().as_secs_f64())
}

fn millis(seconds: f64) -> f64 {
    seconds * 1000.
}

fn unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis())
}

/// Logs that `name` happened now.
pub fn mark(name: &str) {
    if enabled() {
        eprintln!("startup  {name}  at {:.2}", since_start());
    }
}

/// Times a phase until it's dropped: `let _span = trace::span("config");`.
#[must_use = "a span measures until it is dropped"]
pub struct Span {
    name: &'static str,
    started: Instant,
    at: f64,
    cpu: Option<u64>,
}

/// Starts timing the phase `name`, or does nothing when tracing is off.
pub fn span(name: &'static str) -> Option<Span> {
    enabled().then(|| Span {
        name,
        started: Instant::now(),
        at: since_start(),
        cpu: thread_cpu_nanos(),
    })
}

impl Drop for Span {
    fn drop(&mut self) {
        let took = millis(self.started.elapsed().as_secs_f64());
        let cpu = self
            .cpu
            .zip(thread_cpu_nanos())
            .map_or(f64::NAN, |(before, after)| {
                after.saturating_sub(before) as f64 / 1e6
            });
        eprintln!(
            "startup  {}  at {:.2}  took {took:.2}  cpu {cpu:.2}",
            self.name, self.at
        );
    }
}

/// Logs the first frame of `window` once it's on screen, with the CPU the
/// whole process has used, and quits in `quit` mode.
pub fn on_first_frame(window: &Window) {
    if !enabled() {
        return;
    }
    window.on_next_frame(|_, cx| {
        mark("frame-start");
        // Tasks run once the frame callback returns, after the frame is
        // drawn and presented.
        cx.spawn(async move |cx| {
            let cpu = process_cpu_nanos().map_or(f64::NAN, |nanos| nanos as f64 / 1e6);
            eprintln!(
                "startup  first-frame  at {:.2}  process-cpu {cpu:.2}  unix {}",
                since_start(),
                unix_millis()
            );
            DONE.store(true, Ordering::Relaxed);
            if mode() == Mode::Quit {
                cx.update(|cx| cx.quit()).ok();
            }
        })
        .detach();
    });
}

/// CPU time the calling thread has used, where the platform tells.
#[cfg(unix)]
fn thread_cpu_nanos() -> Option<u64> {
    let time = rustix::time::clock_gettime(rustix::time::ClockId::ThreadCPUTime);
    Some(time.tv_sec as u64 * 1_000_000_000 + time.tv_nsec as u64)
}

#[cfg(not(unix))]
fn thread_cpu_nanos() -> Option<u64> {
    None
}

/// CPU time every thread of the process has used, where the platform tells.
#[cfg(unix)]
fn process_cpu_nanos() -> Option<u64> {
    let time = rustix::time::clock_gettime(rustix::time::ClockId::ProcessCPUTime);
    Some(time.tv_sec as u64 * 1_000_000_000 + time.tv_nsec as u64)
}

#[cfg(not(unix))]
fn process_cpu_nanos() -> Option<u64> {
    None
}
