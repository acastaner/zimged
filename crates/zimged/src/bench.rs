//! Timing marks for `tools/bench` (§1.2).
//!
//! Off unless `ZIMGED_BENCH_T0_NS` is set (the bench sets it to the wall-clock time just before it
//! `exec`s zimged). Then each [`mark`] prints `<name> <ms since t0>` on stdout, and with
//! `ZIMGED_BENCH_EXIT=1` the app closes as soon as the first image is painted.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

fn t0_ns() -> Option<u128> {
    static T0: OnceLock<Option<u128>> = OnceLock::new();
    *T0.get_or_init(|| std::env::var("ZIMGED_BENCH_T0_NS").ok()?.parse().ok())
}

fn now_ns() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos())
}

/// Print a timing mark, when benchmarking.
pub fn mark(name: &str) {
    if let Some(t0) = t0_ns() {
        let ms = now_ns().saturating_sub(t0) as f64 / 1e6;
        println!("{name} {ms:.2}");
    }
}

/// Print a timing mark the first time only (e.g. `decoded`, which every later decode would repeat).
pub fn mark_once(name: &str, done: &AtomicBool) {
    if !done.swap(true, Ordering::Relaxed) {
        mark(name);
    }
}

/// Should the app close right after painting the first image?
pub fn exit_after_first_paint() -> bool {
    t0_ns().is_some() && std::env::var_os("ZIMGED_BENCH_EXIT").is_some()
}
