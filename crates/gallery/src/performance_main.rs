use gallery::{
    Gallery, GalleryTheme, StoryId, init, open_gallery_with_theme,
    performance::{MIN_DRAW_SAMPLES, PERFORMANCE_VIEWPORTS, PerformanceReport},
};
use gpui::{
    AsyncApp, Entity, Global, Task,
    profiler::{FrameEvent, FrameTimingCollector, set_trace_enabled},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicI32, Ordering},
    },
    time::{Duration, Instant},
};

const WARMUP_DRAWS: usize = 30;
const SETTLING_DRAWS_PER_VIEWPORT: usize = 5;
const DRIVE_INTERVAL: Duration = Duration::from_nanos(8_333_333);
const RUN_TIMEOUT: Duration = Duration::from_secs(45);
const FAILURE_EXIT_CODE: i32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SampleDestination {
    Setup(usize),
    Steady(usize),
}

struct MeasurementPlan {
    viewport: usize,
    settling_draws_remaining: usize,
    viewport_steady_draws: usize,
    steady_draws: usize,
}

impl MeasurementPlan {
    fn new() -> Self {
        Self {
            viewport: 0,
            settling_draws_remaining: SETTLING_DRAWS_PER_VIEWPORT,
            viewport_steady_draws: 0,
            steady_draws: 0,
        }
    }

    fn steady_draws(&self) -> usize {
        self.steady_draws
    }

    fn record_draw(&mut self) -> (SampleDestination, Option<usize>) {
        if self.settling_draws_remaining > 0 {
            self.settling_draws_remaining -= 1;
            return (SampleDestination::Setup(self.viewport), None);
        }

        let viewport = self.viewport;
        self.viewport_steady_draws += 1;
        self.steady_draws += 1;
        let steady_draws_per_viewport = MIN_DRAW_SAMPLES / PERFORMANCE_VIEWPORTS.len();
        let completed_viewport = self.viewport_steady_draws == steady_draws_per_viewport;
        if completed_viewport {
            self.viewport += 1;
            self.viewport_steady_draws = 0;
            self.settling_draws_remaining = SETTLING_DRAWS_PER_VIEWPORT;
        }

        let next_viewport = (completed_viewport && self.viewport < PERFORMANCE_VIEWPORTS.len())
            .then_some(self.viewport);
        (SampleDestination::Steady(viewport), next_viewport)
    }
}

struct PerformanceTask {
    _task: Task<()>,
}

impl Global for PerformanceTask {}

fn main() {
    let exit_code = Arc::new(AtomicI32::new(0));
    let task_exit_code = exit_code.clone();

    gpui_platform::application()
        .with_assets(gpui_component_assets::Assets)
        .run(move |cx| {
            init(cx);
            set_trace_enabled(true);
            let gallery = open_gallery_with_theme(StoryId::All, GalleryTheme::Dark, cx);
            let task = cx.spawn(async move |cx| {
                run_performance_measurement(gallery, task_exit_code, cx).await;
            });
            cx.set_global(PerformanceTask { _task: task });
        });

    let code = exit_code.load(Ordering::Relaxed);
    if code != 0 {
        std::process::exit(code);
    }
}

