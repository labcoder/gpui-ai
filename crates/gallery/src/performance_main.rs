use gallery::{
    Gallery, GalleryTheme, StoryId, init, open_gallery_with_theme,
    performance::{
        AMBIENT_VIEWPORTS, DRIVEN_VIEWPORTS, FILTER_SETTLING_DRAWS, FILTER_TRANSITION_DRAWS,
        MAX_AMBIENT_P95_DRAW_NANOS, MAX_AMBIENT_P99_DRAW_NANOS, MAX_P99_DRAW_NANOS,
        MAX_VISIBLE_FILTER_ROWS, MIN_DRAW_SAMPLES, PERFORMANCE_VIEWPORTS, PerformanceReport,
        SIXTY_HZ_DRAW_NANOS, STEADY_DRAWS_PER_VIEWPORT,
    },
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
    Transition(usize),
    Steady(usize),
}

struct MeasurementPlan {
    viewport: usize,
    settling_draws_remaining: usize,
    viewport_steady_draws: usize,
    steady_draws: usize,
    filter_transition_draws: usize,
    filter_settling_draws_remaining: usize,
}

impl MeasurementPlan {
    fn new() -> Self {
        Self {
            viewport: 0,
            settling_draws_remaining: SETTLING_DRAWS_PER_VIEWPORT,
            viewport_steady_draws: 0,
            steady_draws: 0,
            filter_transition_draws: 0,
            filter_settling_draws_remaining: 0,
        }
    }

    fn steady_draws(&self) -> usize {
        self.steady_draws
    }

