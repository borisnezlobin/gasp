//! How much memory the app holds, for the benchmarks to report: the
//! physical footprint on macOS (what Activity Monitor shows as Memory),
//! the resident size elsewhere.

/// The process's memory in bytes, where the platform tells.
pub fn footprint_bytes() -> Option<u64> {
    platform::footprint_bytes()
}

/// The footprint as a line for a report, such as `memory: 64.2 MB`.
pub fn report() -> Option<String> {
    footprint_bytes().map(|bytes| format!("memory: {:.1} MB", bytes as f64 / 1e6))
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod platform {
    /// `RUSAGE_INFO_V0` from `<sys/resource.h>`.
    const RUSAGE_INFO_V0: i32 = 0;

    /// `struct rusage_info_v0`.
    #[repr(C)]
    #[derive(Default)]
    struct RusageInfoV0 {
        uuid: [u8; 16],
        user_time: u64,
        system_time: u64,
        package_idle_wakeups: u64,
        interrupt_wakeups: u64,
        pageins: u64,
        wired_size: u64,
        resident_size: u64,
        phys_footprint: u64,
        process_start: u64,
        process_exit: u64,
    }

    unsafe extern "C" {
        fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut RusageInfoV0) -> i32;
    }

    pub fn footprint_bytes() -> Option<u64> {
        let mut info = RusageInfoV0::default();
        let pid = std::process::id() as i32;
        // SAFETY: `info` is a `rusage_info_v0`, which is what the V0
        // flavour writes, and it lives for the call.
        let status = unsafe { proc_pid_rusage(pid, RUSAGE_INFO_V0, &mut info) };
        (status == 0).then_some(info.phys_footprint)
    }
}

#[cfg(target_os = "linux")]
mod platform {
    pub fn footprint_bytes() -> Option<u64> {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        let line = status.lines().find(|line| line.starts_with("VmRSS:"))?;
        let kilobytes: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
        Some(kilobytes * 1024)
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod platform {
    pub fn footprint_bytes() -> Option<u64> {
        None
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_running_process_has_a_footprint() {
        if cfg!(any(target_os = "macos", target_os = "linux")) {
            let bytes = super::footprint_bytes().unwrap();
            assert!(bytes > 1_000_000, "{bytes}");
        }
    }
}
