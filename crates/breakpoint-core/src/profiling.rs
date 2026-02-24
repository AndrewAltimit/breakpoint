//! Feature-gated profiling primitives for measuring per-frame/per-tick performance.
//!
//! Enable with the `profiling` feature flag. When disabled, the [`profile!`] macro
//! expands to nothing and all types are absent — zero overhead in production.

use std::cell::RefCell;

use serde::Serialize;

// ── Timing abstraction ──────────────────────────────────────────────

/// Returns the current time in microseconds.
#[cfg(not(target_arch = "wasm32"))]
fn now_us() -> f64 {
    use std::time::Instant;
    thread_local! {
        static EPOCH: Instant = Instant::now();
    }
    EPOCH.with(|epoch| epoch.elapsed().as_secs_f64() * 1_000_000.0)
}

#[cfg(target_arch = "wasm32")]
fn now_us() -> f64 {
    // web_sys::Performance::now() returns milliseconds with sub-ms precision.
    thread_local! {
        static PERF: Option<web_sys::Performance> = web_sys::window()
            .and_then(|w| w.performance());
    }
    PERF.with(|p| p.as_ref().map(|p| p.now() * 1000.0).unwrap_or(0.0))
}

// ── Per-frame scope collector (thread-local) ────────────────────────

thread_local! {
    static FRAME: RefCell<ProfileFrame> = RefCell::new(ProfileFrame::new());
}

/// A single named measurement within a frame.
#[derive(Debug, Clone, Serialize)]
pub struct ScopeTiming {
    pub name: &'static str,
    pub duration_us: f64,
}

/// Collects scope timings for a single frame/tick.
#[derive(Debug, Clone, Default)]
pub struct ProfileFrame {
    pub scopes: Vec<ScopeTiming>,
}

impl ProfileFrame {
    pub fn new() -> Self {
        Self {
            scopes: Vec::with_capacity(32),
        }
    }

    /// Clear all recorded scopes for a new frame.
    pub fn reset() {
        FRAME.with(|f| f.borrow_mut().scopes.clear());
    }

    /// Push a completed scope timing.
    pub fn push(name: &'static str, duration_us: f64) {
        FRAME.with(|f| {
            f.borrow_mut()
                .scopes
                .push(ScopeTiming { name, duration_us });
        });
    }

    /// Take a snapshot of the current frame's scopes.
    pub fn snapshot() -> Vec<ScopeTiming> {
        FRAME.with(|f| f.borrow().scopes.clone())
    }
}

// ── RAII scope guard ────────────────────────────────────────────────

/// Drop guard that measures elapsed time and records it to the thread-local
/// [`ProfileFrame`].
pub struct ProfileScope {
    name: &'static str,
    start_us: f64,
}

impl ProfileScope {
    #[inline]
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            start_us: now_us(),
        }
    }
}

impl Drop for ProfileScope {
    #[inline]
    fn drop(&mut self) {
        let elapsed = now_us() - self.start_us;
        ProfileFrame::push(self.name, elapsed);
    }
}

// ── Rolling statistics ──────────────────────────────────────────────

/// Per-scope rolling statistics over a sliding window.
#[derive(Debug, Clone, Serialize)]
pub struct ScopeStats {
    pub name: String,
    pub min_us: f64,
    pub max_us: f64,
    pub mean_us: f64,
    pub p95_us: f64,
    pub last_us: f64,
    pub sample_count: usize,
}

/// Maintains rolling stats across frames for all observed scopes.
pub struct ProfileStats {
    window_size: usize,
    /// scope_name → ring buffer of durations (μs)
    rings: std::collections::HashMap<&'static str, RingBuf>,
    frame_count: u64,
}

struct RingBuf {
    data: Vec<f64>,
    head: usize,
    len: usize,
    cap: usize,
}

impl RingBuf {
    fn new(cap: usize) -> Self {
        Self {
            data: vec![0.0; cap],
            head: 0,
            len: 0,
            cap,
        }
    }

    fn push(&mut self, val: f64) {
        self.data[self.head] = val;
        self.head = (self.head + 1) % self.cap;
        if self.len < self.cap {
            self.len += 1;
        }
    }

    fn samples(&self) -> &[f64] {
        &self.data[..self.len]
    }
}

