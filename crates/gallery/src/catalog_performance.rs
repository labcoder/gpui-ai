//! Catalog-scroll instrumentation types shared by the native scan harness.
//!
//! Unlike [`crate::performance`], which measures prepared isolated viewports,
//! these metrics describe one continuous scripted scroll through the real
//! complete-catalog view (`StoryId::All`). They capture whole-frame costs
//! including story materialization, invalidation-to-draw latency, inter-frame
//! gaps, working-set growth, idle frame demand, and final-story reachability —
//! the composition costs the isolated gate cannot see.

use crate::StoryId;

/// Wall-clock interval between synthetic scroll steps.
pub const SCROLL_STEP: std::time::Duration = std::time::Duration::from_millis(16);
/// Catalog pixels advanced per scroll step. One uniform story row is 320px,
/// so this yields roughly sixteen measured frames per story region.
pub const STEP_DISTANCE_PX: f32 = 20.0;
/// Minimum retained steady draws before a story region counts as visited.
pub const MIN_STEADY_DRAWS_PER_STORY: usize = 10;
/// Unmeasured draws discarded after crossing into a new story region so
/// boundary materialization frames never pollute steady-region statistics.
pub const BOUNDARY_SETTLING_DRAWS: usize = 3;
/// Interval between idle-demand probes while parked on the final story.
pub const IDLE_PROBE_MS: u64 = 100;
/// Any frame at or above this length is treated as a perceptible stall.
pub const STALL_FRAME_NANOS: u64 = 100_000_000;
/// Gate: maximum accepted invalidation-to-draw latency p99 while scrolling.
pub const MAX_SCAN_LATENCY_P99_NANOS: u64 = 150_000_000;
/// Gate: maximum accepted peak working-set bytes on hosts that report it.
pub const MAX_PEAK_WORKING_SET_BYTES: u64 = 400 * 1024 * 1024;
/// Gate: maximum idle draws tolerated while parked on a static story.
pub const MAX_IDLE_DRAWS_WHILE_PARKED: u64 = 10;
/// Hard ceiling on one continuous scan before the run is declared timed out.
pub const SCAN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(180);

/// Whole-frame statistics for one catalog story region.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RegionMetrics {
    /// Retained draw samples in nanoseconds for this region.
    pub samples: Vec<u64>,
}

impl RegionMetrics {
    /// Records one draw sample in nanoseconds.
    pub fn push(&mut self, nanos: u64) {
        self.samples.push(nanos);
    }

    /// Mean draw time in milliseconds.
    pub fn mean_ms(&self) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        self.samples.iter().map(|s| *s as f64).sum::<f64>()
            / self.samples.len() as f64
            / 1_000_000.0
    }

    /// Longest retained draw in milliseconds.
    pub fn max_ms(&self) -> f64 {
        self.samples.iter().copied().max().unwrap_or_default() as f64 / 1_000_000.0
    }

    /// Number of retained draws at or above `nanos`.
    pub fn draws_over(&self, nanos: u64) -> usize {
        self.samples
            .iter()
            .filter(|sample| **sample >= nanos)
            .count()
    }

    /// Whether the region collected enough samples to count as visited.
    pub fn is_visited(&self) -> bool {
        self.samples.len() >= MIN_STEADY_DRAWS_PER_STORY
    }
}

/// Results of one continuous catalog scan.
#[derive(Debug, Clone, Default)]
pub struct CatalogScanReport {
    /// Per-story-region statistics indexed alongside `StoryId::ALL`.
    pub regions: Vec<RegionMetrics>,
    /// Invalidation-to-draw latencies in nanoseconds observed while scrolling.
    pub scan_latencies: Vec<u64>,
    /// Single longest gap between successive frame starts while scrolling.
    pub longest_scroll_gap_nanos: u64,
    /// Peak working-set bytes, when the host reports process memory.
    pub peak_working_set_bytes: Option<u64>,
    /// Settled working-set bytes taken after parking on the final story.
    pub settled_working_set_bytes: Option<u64>,
    /// Draws observed while parked idle on the final static story.
    pub idle_draws_while_parked: u64,
    /// Whether the final story region retained steady samples.
    pub reached_final_story: bool,
    /// Whether the scan hit [`SCAN_TIMEOUT`] before completing.
    pub timed_out: bool,
}

