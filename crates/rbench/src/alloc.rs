//! Opt-in whole-process Rust allocation accounting. Does not count driver/ObjC,
//! mmap or allocator overhead. Snapshots of multiple atomics are observational.
use std::alloc::{GlobalAlloc, Layout};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub allocations: u64,
    pub reallocations: u64,
    pub deallocations: u64,
    pub requested_bytes: u64,
    pub live_bytes: u64,
    pub lifetime_peak_bytes: u64,
}
pub struct TrackingAllocator<A> {
    inner: A,
    allocations: AtomicU64,
    reallocations: AtomicU64,
    deallocations: AtomicU64,
    requested: AtomicU64,
    live: AtomicU64,
    peak: AtomicU64,
}
impl<A> TrackingAllocator<A> {
    pub const fn new(inner: A) -> Self {
        Self {
            inner,
            allocations: AtomicU64::new(0),
            reallocations: AtomicU64::new(0),
            deallocations: AtomicU64::new(0),
            requested: AtomicU64::new(0),
            live: AtomicU64::new(0),
            peak: AtomicU64::new(0),
        }
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            allocations: self.allocations.load(Relaxed),
            reallocations: self.reallocations.load(Relaxed),
            deallocations: self.deallocations.load(Relaxed),
            requested_bytes: self.requested.load(Relaxed),
            live_bytes: self.live.load(Relaxed),
            lifetime_peak_bytes: self.peak.load(Relaxed),
        }
    }
    fn grow(&self, n: u64) {
        let live = self.live.fetch_add(n, Relaxed) + n;
        self.peak.fetch_max(live, Relaxed);
    }
    fn allocated(&self, n: usize) {
        self.allocations.fetch_add(1, Relaxed);
        self.requested.fetch_add(n as u64, Relaxed);
        self.grow(n as u64);
    }
}
// SAFETY: all calls forward original pointers and layouts to the wrapped
// allocator. Accounting performs no allocation and does not dereference memory.
unsafe impl<A: GlobalAlloc> GlobalAlloc for TrackingAllocator<A> {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = unsafe { self.inner.alloc(l) };
        if !p.is_null() {
            self.allocated(l.size());
        }
        p
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        let p = unsafe { self.inner.alloc_zeroed(l) };
        if !p.is_null() {
            self.allocated(l.size());
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { self.inner.dealloc(p, l) };
        self.deallocations.fetch_add(1, Relaxed);
        self.live.fetch_sub(l.size() as u64, Relaxed);
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        let p = unsafe { self.inner.realloc(p, l, n) };
        if !p.is_null() {
            self.reallocations.fetch_add(1, Relaxed);
            self.requested.fetch_add(n as u64, Relaxed);
            if n >= l.size() {
                self.grow((n - l.size()) as u64);
            } else {
                self.live.fetch_sub((l.size() - n) as u64, Relaxed);
            }
        }
        p
    }
}