impl ProfileStats {
    /// Create a new stats accumulator with the given window size (frames).
    pub fn new(window_size: usize) -> Self {
        Self {
            window_size,
            rings: std::collections::HashMap::new(),
            frame_count: 0,
        }
    }

    /// Record all scopes from a completed frame.
    pub fn record_frame(&mut self, scopes: &[ScopeTiming]) {
        self.frame_count += 1;
        for s in scopes {
            let ring = self
                .rings
                .entry(s.name)
                .or_insert_with(|| RingBuf::new(self.window_size));
            ring.push(s.duration_us);
        }
    }

    /// Compute stats for all recorded scopes.
    pub fn compute(&self) -> Vec<ScopeStats> {
        let mut out = Vec::with_capacity(self.rings.len());
        for (&name, ring) in &self.rings {
            let samples = ring.samples();
            if samples.is_empty() {
                continue;
            }
            let mut sorted: Vec<f64> = samples.to_vec();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let min_us = sorted[0];
            let max_us = sorted[sorted.len() - 1];
            let mean_us = sorted.iter().sum::<f64>() / sorted.len() as f64;
            let p95_idx = ((sorted.len() as f64 * 0.95) as usize).min(sorted.len() - 1);
            let p95_us = sorted[p95_idx];
            let last_us = *samples.last().unwrap_or(&0.0);
            out.push(ScopeStats {
                name: name.to_string(),
                min_us,
                max_us,
                mean_us,
                p95_us,
                last_us,
                sample_count: samples.len(),
            });
        }
        // Sort by name for stable ordering
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    /// Total frames recorded.
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }
}

// ── Report (serializable snapshot) ──────────────────────────────────

/// Serializable profile report for REST endpoints and JS bridge.
#[derive(Debug, Clone, Serialize)]
pub struct ProfileReport {
    pub frame_count: u64,
    pub scopes: Vec<ScopeStats>,
}

// ── Macro ───────────────────────────────────────────────────────────

/// Start a named profiling scope. The timing is recorded when the scope guard
/// is dropped. Compiles to nothing when the `profiling` feature is disabled.
///
/// Usage:
/// ```ignore
/// breakpoint_core::profile!("my_scope");
/// // ... code to measure ...
/// // timing recorded automatically when `_guard` drops
/// ```
#[macro_export]
macro_rules! profile {
    ($name:expr) => {
        let _profile_guard = $crate::profiling::ProfileScope::new($name);
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_records_timing() {
        ProfileFrame::reset();
        {
            let _g = ProfileScope::new("test_scope");
            // Burn a tiny amount of time
            std::hint::black_box(0u64.wrapping_add(1));
        }
        let scopes = ProfileFrame::snapshot();
        assert_eq!(scopes.len(), 1);
        assert_eq!(scopes[0].name, "test_scope");
        assert!(scopes[0].duration_us >= 0.0);
    }

    #[test]
    fn stats_rolling_window() {
        let mut stats = ProfileStats::new(4);
        for i in 0..6 {
            stats.record_frame(&[ScopeTiming {
                name: "tick",
                duration_us: (i as f64) * 100.0,
            }]);
        }
        let computed = stats.compute();
        assert_eq!(computed.len(), 1);
        let tick = &computed[0];
        assert_eq!(tick.name, "tick");
        // Window of 4: values 200, 300, 400, 500
        assert_eq!(tick.sample_count, 4);
        assert!((tick.min_us - 200.0).abs() < 0.01);
        assert!((tick.max_us - 500.0).abs() < 0.01);
    }

    #[test]
    fn profile_macro_works() {
        ProfileFrame::reset();
        {
            profile!("macro_test");
            // Guard drops at end of this block, recording the timing
        }
        let scopes = ProfileFrame::snapshot();
        assert!(scopes.iter().any(|s| s.name == "macro_test"));
    }

    #[test]
    fn report_serializes() {
        let report = ProfileReport {
            frame_count: 42,
            scopes: vec![ScopeStats {
                name: "test".to_string(),
                min_us: 10.0,
                max_us: 100.0,
                mean_us: 55.0,
                p95_us: 95.0,
                last_us: 50.0,
                sample_count: 10,
            }],
        };
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"frame_count\":42"));
    }
}