impl CatalogScanReport {
    /// Builds an empty report sized for the full story list.
    pub fn new() -> Self {
        Self {
            regions: vec![RegionMetrics::default(); StoryId::ALL.len()],
            ..Self::default()
        }
    }

    /// Latency p99 in nanoseconds across retained scroll samples.
    pub fn latency_p99_nanos(&self) -> Option<u64> {
        percentile(&self.scan_latencies, 99)
    }

    /// Returns every violated hardware-dependent gate condition.
    ///
    /// Deliberate diagnosis-phase policy: frames between 16.67ms and
    /// [`STALL_FRAME_NANOS`] are reported per region but not gated; only
    /// unvisited regions, perceptible stalls, latency, memory, idle demand,
    /// and reachability fail the gate.
    pub fn gate_failures(&self) -> Vec<String> {
        let mut failures = Vec::new();
        for (story, metrics) in StoryId::ALL.iter().zip(&self.regions) {
            if !metrics.is_visited() {
                failures.push(format!(
                    "story {} was not reached with steady samples",
                    story.slug()
                ));
                continue;
            }
            let stalled = metrics.draws_over(STALL_FRAME_NANOS);
            if stalled > 0 {
                failures.push(format!(
                    "story {} had {stalled} draw(s) at or above {:.1}ms",
                    story.slug(),
                    STALL_FRAME_NANOS as f64 / 1_000_000.0
                ));
            }
        }
        if !self.reached_final_story || self.timed_out {
            failures.push("scan did not finish the catalog".to_string());
        }
        if let Some(p99) = self.latency_p99_nanos()
            && p99 > MAX_SCAN_LATENCY_P99_NANOS
        {
            failures.push(format!(
                "scroll invalidation-to-draw p99 {:.1}ms exceeds the {:.0}ms budget",
                p99 as f64 / 1_000_000.0,
                MAX_SCAN_LATENCY_P99_NANOS as f64 / 1_000_000.0
            ));
        }
        if self.longest_scroll_gap_nanos > STALL_FRAME_NANOS {
            failures.push(format!(
                "longest inter-frame gap while scrolling {:.1}ms exceeds the stall threshold",
                self.longest_scroll_gap_nanos as f64 / 1_000_000.0
            ));
        }
        if let Some(peak) = self.peak_working_set_bytes
            && peak > MAX_PEAK_WORKING_SET_BYTES
        {
            failures.push(format!(
                "peak working set {:.0}MB exceeds the {:.0}MB cap",
                peak as f64 / (1024.0 * 1024.0),
                MAX_PEAK_WORKING_SET_BYTES as f64 / (1024.0 * 1024.0)
            ));
        }
        if self.idle_draws_while_parked > MAX_IDLE_DRAWS_WHILE_PARKED {
            failures.push(format!(
                "{} draws scheduled while parked idle on a static story",
                self.idle_draws_while_parked
            ));
        }
        failures
    }

    /// Prints the full human-readable scan report.
    pub fn print(&self) {
        eprintln!("gpui-ai catalog scan report");
        for (story, metrics) in StoryId::ALL.iter().zip(&self.regions) {
            if metrics.samples.is_empty() {
                eprintln!("  {:<18} UNVISITED", story.title());
                continue;
            }
            eprintln!(
                "  {:<18} draws {:>4}; mean {:>7.3}ms; max {:>8.3}ms; over16.67 {}",
                story.title(),
                metrics.samples.len(),
                metrics.mean_ms(),
                metrics.max_ms(),
                metrics.draws_over(16_666_667),
            );
        }
        if let Some(p99) = self.latency_p99_nanos() {
            eprintln!(
                "  invalidation-to-draw p99: {:.1}ms over {} samples",
                p99 as f64 / 1_000_000.0,
                self.scan_latencies.len()
            );
        }
        eprintln!(
            "  longest inter-frame gap while scrolling: {:.1}ms",
            self.longest_scroll_gap_nanos as f64 / 1_000_000.0
        );
        if let Some(peak) = self.peak_working_set_bytes {
            eprintln!(
                "  peak working set: {:.0}MB",
                peak as f64 / (1024.0 * 1024.0)
            );
        }
        if let Some(settled) = self.settled_working_set_bytes {
            eprintln!(
                "  settled working set: {:.0}MB",
                settled as f64 / (1024.0 * 1024.0)
            );
        }
        eprintln!(
            "  idle draws parked on final story: {} over {}ms of probes",
            self.idle_draws_while_parked,
            IDLE_PROBE_MS * 90
        );
        eprintln!(
            "  reached final story: {}; timed out: {}",
            self.reached_final_story, self.timed_out
        );
    }
}