async fn run_performance_measurement(
    gallery: Entity<Gallery>,
    exit_code: Arc<AtomicI32>,
    cx: &mut AsyncApp,
) {
    let started_at = Instant::now();
    let mut collector = FrameTimingCollector::new();
    let mut warmup_remaining = WARMUP_DRAWS;
    let mut draw_samples = Vec::with_capacity(MIN_DRAW_SAMPLES);
    let mut present_samples = Vec::with_capacity(MIN_DRAW_SAMPLES);
    let mut viewport_draw_samples = vec![Vec::new(); PERFORMANCE_VIEWPORTS.len()];
    let mut setup_draw_samples = vec![Vec::new(); PERFORMANCE_VIEWPORTS.len()];
    let mut plan = MeasurementPlan::new();

    gallery.update(cx, |gallery, cx| {
        gallery.scroll_catalog_to(PERFORMANCE_VIEWPORTS[0], cx);
    });

    while plan.steady_draws() < MIN_DRAW_SAMPLES && started_at.elapsed() < RUN_TIMEOUT {
        cx.background_executor().timer(DRIVE_INTERVAL).await;
        let mut next_viewport = None;

        for event in collector.collect_unseen() {
            match event {
                FrameEvent::Draw(_) if warmup_remaining > 0 => warmup_remaining -= 1,
                FrameEvent::Draw(timing) if plan.steady_draws() < MIN_DRAW_SAMPLES => {
                    let draw_nanos = duration_nanos(timing.draw_duration());
                    let (destination, viewport_to_scroll) = plan.record_draw();
                    match destination {
                        SampleDestination::Setup(viewport) => {
                            setup_draw_samples[viewport].push(draw_nanos);
                        }
                        SampleDestination::Steady(viewport) => {
                            draw_samples.push(draw_nanos);
                            viewport_draw_samples[viewport].push(draw_nanos);
                        }
                    }
                    if viewport_to_scroll.is_some() {
                        next_viewport = viewport_to_scroll;
                        break;
                    }
                }
                FrameEvent::Present(timing) if warmup_remaining == 0 => {
                    if let Some(interval) = timing.animation_interval {
                        present_samples.push(duration_nanos(interval));
                    }
                }
                FrameEvent::Draw(_) | FrameEvent::Present(_) => {}
            }
        }

        if let Some(viewport) = next_viewport {
            gallery.update(cx, |gallery, cx| {
                gallery.scroll_catalog_to(PERFORMANCE_VIEWPORTS[viewport], cx);
            });
        } else {
            cx.refresh();
        }
    }

    set_trace_enabled(false);
    let report = PerformanceReport::from_samples(draw_samples, present_samples);
    report.print();
    let viewports = PERFORMANCE_VIEWPORTS
        .iter()
        .map(|story| story.title())
        .collect::<Vec<_>>()
        .join(", ");
    eprintln!("  viewports: {viewports}");
    for (story, samples) in PERFORMANCE_VIEWPORTS.iter().zip(&viewport_draw_samples) {
        print_draw_distribution(&format!("{} steady", story.title()), samples);
    }
    for (story, samples) in PERFORMANCE_VIEWPORTS.iter().zip(&setup_draw_samples) {
        print_draw_distribution(&format!("{} setup", story.title()), samples);
    }

    let failures = report.gate_failures();
    if started_at.elapsed() >= RUN_TIMEOUT && report.draw.samples < MIN_DRAW_SAMPLES {
        eprintln!("  gate failed: measurement timed out after {RUN_TIMEOUT:?}");
    }
    for failure in &failures {
        eprintln!("  gate failed: {failure}");
    }
    if failures.is_empty() && report.draw.samples >= MIN_DRAW_SAMPLES {
        eprintln!("  gate passed");
    } else {
        exit_code.store(FAILURE_EXIT_CODE, Ordering::Relaxed);
    }

    cx.update(|cx| cx.quit());
}

fn duration_nanos(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

fn print_draw_distribution(label: &str, draw_samples: &[u64]) {
    let report = PerformanceReport::from_samples(draw_samples.to_vec(), Vec::new());
    let over_8_33_ms = report.draw_over_8_33_ms;
    let draw = report.draw;
    eprintln!(
        "  {label}: {} draws; mean {:.3}ms; p95 {:.3}ms; p99 {:.3}ms; max {:.3}ms; over 8.333ms {over_8_33_ms}",
        draw.samples,
        draw.mean_nanos / 1_000_000.0,
        draw.p95_nanos as f64 / 1_000_000.0,
        draw.p99_nanos as f64 / 1_000_000.0,
        draw.max_nanos as f64 / 1_000_000.0,
    );
}

#[cfg(test)]
mod tests {
    use super::{MeasurementPlan, SETTLING_DRAWS_PER_VIEWPORT, SampleDestination};
    use gallery::performance::{MIN_DRAW_SAMPLES, PERFORMANCE_VIEWPORTS};

    #[test]
    fn measurement_plan_settles_each_viewport_before_equal_steady_samples() {
        let mut plan = MeasurementPlan::new();
        let steady_draws_per_viewport = MIN_DRAW_SAMPLES / PERFORMANCE_VIEWPORTS.len();

        for viewport in 0..PERFORMANCE_VIEWPORTS.len() {
            for _ in 0..SETTLING_DRAWS_PER_VIEWPORT {
                assert_eq!(
                    plan.record_draw(),
                    (SampleDestination::Setup(viewport), None)
                );
            }
            for draw in 0..steady_draws_per_viewport {
                let next_viewport = (draw + 1 == steady_draws_per_viewport)
                    .then_some(viewport + 1)
                    .filter(|next| *next < PERFORMANCE_VIEWPORTS.len());
                assert_eq!(
                    plan.record_draw(),
                    (SampleDestination::Steady(viewport), next_viewport)
                );
            }
        }

        assert_eq!(plan.steady_draws(), MIN_DRAW_SAMPLES);
    }
}
