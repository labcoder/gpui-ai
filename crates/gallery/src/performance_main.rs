use gallery::{
    Gallery, GalleryTheme, StoryId, init, open_gallery_with_theme,
    performance::{MIN_DRAW_SAMPLES, PerformanceReport},
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
const DRIVE_INTERVAL: Duration = Duration::from_nanos(8_333_333);
const RUN_TIMEOUT: Duration = Duration::from_secs(45);
const FAILURE_EXIT_CODE: i32 = 1;

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
    let mut viewport_phase = 0;

    while draw_samples.len() < MIN_DRAW_SAMPLES && started_at.elapsed() < RUN_TIMEOUT {
        cx.background_executor().timer(DRIVE_INTERVAL).await;

        for event in collector.collect_unseen() {
            match event {
                FrameEvent::Draw(_) if warmup_remaining > 0 => warmup_remaining -= 1,
                FrameEvent::Draw(timing) if draw_samples.len() < MIN_DRAW_SAMPLES => {
                    draw_samples.push(duration_nanos(timing.draw_duration()));
                }
                FrameEvent::Present(timing) if warmup_remaining == 0 => {
                    if let Some(interval) = timing.animation_interval {
                        present_samples.push(duration_nanos(interval));
                    }
                }
                FrameEvent::Draw(_) | FrameEvent::Present(_) => {}
            }
        }

        let next_phase = match draw_samples.len() {
            0..100 => 0,
            100..200 => 1,
            _ => 2,
        };
        gallery.update(cx, |gallery, cx| {
            if viewport_phase != next_phase {
                viewport_phase = next_phase;
                let story = match viewport_phase {
                    1 => StoryId::Search,
                    2 => StoryId::Approval,
                    _ => StoryId::Loading,
                };
                gallery.scroll_catalog_to(story, cx);
            } else {
                cx.notify();
            }
        });
    }

    set_trace_enabled(false);
    let report = PerformanceReport::from_samples(draw_samples, present_samples);
    report.print();
    eprintln!("  viewports: loading/live, search/mixed, approval/static");

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