fn percentile(samples: &[u64], rank_percent: usize) -> Option<u64> {
    if samples.is_empty() {
        return None;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let rank = (rank_percent * sorted.len()).div_ceil(100);
    Some(sorted[(rank - 1).min(sorted.len() - 1)])
}

/// Returns `(peak_working_set, current_working_set)` in bytes on Windows.
#[cfg(all(windows, feature = "performance"))]
pub fn process_memory() -> Option<(u64, u64)> {
    use windows_sys::Win32::System::{
        ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
        Threading::GetCurrentProcess,
    };

    let mut counters = PROCESS_MEMORY_COUNTERS {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        PageFaultCount: 0,
        PeakWorkingSetSize: 0,
        WorkingSetSize: 0,
        QuotaPeakPagedPoolUsage: 0,
        QuotaPagedPoolUsage: 0,
        QuotaNonPagedPoolUsage: 0,
        QuotaPeakNonPagedPoolUsage: 0,
        PagefileUsage: 0,
        PeakPagefileUsage: 0,
    };
    let ok = unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut counters,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        )
    };
    if ok != 0 {
        Some((
            counters.PeakWorkingSetSize as u64,
            counters.WorkingSetSize as u64,
        ))
    } else {
        None
    }
}

/// Stub memory probe for hosts without a Windows process-status API.
#[cfg(not(all(windows, feature = "performance")))]
pub fn process_memory() -> Option<(u64, u64)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn completed_report() -> CatalogScanReport {
        let mut report = CatalogScanReport::new();
        for metrics in &mut report.regions {
            for _ in 0..MIN_STEADY_DRAWS_PER_STORY {
                metrics.push(4_000_000);
            }
        }
        report.scan_latencies = vec![20_000_000; 50];
        report.reached_final_story = true;
        report.idle_draws_while_parked = 0;
        report.longest_scroll_gap_nanos = 30_000_000;
        report
    }

    #[test]
    fn healthy_report_passes_every_gate() {
        assert!(completed_report().gate_failures().is_empty());
    }

    #[test]
    fn unvisited_and_stalling_regions_fail_the_gate() {
        let mut report = completed_report();
        let records = StoryId::ALL
            .iter()
            .position(|story| *story == StoryId::RecordsTable)
            .expect("records table is a catalog story");
        let filter = StoryId::ALL
            .iter()
            .position(|story| *story == StoryId::FilterTable)
            .expect("filter table is a catalog story");
        report.regions[records] = RegionMetrics::default();
        report.regions[filter].push(STALL_FRAME_NANOS + 1);
        let failures = report.gate_failures();
        assert!(failures.iter().any(|f| f.contains("records-table")));
        assert!(failures.iter().any(|f| f.contains("filter-table")));
    }

    #[test]
    fn incomplete_scans_latency_memory_and_idle_demand_fail_the_gate() {
        let mut report = completed_report();
        report.reached_final_story = false;
        assert!(
            report
                .gate_failures()
                .iter()
                .any(|f| f.contains("did not finish"))
        );

        let mut report = completed_report();
        report.scan_latencies = vec![MAX_SCAN_LATENCY_P99_NANOS + 1; 50];
        assert!(report.gate_failures().iter().any(|f| f.contains("p99")));

        let mut report = completed_report();
        report.peak_working_set_bytes = Some(MAX_PEAK_WORKING_SET_BYTES + 1);
        assert!(
            report
                .gate_failures()
                .iter()
                .any(|f| f.contains("working set"))
        );

        let mut report = completed_report();
        report.idle_draws_while_parked = MAX_IDLE_DRAWS_WHILE_PARKED + 1;
        assert!(report.gate_failures().iter().any(|f| f.contains("idle")));

        let mut report = completed_report();
        report.timed_out = true;
        assert!(
            report
                .gate_failures()
                .iter()
                .any(|f| f.contains("did not finish"))
        );
    }

    #[test]
    fn latency_p99_is_order_statistic_not_maximum() {
        let mut report = completed_report();
        report.scan_latencies = vec![1_000_000; 99];
        report.scan_latencies.push(MAX_SCAN_LATENCY_P99_NANOS + 1);
        assert_eq!(report.latency_p99_nanos(), Some(1_000_000));
    }
}
