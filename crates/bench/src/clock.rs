//! What a bench's durations measure. Wall-clock time by default; with
//! `GASP_BENCH_CLOCK=cpu`, the CPU time of the measuring thread, which
//! other processes competing for the cores barely move, for comparing
//! before and after on a busy machine.
#![allow(unsafe_code)]

use std::sync::OnceLock;
use std::time::{Duration, Instant};

pub const CLOCK_VARIABLE: &str = "GASP_BENCH_CLOCK";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clock {
    Wall,
    ThreadCpu,
}

impl Clock {
    /// The clock the environment asks for.
    pub fn chosen() -> Clock {
        static CHOSEN: OnceLock<Clock> = OnceLock::new();
        *CHOSEN.get_or_init(|| match std::env::var(CLOCK_VARIABLE).as_deref() {
            Ok("cpu") => Clock::ThreadCpu,
            _ => Clock::Wall,
        })
    }
}

/// A started measurement on the chosen clock.
pub enum Stopwatch {
    Wall(Instant),
    ThreadCpu(Duration),
}

impl Stopwatch {
    pub fn start() -> Self {
        match Clock::chosen() {
            Clock::Wall => Stopwatch::Wall(Instant::now()),
            Clock::ThreadCpu => Stopwatch::ThreadCpu(thread_cpu_time()),
        }
    }

    pub fn elapsed(&self) -> Duration {
        match self {
            Stopwatch::Wall(started) => started.elapsed(),
            Stopwatch::ThreadCpu(started) => thread_cpu_time().saturating_sub(*started),
        }
    }
}

/// Asks macOS to run this thread on the performance cores, as it does for
/// the thread handling a keystroke, so a bench isn't timed on an
/// efficiency core while builds fill the machine. Elsewhere it does nothing.
pub fn prefer_fast_cores() {
    #[cfg(target_os = "macos")]
    // SAFETY: sets this thread's own QoS class; no pointers are involved.
    unsafe {
        libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE, 0);
    }
}

fn thread_cpu_time() -> Duration {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `time` is a valid, writable timespec for the call.
    let status = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut time) };
    if status != 0 {
        return Duration::ZERO;
    }
    Duration::new(time.tv_sec as u64, time.tv_nsec as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thread_cpu_time_moves_with_work() {
        let started = thread_cpu_time();
        let mut sum = 0u64;
        for value in 0..5_000_000u64 {
            sum = std::hint::black_box(sum.wrapping_add(value * value));
        }
        assert!(thread_cpu_time() > started, "{sum}");
    }
}
