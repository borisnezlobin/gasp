//! A global allocator that counts allocations and live bytes, so a bench can
//! report allocation churn per operation and the memory a structure keeps.
//!
//! A bench installs it with
//! `#[global_allocator] static ALLOCATOR: CountingAllocator = CountingAllocator;`
//! and reads it through [`CountingAllocator::measure`].
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

pub struct CountingAllocator;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static ALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);
static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
static PEAK_LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);

fn record_allocation(size: usize) {
    ALLOCATIONS.fetch_add(1, Relaxed);
    ALLOCATED_BYTES.fetch_add(size, Relaxed);
    let live = LIVE_BYTES.fetch_add(size, Relaxed) + size;
    PEAK_LIVE_BYTES.fetch_max(live, Relaxed);
}

fn record_free(size: usize) {
    LIVE_BYTES.fetch_sub(size, Relaxed);
}

// SAFETY: every call forwards to the system allocator with the caller's
// arguments unchanged; the counters are only bookkeeping.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_allocation(layout.size());
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_allocation(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        record_free(layout.size());
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record_free(layout.size());
        record_allocation(new_size);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

/// What the allocator saw while an operation ran. Counts are only meaningful
/// when [`CountingAllocator`] is the global allocator.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AllocStats {
    /// Calls to allocate or reallocate.
    pub allocations: usize,
    /// Bytes requested by those calls.
    pub allocated_bytes: usize,
    /// Growth in live bytes from start to end: what the operation kept.
    pub retained_bytes: isize,
    /// The most live bytes above the starting point at any moment.
    pub peak_extra_bytes: usize,
}

impl CountingAllocator {
    /// Runs `operation` and returns its result with what it allocated.
    pub fn measure<T>(operation: impl FnOnce() -> T) -> (T, AllocStats) {
        let allocations = ALLOCATIONS.load(Relaxed);
        let allocated_bytes = ALLOCATED_BYTES.load(Relaxed);
        let live = LIVE_BYTES.load(Relaxed);
        PEAK_LIVE_BYTES.store(live, Relaxed);
        let result = std::hint::black_box(operation());
        let stats = AllocStats {
            allocations: ALLOCATIONS.load(Relaxed) - allocations,
            allocated_bytes: ALLOCATED_BYTES.load(Relaxed) - allocated_bytes,
            retained_bytes: LIVE_BYTES.load(Relaxed) as isize - live as isize,
            peak_extra_bytes: PEAK_LIVE_BYTES.load(Relaxed).saturating_sub(live),
        };
        (result, stats)
    }

    /// Bytes currently allocated through this allocator.
    pub fn live_bytes() -> usize {
        LIVE_BYTES.load(Relaxed)
    }
}

impl AllocStats {
    /// Averages over `operations` runs measured together.
    pub fn per_operation(&self, operations: usize) -> (f64, f64) {
        let operations = operations.max(1) as f64;
        (
            self.allocations as f64 / operations,
            self.allocated_bytes as f64 / operations,
        )
    }
}

/// A byte count in B, KB or MB, whichever reads better.
pub fn format_bytes(bytes: f64) -> String {
    if bytes.abs() >= 1024. * 1024. {
        format!("{:.2} MB", bytes / (1024. * 1024.))
    } else if bytes.abs() >= 1024. {
        format!("{:.1} KB", bytes / 1024.)
    } else {
        format!("{bytes:.0} B")
    }
}
