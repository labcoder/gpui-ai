//! Native gallery frame-budget reporting.

use crate::StoryId;

/// Equal number of steady-state draws retained for every representative viewport.
pub const STEADY_DRAWS_PER_VIEWPORT: usize = 100;
/// Driven Filter Table draws retained while its controlled projection is changing.
pub const FILTER_TRANSITION_DRAWS: usize = 40;
/// Unmeasured Filter Table draws allowed after the final change before idle sampling.
pub const FILTER_SETTLING_DRAWS: usize = 30;
/// Maximum constructed/paint-eligible rows accepted for the 1,000-row Filter Table.
pub const MAX_VISIBLE_FILTER_ROWS: usize = 64;
/// Representative catalog viewports measured by the native frame-budget gate.
pub const PERFORMANCE_VIEWPORTS: [StoryId; 9] = [
    StoryId::Loading,
    StoryId::StreamingText,
    StoryId::Approval,
    StoryId::PromptBar,
    StoryId::Chat,
    StoryId::FilterTable,
    StoryId::RecordsTable,
    StoryId::DiffTable,
    StoryId::ComparisonTable,
];
/// Minimum number of measured draws required by the performance gate.
pub const MIN_DRAW_SAMPLES: usize = PERFORMANCE_VIEWPORTS.len() * STEADY_DRAWS_PER_VIEWPORT;
/// Maximum accepted 99th-percentile draw time for a 120 Hz frame budget.
pub const MAX_P99_DRAW_NANOS: u64 = 8_333_333;
/// Threshold used to identify frames longer than a 60 Hz frame budget.
pub const SIXTY_HZ_DRAW_NANOS: u64 = 16_666_667;

/// Summary statistics for one set of nanosecond samples.
#[derive(Debug, Clone, PartialEq)]
pub struct Distribution {
    /// Number of samples.
    pub samples: usize,
    /// Arithmetic mean in nanoseconds.
    pub mean_nanos: f64,
    /// 50th-percentile sample in nanoseconds.
    pub p50_nanos: u64,
    /// 95th-percentile sample in nanoseconds.
    pub p95_nanos: u64,
    /// 99th-percentile sample in nanoseconds.
    pub p99_nanos: u64,
    /// Maximum sample in nanoseconds.
    pub max_nanos: u64,
}

impl Distribution {
    fn from_samples(mut samples: Vec<u64>) -> Self {
        samples.sort_unstable();
        let count = samples.len();
        let mean_nanos = if count == 0 {
            0.0
        } else {
            samples.iter().map(|sample| *sample as f64).sum::<f64>() / count as f64
        };

        Self {
            samples: count,
            mean_nanos,
            p50_nanos: percentile(&samples, 50),
            p95_nanos: percentile(&samples, 95),
            p99_nanos: percentile(&samples, 99),
            max_nanos: samples.last().copied().unwrap_or_default(),
        }
    }
}

/// Draw and presentation results collected from the native gallery.
#[derive(Debug, Clone, PartialEq)]
pub struct PerformanceReport {
    /// Window draw-time distribution.
    pub draw: Distribution,
    /// Consecutive presentation-interval distribution.
    pub present: Distribution,
    /// Draws longer than one 120 Hz frame budget.
    pub draw_over_8_33_ms: usize,
    /// Draws longer than one 60 Hz frame budget.
    pub draw_over_16_67_ms: usize,
}

impl PerformanceReport {
    /// Builds a report from raw nanosecond samples.
    pub fn from_samples(draw_samples: Vec<u64>, present_samples: Vec<u64>) -> Self {
        let draw_over_8_33_ms = draw_samples
            .iter()
            .filter(|sample| **sample > MAX_P99_DRAW_NANOS)
            .count();
        let draw_over_16_67_ms = draw_samples
            .iter()
            .filter(|sample| **sample > SIXTY_HZ_DRAW_NANOS)
            .count();

        Self {
            draw: Distribution::from_samples(draw_samples),
            present: Distribution::from_samples(present_samples),
            draw_over_8_33_ms,
            draw_over_16_67_ms,
        }
    }

