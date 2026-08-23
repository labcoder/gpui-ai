//! Release-only catalog-scan harness.
//!
//! Drives one continuous scripted scroll through the real complete-catalog
//! view (`StoryId::All`) — the composition path the isolated-viewport gate
//! never exercises — recording whole-frame draw costs per story region,
//! invalidation-to-draw latency, frameless gaps while scrolling, working-set
//! growth, idle frame demand on a static story, and final-story reachability.
//!
//! Run: `npm run test:catalog`

use gallery::{
    Gallery, GalleryTheme, StoryId,
    catalog_performance::{
        self, BOUNDARY_SETTLING_DRAWS, CatalogScanReport, IDLE_PROBE_MS,
        MIN_STEADY_DRAWS_PER_STORY, SCAN_TIMEOUT, SCROLL_STEP, STEP_DISTANCE_PX, process_memory,
    },
    init, open_gallery_with_theme,
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

const FAILURE_EXIT_CODE: i32 = 1;
/// Maximum scroll attempts per region before declaring it unmeasurable.
const REGION_STEP_CAP: usize = 60;
/// A scroll phase longer than this without any drawn frame is a stall.
const FRAMELESS_STALL_NANOS: u64 = 100_000_000;

struct ScanTask {
    _task: Task<()>,
}

impl Global for ScanTask {}

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
                run_catalog_scan(gallery, task_exit_code, cx).await;
            });
            cx.set_global(ScanTask { _task: task });
        });

    let code = exit_code.load(Ordering::Relaxed);
    if code != 0 {
        std::process::exit(code);
    }
}

async fn run_catalog_scan(gallery: Entity<Gallery>, exit_code: Arc<AtomicI32>, cx: &mut AsyncApp) {
    let started_at = Instant::now();
    let mut collector = FrameTimingCollector::new();
    let mut report = CatalogScanReport::new();
    let mut attributed_region = 0usize;
    let mut settling_remaining;
    let mut last_frame_seen_at: Option<Instant> = None;
    let mut peak_memory = process_memory();

    eprintln!(
        "catalog scan: visiting {} story regions",
        StoryId::ALL.len()
    );

    // Deterministic traversal: simulated streams change row heights while
    // scrolling, which pins the virtual-list anchor and races the scan.
    gallery.update(cx, |gallery, cx| {
        gallery.set_scan_simulation_suspended(true, cx);
    });

    for (region, story) in StoryId::ALL.iter().enumerate() {
        if started_at.elapsed() >= SCAN_TIMEOUT {
            report.timed_out = true;
            break;
        }
        // Jump to the region with the proven direct-offset seam, then gather
        // steady samples with small incremental scrolls inside it.
        gallery.update(cx, |gallery, cx| {
            gallery.scroll_catalog_to(*story, cx);
        });
        // Stories near the catalog end can never own scroll top (the list
        // clamps when remaining content is shorter than the viewport), so
        // frames they appear in are attributed to the *requested* story.
        let focus_matches_request = current_focus(&gallery, cx) == region;
        settling_remaining = BOUNDARY_SETTLING_DRAWS;
        let mut region_steps = 0usize;
        while report.regions[region].samples.len() < MIN_STEADY_DRAWS_PER_STORY
            && region_steps < REGION_STEP_CAP
            && started_at.elapsed() < SCAN_TIMEOUT
        {
            cx.background_executor().timer(SCROLL_STEP).await;
            let _moved = push_step(&gallery, cx);
            let focus_region = if focus_matches_request {
                region
            } else {
                current_focus(&gallery, cx)
            };

            for event in collector.collect_unseen() {
                let FrameEvent::Draw(timing) = event else {
                    continue;
                };
                attribute_frame(
                    &mut report,
                    focus_region,
                    &mut attributed_region,
                    &mut settling_remaining,
                    &timing,
                );
                if let Some(latency) = timing.dirty_to_draw_duration() {
                    report.scan_latencies.push(duration_nanos(latency));
                }
            }
            region_steps += 1;

            if let Some(previous) = last_frame_seen_at {
                let frameless = previous.elapsed();
                if frameless.as_nanos() as u64 > FRAMELESS_STALL_NANOS {
                    report.longest_scroll_gap_nanos = report
                        .longest_scroll_gap_nanos
                        .max(frameless.as_nanos() as u64);
                }
            }
            last_frame_seen_at = Some(Instant::now());

            if let Some((new_peak, current)) = process_memory() {
                let peak = peak_memory.map_or(new_peak, |(recorded, _)| recorded.max(new_peak));
                peak_memory = Some((peak, current));
            }
        }
    }
    if started_at.elapsed() >= SCAN_TIMEOUT {
        report.timed_out = true;
    }
    if let Some((peak, _)) = peak_memory {
        report.peak_working_set_bytes = Some(peak);
    }

    // Idle probe: park on a static story with the simulation still suspended,
    // so any scheduled frames are genuine idle-demand violations rather than
    // simulated animation doing its job. Loading is deliberately excluded —
    // its shimmer animation legitimately requests frames.
    //
    // Measured behavior (2026-08-21 diagnostics): every story is idle-clean in
    // isolation (~1 draw / 3s). In catalog mode, parks directly after bulk
    // materialization show decaying invalidation bursts from the async
    // Markdown pipeline draining its parse queue; later parks measure zero.
    gallery.update(cx, |gallery, cx| {
        gallery.scroll_catalog_to(StoryId::Approval, cx);
    });
    let park_draws = count_park_draws(&mut collector, cx).await;
    report.idle_draws_while_parked = park_draws;
    gallery.update(cx, |gallery, cx| {
        gallery.set_scan_simulation_suspended(false, cx);
    });
    report.settled_working_set_bytes = process_memory().map(|(_, current)| current);
    report.reached_final_story = report
        .regions
        .last()
        .is_some_and(catalog_performance::RegionMetrics::is_visited);
    report.timed_out = started_at.elapsed() >= SCAN_TIMEOUT;

    set_trace_enabled(false);
    report.print();
    let failures = report.gate_failures();
    for failure in &failures {
        eprintln!("  gate failed: {failure}");
    }
    if failures.is_empty() {
        eprintln!("  gate passed");
    } else {
        exit_code.store(FAILURE_EXIT_CODE, Ordering::Relaxed);
    }
    cx.update(|cx| cx.quit());
}

