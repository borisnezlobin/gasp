//! The app's memory allocator on macOS: the system's, except that blocks
//! of a mebibyte or more are mapped from the kernel directly and unmapped
//! when freed.
//!
//! The macOS allocator keeps large blocks it's given back, still counted
//! against the app, for reuse. Decoding a note's images passes tens of
//! megabytes through such blocks, so after opening image-heavy notes the
//! app kept hundreds of megabytes it no longer used (538 MB against 236 MB
//! with `MallocSpaceEfficient=1` in the open bench). Unmapped blocks go
//! back at once. A large block costs a system call either way.

#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::ffi::c_void;

use rustix::mm::{MapFlags, ProtFlags, mmap_anonymous, munmap};

/// Blocks at least this big are mapped from the kernel.
const MAPPED_FROM: usize = 1 << 20;
/// The alignment a mapping always has: the smallest page size.
const MAPPING_ALIGNMENT: usize = 4096;

/// The system allocator, with large blocks mapped directly.
pub struct ReturningAllocator;

fn is_mapped(size: usize, align: usize) -> bool {
    size >= MAPPED_FROM && align <= MAPPING_ALIGNMENT
}

unsafe fn map(size: usize) -> *mut u8 {
    // SAFETY: a new private anonymous mapping, placed by the kernel.
    let mapped = unsafe {
        mmap_anonymous(
            std::ptr::null_mut(),
            size,
            ProtFlags::READ | ProtFlags::WRITE,
            MapFlags::PRIVATE,
        )
    };
    mapped.map_or(std::ptr::null_mut(), |pointer| pointer.cast())
}

unsafe fn unmap(pointer: *mut u8, size: usize) {
    // SAFETY: the caller passes a mapping `map` made of `size` bytes.
    unsafe {
        munmap(pointer.cast::<c_void>(), size).ok();
    }
}

// SAFETY: every block comes from `System` or a fresh mapping, and goes
// back the same way: the layout's size and alignment, which a block keeps
// for its life, decide which.
unsafe impl GlobalAlloc for ReturningAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if is_mapped(layout.size(), layout.align()) {
            // SAFETY: see `map`.
            return unsafe { map(layout.size()) };
        }
        // SAFETY: forwarded as given.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if is_mapped(layout.size(), layout.align()) {
            // SAFETY: see `map`; fresh mappings are zeroed.
            return unsafe { map(layout.size()) };
        }
        // SAFETY: forwarded as given.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        if is_mapped(layout.size(), layout.align()) {
            // SAFETY: a block this size was mapped by `alloc`.
            return unsafe { unmap(pointer, layout.size()) };
        }
        // SAFETY: a block this size came from `System`.
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let was_mapped = is_mapped(layout.size(), layout.align());
        if !was_mapped && !is_mapped(new_size, layout.align()) {
            // SAFETY: both sizes stay with `System`.
            return unsafe { System.realloc(pointer, layout, new_size) };
        }
        // SAFETY: `new_size` is non-zero and, rounded to the alignment,
        // doesn't overflow, as `realloc`'s contract promises.
        let new_layout = unsafe { Layout::from_size_align_unchecked(new_size, layout.align()) };
        // SAFETY: a new block, then the old one's bytes that fit, then the
        // old block freed the way it was made.
        unsafe {
            let moved = self.alloc(new_layout);
            if !moved.is_null() {
                std::ptr::copy_nonoverlapping(pointer, moved, layout.size().min(new_size));
                self.dealloc(pointer, layout);
            }
            moved
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_move_between_the_system_and_mappings_intact() {
        let allocator = ReturningAllocator;
        let small = Layout::from_size_align(4096, 8).unwrap();
        // SAFETY: each block is used within its size and freed once with
        // the layout it has then.
        unsafe {
            let block = allocator.alloc(small);
            std::ptr::write_bytes(block, 7, small.size());
            let grown = allocator.realloc(block, small, MAPPED_FROM * 2);
            assert_eq!(*grown.add(small.size() - 1), 7);
            *grown.add(MAPPED_FROM * 2 - 1) = 9;
            let large = Layout::from_size_align(MAPPED_FROM * 2, 8).unwrap();
            let shrunk = allocator.realloc(grown, large, 100);
            assert_eq!(*shrunk.add(99), 7);
            allocator.dealloc(shrunk, Layout::from_size_align(100, 8).unwrap());
            let zeroed = allocator.alloc_zeroed(large);
            assert_eq!(*zeroed.add(MAPPED_FROM), 0);
            allocator.dealloc(zeroed, large);
        }
    }
}