    /// Returns every violated hardware-dependent gate condition.
    pub fn gate_failures(&self) -> Vec<String> {
        let mut failures = Vec::new();
        if self.draw.samples < MIN_DRAW_SAMPLES {
            failures.push(format!(
                "requires at least {MIN_DRAW_SAMPLES} draw samples; observed {}",
                self.draw.samples
            ));
        }
        if self.draw.p99_nanos > MAX_P99_DRAW_NANOS {
            failures.push(format!(
                "draw p99 {:.3}ms exceeds the 8.333ms budget",
                nanos_to_ms(self.draw.p99_nanos)
            ));
        }
        if self.draw.samples > 0 && self.draw_over_16_67_ms * 100 >= self.draw.samples {
            failures.push(format!(
                "{} of {} draws exceeded 16.667ms; the limit is fewer than 1%",
                self.draw_over_16_67_ms, self.draw.samples
            ));
        }
        failures
    }

    /// Prints a compact human-readable report.
    pub fn print(&self) {
        eprintln!("mighty-gpui native performance report");
        print_distribution("draw", &self.draw);
        if self.present.samples > 0 {
            print_distribution("present interval (display-dependent)", &self.present);
        }
        eprintln!(
            "  draws over 8.333ms: {}/{}",
            self.draw_over_8_33_ms, self.draw.samples
        );
        eprintln!(
            "  draws over 16.667ms: {}/{}",
            self.draw_over_16_67_ms, self.draw.samples
        );
    }
}

fn percentile(samples: &[u64], percentile: usize) -> u64 {
    if samples.is_empty() {
        return 0;
    }
    let rank = (percentile * samples.len()).div_ceil(100);
    samples[rank.saturating_sub(1).min(samples.len() - 1)]
}

fn print_distribution(label: &str, distribution: &Distribution) {
    eprintln!("  {label} samples: {}", distribution.samples);
    eprintln!("    mean: {:.3}ms", distribution.mean_nanos / 1_000_000.0);
    eprintln!("    p50: {:.3}ms", nanos_to_ms(distribution.p50_nanos));
    eprintln!("    p95: {:.3}ms", nanos_to_ms(distribution.p95_nanos));
    eprintln!("    p99: {:.3}ms", nanos_to_ms(distribution.p99_nanos));
    eprintln!("    max: {:.3}ms", nanos_to_ms(distribution.max_nanos));
}

fn nanos_to_ms(nanos: u64) -> f64 {
    nanos as f64 / 1_000_000.0
}

#[cfg(test)]
mod tests {
    use crate::StoryId;

    use super::{
        MAX_P99_DRAW_NANOS, MIN_DRAW_SAMPLES, PERFORMANCE_VIEWPORTS, PerformanceReport,
        STEADY_DRAWS_PER_VIEWPORT,
    };

    #[test]
    fn performance_viewports_cover_animation_streaming_and_idle_work() {
        assert_eq!(
            super::PERFORMANCE_VIEWPORTS,
            [
                StoryId::Loading,
                StoryId::StreamingText,
                StoryId::Approval,
                StoryId::PromptBar,
                StoryId::Chat,
                StoryId::FilterTable,
                StoryId::RecordsTable,
                StoryId::DiffTable,
                StoryId::ComparisonTable,
            ]
        );
        assert_eq!(
            MIN_DRAW_SAMPLES,
            PERFORMANCE_VIEWPORTS.len() * STEADY_DRAWS_PER_VIEWPORT
        );
    }

    #[test]
    fn representative_draw_samples_pass_the_120_hz_gate() {
        let mut draws = vec![4_000_000; MIN_DRAW_SAMPLES];
        draws[MIN_DRAW_SAMPLES - 1] = 9_000_000;

        let report = PerformanceReport::from_samples(draws, vec![8_333_333; 20]);

        assert_eq!(report.draw.samples, MIN_DRAW_SAMPLES);
        assert_eq!(report.draw.p99_nanos, 4_000_000);
        assert_eq!(report.draw_over_8_33_ms, 1);
        assert!(report.gate_failures().is_empty());
    }

    #[test]
    fn gate_rejects_too_few_samples_slow_p99_and_one_percent_long_frames() {
        let too_few =
            PerformanceReport::from_samples(vec![4_000_000; MIN_DRAW_SAMPLES - 1], Vec::new());
        assert!(!too_few.gate_failures().is_empty());

        let slow = PerformanceReport::from_samples(
            vec![MAX_P99_DRAW_NANOS + 1; MIN_DRAW_SAMPLES],
            Vec::new(),
        );
        assert!(!slow.gate_failures().is_empty());

        let mut long_frames = vec![4_000_000; MIN_DRAW_SAMPLES];
        long_frames[..MIN_DRAW_SAMPLES / 100].fill(17_000_000);
        let long_frames = PerformanceReport::from_samples(long_frames, Vec::new());
        assert!(!long_frames.gate_failures().is_empty());
    }
}