    fn record_draw(&mut self) -> (SampleDestination, Option<usize>, bool) {
        if self.settling_draws_remaining > 0 {
            self.settling_draws_remaining -= 1;
            return (SampleDestination::Setup(self.viewport), None, false);
        }

        if DRIVEN_VIEWPORTS.contains(&PERFORMANCE_VIEWPORTS[self.viewport])
            && self.filter_transition_draws < FILTER_TRANSITION_DRAWS
        {
            self.filter_transition_draws += 1;
            let toggle_projection = self.filter_transition_draws % 8 == 1;
            if self.filter_transition_draws == FILTER_TRANSITION_DRAWS {
                self.filter_settling_draws_remaining = FILTER_SETTLING_DRAWS;
            }
            return (
                SampleDestination::Transition(self.viewport),
                None,
                toggle_projection,
            );
        }

        if self.filter_settling_draws_remaining > 0 {
            self.filter_settling_draws_remaining -= 1;
            return (SampleDestination::Setup(self.viewport), None, false);
        }

        let viewport = self.viewport;
        self.viewport_steady_draws += 1;
        self.steady_draws += 1;
        let completed_viewport = self.viewport_steady_draws == STEADY_DRAWS_PER_VIEWPORT;
        if completed_viewport {
            self.viewport += 1;
            self.viewport_steady_draws = 0;
            self.settling_draws_remaining = SETTLING_DRAWS_PER_VIEWPORT;
            self.filter_transition_draws = 0;
        }

        let next_viewport = (completed_viewport && self.viewport < PERFORMANCE_VIEWPORTS.len())
            .then_some(self.viewport);
        (SampleDestination::Steady(viewport), next_viewport, false)
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
            let gallery = open_gallery_with_theme(StoryId::All, GalleryTheme::DARK, cx);
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
    let mut driven_transition_draw_samples = vec![Vec::new(); PERFORMANCE_VIEWPORTS.len()];
    let mut filter_projection_is_filtered = false;
    let mut maximum_visible_filter_rows = 0;
    let mut maximum_animating_filter_rows = 0;
    let mut plan = MeasurementPlan::new();
    let mut current_viewport = 0usize;

    gallery.update(cx, |gallery, cx| {
        gallery.prepare_performance_viewport(PERFORMANCE_VIEWPORTS[0], cx);
    });

    while plan.steady_draws() < MIN_DRAW_SAMPLES && started_at.elapsed() < RUN_TIMEOUT {
        cx.background_executor().timer(DRIVE_INTERVAL).await;
        let mut next_viewport = None;
        let mut toggle_filter_projection = false;

        for event in collector.collect_unseen() {
            match event {
                FrameEvent::Draw(_) if warmup_remaining > 0 => warmup_remaining -= 1,
                FrameEvent::Draw(timing) if plan.steady_draws() < MIN_DRAW_SAMPLES => {
                    let draw_nanos = duration_nanos(timing.draw_duration());
                    let (destination, viewport_to_scroll, toggle_filter) = plan.record_draw();
                    match destination {
                        SampleDestination::Setup(viewport) => {
                            setup_draw_samples[viewport].push(draw_nanos);
                        }
                        SampleDestination::Steady(viewport) => {
                            draw_samples.push(draw_nanos);
                            viewport_draw_samples[viewport].push(draw_nanos);
                        }
                        SampleDestination::Transition(viewport) => {
                            driven_transition_draw_samples[viewport].push(draw_nanos);
                        }
                    }
                    toggle_filter_projection ^= toggle_filter;
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

        if let Some((visible_rows, animating_rows)) =
            gallery.read_with(cx, |gallery, cx| gallery.performance_filter_counts(cx))
        {
            maximum_visible_filter_rows = maximum_visible_filter_rows.max(visible_rows);
            maximum_animating_filter_rows = maximum_animating_filter_rows.max(animating_rows);
        }

        if let Some(viewport) = next_viewport {
            current_viewport = viewport;
            gallery.update(cx, |gallery, cx| {
                gallery.prepare_performance_viewport(PERFORMANCE_VIEWPORTS[viewport], cx);
            });
        } else if toggle_filter_projection {
            filter_projection_is_filtered = !filter_projection_is_filtered;
            gallery.update(cx, |gallery, cx| {
                match PERFORMANCE_VIEWPORTS[current_viewport] {
                    StoryId::Thinking => {
                        gallery.set_performance_thinking_open(filter_projection_is_filtered, cx)
                    }
                    StoryId::ToolCalls => {
                        gallery.set_performance_tools_open(filter_projection_is_filtered, cx)
                    }
                    _ => {
                        gallery.set_performance_filter_projection(filter_projection_is_filtered, cx)
                    }
                }
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
        print_draw_distribution(&format!("{} setup/settling", story.title()), samples);
    }

    for driven in DRIVEN_VIEWPORTS {
        let viewport = PERFORMANCE_VIEWPORTS
            .iter()
            .position(|story| *story == driven)
            .expect("every driven viewport is measured");
        print_draw_distribution(
            &format!("{} driven transition", driven.title()),
            &driven_transition_draw_samples[viewport],
        );
    }
    eprintln!(
        "  Filter table bounded work: maximum visible constructed/paint-eligible rows {maximum_visible_filter_rows}/1000; maximum rows with motion state {maximum_animating_filter_rows}"
    );

    let mut failures = report.gate_failures();
    for driven in DRIVEN_VIEWPORTS {
        let viewport = PERFORMANCE_VIEWPORTS
            .iter()
            .position(|story| *story == driven)
            .expect("every driven viewport is measured");
        let transition_report = PerformanceReport::from_samples(
            driven_transition_draw_samples[viewport].clone(),
            Vec::new(),
        );
        let title = driven.title();
        if transition_report.draw.samples != FILTER_TRANSITION_DRAWS {
            failures.push(format!(
                "{title} transition requires {FILTER_TRANSITION_DRAWS} samples; observed {}",
                transition_report.draw.samples
            ));
        }
        // The plan's driven budgets: p95 within 4.0 ms and p99 within
        // 6.0 ms are targets, reported when missed; p99 over 8.333 ms or a
        // long frame is the may-not-ship line and fails the gate.
        if transition_report.draw.p95_nanos > MAX_AMBIENT_P95_DRAW_NANOS
            || transition_report.draw.p99_nanos > MAX_AMBIENT_P99_DRAW_NANOS
        {
            eprintln!(
                "  {title} transition misses the driven target (p95 {:.3}ms / p99 {:.3}ms vs 4.0/6.0ms)",
                transition_report.draw.p95_nanos as f64 / 1_000_000.0,
                transition_report.draw.p99_nanos as f64 / 1_000_000.0,
            );
        }
        if transition_report.draw.p99_nanos > MAX_P99_DRAW_NANOS {
            failures.push(format!(
                "{title} transition draw p99 {:.3}ms exceeds the 8.333ms budget",
                transition_report.draw.p99_nanos as f64 / 1_000_000.0
            ));
        }
        if transition_report.draw.max_nanos > SIXTY_HZ_DRAW_NANOS {
            failures.push(format!(
                "{title} transition max {:.3}ms exceeds the 16.667ms long-frame threshold",
                transition_report.draw.max_nanos as f64 / 1_000_000.0
            ));
        }
    }
    // The ambient viewports' steady state is their driven state — the clocks
    // never rest while mounted — so they answer to the driven-scenario
    // budgets, not only the global gate. Counts are already printed above,
    // even when the percentiles pass.
    for ambient in AMBIENT_VIEWPORTS {
        let viewport = PERFORMANCE_VIEWPORTS
            .iter()
            .position(|story| *story == ambient)
            .expect("every ambient viewport is measured");
        let ambient_report =
            PerformanceReport::from_samples(viewport_draw_samples[viewport].clone(), Vec::new());
        if ambient_report.draw.p95_nanos > MAX_AMBIENT_P95_DRAW_NANOS {
            failures.push(format!(
                "{} steady draw p95 {:.3}ms exceeds the 4.0ms driven budget",
                ambient.title(),
                ambient_report.draw.p95_nanos as f64 / 1_000_000.0
            ));
        }
        if ambient_report.draw.p99_nanos > MAX_AMBIENT_P99_DRAW_NANOS {
            failures.push(format!(
                "{} steady draw p99 {:.3}ms exceeds the 6.0ms driven budget",
                ambient.title(),
                ambient_report.draw.p99_nanos as f64 / 1_000_000.0
            ));
        }
        if ambient_report.draw.max_nanos > SIXTY_HZ_DRAW_NANOS {
            failures.push(format!(
                "{} steady max {:.3}ms exceeds the 16.667ms long-frame threshold",
                ambient.title(),
                ambient_report.draw.max_nanos as f64 / 1_000_000.0
            ));
        }
    }

    if maximum_visible_filter_rows == 0 || maximum_visible_filter_rows >= MAX_VISIBLE_FILTER_ROWS {
        failures.push(format!(
            "Filter visible construction must be 1..{MAX_VISIBLE_FILTER_ROWS}; observed {maximum_visible_filter_rows}"
        ));
    }
    if maximum_animating_filter_rows == 0
        || maximum_animating_filter_rows >= MAX_VISIBLE_FILTER_ROWS
    {
        failures.push(format!(
            "Filter motion state must be 1..{MAX_VISIBLE_FILTER_ROWS}; observed {maximum_animating_filter_rows}"
        ));
    }
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
    use gallery::performance::{
        DRIVEN_VIEWPORTS, FILTER_SETTLING_DRAWS, FILTER_TRANSITION_DRAWS, MIN_DRAW_SAMPLES,
        PERFORMANCE_VIEWPORTS, STEADY_DRAWS_PER_VIEWPORT,
    };

    #[test]
    fn measurement_plan_settles_each_viewport_before_equal_steady_samples() {
        let mut plan = MeasurementPlan::new();
        let steady_draws_per_viewport = STEADY_DRAWS_PER_VIEWPORT;

        for (viewport, story) in PERFORMANCE_VIEWPORTS.iter().enumerate() {
            for _ in 0..SETTLING_DRAWS_PER_VIEWPORT {
                assert_eq!(
                    plan.record_draw(),
                    (SampleDestination::Setup(viewport), None, false)
                );
            }
            if DRIVEN_VIEWPORTS.contains(story) {
                for draw in 1..=FILTER_TRANSITION_DRAWS {
                    assert_eq!(
                        plan.record_draw(),
                        (SampleDestination::Transition(viewport), None, draw % 8 == 1,)
                    );
                }
                for _ in 0..FILTER_SETTLING_DRAWS {
                    assert_eq!(
                        plan.record_draw(),
                        (SampleDestination::Setup(viewport), None, false)
                    );
                }
            }
            for draw in 0..steady_draws_per_viewport {
                let next_viewport = (draw + 1 == steady_draws_per_viewport)
                    .then_some(viewport + 1)
                    .filter(|next| *next < PERFORMANCE_VIEWPORTS.len());
                assert_eq!(
                    plan.record_draw(),
                    (SampleDestination::Steady(viewport), next_viewport, false,)
                );
            }
        }

        assert_eq!(plan.steady_draws(), MIN_DRAW_SAMPLES);
        assert_eq!(
            plan.steady_draws(),
            PERFORMANCE_VIEWPORTS.len() * STEADY_DRAWS_PER_VIEWPORT
        );
    }
}