/// Issues one scripted scroll step through the gallery's scan seam.
///
/// Returns whether the catalog view actually moved.
fn push_step(gallery: &Entity<Gallery>, cx: &mut AsyncApp) -> bool {
    gallery.update(cx, |gallery, cx| {
        gallery.advance_catalog_scan(gpui::px(STEP_DISTANCE_PX), cx)
    })
}

/// Attributes one drawn frame to its story region, discarding boundary-settling
/// draws so one-time materialization never lands in steady-region statistics.
fn attribute_frame(
    report: &mut CatalogScanReport,
    focus_region: usize,
    attributed_region: &mut usize,
    settling_remaining: &mut usize,
    timing: &gpui::profiler::FrameTiming,
) {
    if focus_region != *attributed_region {
        *attributed_region = focus_region;
        *settling_remaining = BOUNDARY_SETTLING_DRAWS;
    }
    if *settling_remaining > 0 {
        *settling_remaining -= 1;
    } else if let Some(metrics) = report.regions.get_mut(*attributed_region) {
        metrics.push(duration_nanos(timing.draw_duration()));
    }
}

fn duration_nanos(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

/// Reads the story region the catalog currently focuses.
fn current_focus(gallery: &Entity<Gallery>, cx: &AsyncApp) -> usize {
    gallery.read_with(cx, |gallery: &Gallery, _| gallery.scan_focus_region())
}

/// Milliseconds the idle probe waits after parking before counting frames,
/// letting post-materialization parse bursts drain.
const IDLE_SETTLE_MS: u64 = 5_000;

/// Counts drawn frames over a three-second park for idle-demand diagnosis.
///
/// A settle delay runs first and its draws are discarded: parking directly
/// after bulk story materialization catches the known decaying invalidation
/// burst from the async Markdown pipeline draining its parse queue (measured
/// 2026-08-21, reconfirmed after the 2026-08-22 upstream bump). Steady-state
/// demand is what this probe exists to catch.
async fn count_park_draws(collector: &mut FrameTimingCollector, cx: &mut AsyncApp) -> u64 {
    let settle_until = Instant::now() + Duration::from_millis(IDLE_SETTLE_MS);
    while Instant::now() < settle_until {
        cx.background_executor()
            .timer(Duration::from_millis(IDLE_PROBE_MS))
            .await;
        let _ = collector.collect_unseen();
    }

    let mut draws = 0u64;
    let probe_started = Instant::now();
    while probe_started.elapsed() < Duration::from_secs(3) {
        cx.background_executor()
            .timer(Duration::from_millis(IDLE_PROBE_MS))
            .await;
        for event in collector.collect_unseen() {
            if matches!(event, FrameEvent::Draw(_)) {
                draws += 1;
            }
        }
    }
    draws
}
