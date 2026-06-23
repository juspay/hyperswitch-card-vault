//! Lightweight counter metrics for KV operations.
//!
//! These are simple `AtomicU64` counters during the scaffolding phase.
//! They can be upgraded to OpenTelemetry-style metrics (matching the
//! `redis_interface` crate's approach) when the metrics pipeline is wired.

use std::sync::atomic::{AtomicU64, Ordering};

pub struct Counter(AtomicU64);

impl Default for Counter {
    fn default() -> Self {
        Self::new()
    }
}

impl Counter {
    pub const fn new() -> Self {
        Self(AtomicU64::new(0))
    }

    pub fn add(&self, value: u64, _attributes: &[(&str, &str)]) {
        self.0.fetch_add(value, Ordering::Relaxed);
    }

    pub fn get(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

pub static KV_OPERATION_SUCCESSFUL: Counter = Counter::new();
pub static KV_OPERATION_FAILED: Counter = Counter::new();
pub static KV_PUSHED_TO_DRAINER: Counter = Counter::new();
pub static KV_FAILED_TO_PUSH_TO_DRAINER: Counter = Counter::new();
pub static KV_MISS: Counter = Counter::new();
pub static KV_SOFT_KILL_ACTIVE_UPDATE: Counter = Counter::new();
