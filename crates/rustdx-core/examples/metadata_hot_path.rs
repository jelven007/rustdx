//! Offline allocation/timing observation. No network or market qualification.
use rustdx_complete::tcp::stock::PrecisionSnapshot;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::Instant,
};

struct CountingAllocator;
static ENABLED: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static ALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);

// SAFETY: Every operation delegates the unchanged pointer/layout to System.
// Counters have no allocation, ownership or synchronization requirements.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ENABLED.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            ALLOCATED_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        }
        // SAFETY: The caller provides a valid allocation layout.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: The allocator contract supplies the original pointer/layout.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if ENABLED.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            ALLOCATED_BYTES.fetch_add(size, Ordering::Relaxed);
        }
        // SAFETY: The allocator contract supplies a valid block and new size.
        unsafe { System.realloc(ptr, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn main() {
    let codes: Vec<_> = (0..5000).map(|i| format!("{i:06}")).collect();
    let entries: Vec<_> = [0, 1]
        .into_iter()
        .flat_map(|market| {
            codes
                .iter()
                .map(move |code| (market, code.as_str(), 2 + market as u8))
        })
        .collect();
    let snapshot = PrecisionSnapshot::from_decimals(&entries, chrono::Utc::now(), None).unwrap();
    let mut samples = Vec::new();
    for _ in 0..5 {
        ALLOCATIONS.store(0, Ordering::Relaxed);
        ALLOCATED_BYTES.store(0, Ordering::Relaxed);
        ENABLED.store(true, Ordering::Relaxed);
        let start = Instant::now();
        let mut checksum = 0u64;
        for i in 0..200_000 {
            let code = black_box(codes[i % codes.len()].as_str());
            let market = black_box((i % 2) as u16);
            checksum += u64::from(black_box(snapshot.scale(market, code).unwrap()).decimal());
        }
        let elapsed_ns = start.elapsed().as_nanos();
        ENABLED.store(false, Ordering::Relaxed);
        samples.push(serde_json::json!({
            "elapsed_ns": elapsed_ns, "allocations": ALLOCATIONS.load(Ordering::Relaxed),
            "allocated_bytes": ALLOCATED_BYTES.load(Ordering::Relaxed), "checksum": checksum,
        }));
    }
    println!(
        "{}",
        serde_json::json!({
            "schema_version": 1, "synthetic": true, "network": false,
            "dataset": "markets=0,1; codes=000000..004999; decimals=2,3",
            "lookups_per_sample": 200000, "entries": entries.len(), "samples": samples,
        })
    );
}
