//! The motion lab: an instrument for the shared motion primitives.
//!
//! Every 0.3.0 motion decision rides on a handful of primitives — the token
//! roles, `gpui_base::motion::{transition, spring}`, and the arrival stagger
//! — and each has failure modes a passing screenshot cannot catch: a value
//! that jumps when its target reverses mid-flight, a channel that keeps
//! requesting frames after it settles, a cascade that queues delays behind a
//! hundred arrivals. This story drives exactly those cases, on demand and
//! under test, with live readouts, so a primitive that misbehaves is seen
//! here rather than tuned around in a component.
//!
//! Four probes, one per failure family:
//!
//! - **Disclosure** — a panel on the `standard`-role transition, with a
//!   burst driver that toggles it ten times at 70 ms intervals. The value
//!   must retarget from its current sample every time; reaching an endpoint
//!   between toggles would read as a flash.
//! - **Indicator** — a selection marker gliding across differently sized
//!   targets on the `selection` spring, position and width as independent
//!   channels.
//! - **Reorder** — five uniform rows swapping slots on the `reflow` spring,
//!   with a driver that reverses the order again halfway through the
//!   response, which is the case that must decelerate and turn rather than
//!   jump.
//! - **Arrival** — six chips revealed through the decelerating arrival
//!   stagger, re-run against a fresh generation each press.
//!
//! The environment row flips reduced motion live and steps the rem size,
//! because both must be safe to change while everything above is moving.
//! The scrub row freezes the choreography at fixed sample points — a linear
//! endpoint scrub of the same geometry, for parking a pose under visual
//! review; while frozen, no sampler runs and no frames are requested.
//!
//! Readouts under the probes show each channel's target, sampled value,
//! per-frame delta, and whether it is running, plus the active-channel count
//! and the frames drawn while anything moved. The kill rule from the plan
//! applies to what this story reveals: a primitive that jumps on reversal,
//! schedules after settling, or keys by render order gets redesigned, not
//! shipped around.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    Context, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Render,
    StatefulInteractiveElement as _, Styled as _, Task, Window, div, px,
};
use gpui_ai::motion::{MotionTokens, reveal_progress};
use gpui_base::animation::ease_out_cubic;
use gpui_base::motion::{Spring, Transition, spring, transition};
use gpui_component::button::Button;
use gpui_component::{ActiveTheme as _, Selectable as _, h_flex, v_flex};
use std::time::Duration;

/// Toggles in one disclosure burst — the plan's "ten times".
const BURST_TOGGLES: usize = 10;

/// Interval between burst toggles, inside the plan's 50–100 ms window.
const BURST_INTERVAL: Duration = Duration::from_millis(70);

/// The disclosure panel's fully open height.
const DISCLOSURE_HEIGHT: f32 = 96.0;

/// Widths of the indicator's targets — deliberately unequal, so a retarget
/// moves both channels by different amounts.
const INDICATOR_TARGETS: [f32; 4] = [56.0, 112.0, 84.0, 148.0];

/// Gap between indicator targets.
const INDICATOR_GAP: f32 = 8.0;

/// Rows in the reorder probe. Uniform height on purpose: slot geometry is
/// model-derivable at render time, which is the structural condition reflow
/// motion is allowed under.
const ROW_COUNT: usize = 5;

/// One reorder row's height plus its gap.
const ROW_PITCH: f32 = 28.0;

/// Chips in one arrival cascade — the stagger participation bound.
const ARRIVAL_CHIPS: usize = 6;

/// The latest sample of one driven channel, for the readout strip.
#[derive(Clone, Copy, Default)]
struct ChannelSample {
    target: f32,
    value: f32,
    /// Change since the previous render — a per-frame delta, not a true
    /// velocity: the instrument reads what the sampler returned, and the
    /// sampler does not expose its internal state.
    delta: f32,
    running: bool,
}

impl ChannelSample {
    /// Records this render's sample against the previous one.
    fn observe(&mut self, target: f32, value: f32) {
        self.delta = value - self.value;
        self.target = target;
        self.value = value;
        self.running = (value - target).abs() > f32::EPSILON;
    }
}

/// The lab's state: what each probe is asked to show, plus the readouts the
/// most recent render captured.
pub(crate) struct MotionLabStory {
    disclosure_open: bool,
    /// Toggles the running burst still owes. Display only; the driver task
    /// owns the schedule.
    burst_remaining: usize,
    indicator: usize,
    reversed: bool,
    /// Bumped per arrival press: chips key their reveals by generation, so
    /// each press is a fresh cascade rather than a replay of settled state.
    /// Generation zero — the mount — renders at rest, because arrival motion
    /// belongs to declared-fresh items, never to initial load.
    arrival_generation: usize,
    /// `None` is live; `Some(t)` freezes every probe at the linear endpoint
    /// scrub position `t`, running no sampler and requesting no frames.
    scrub: Option<f32>,
    disclosure_sample: ChannelSample,
    indicator_x_sample: ChannelSample,
    indicator_width_sample: ChannelSample,
    /// Reorder rows still moving, as of the most recent render.
    rows_running: usize,
    /// Row 0's sampled top — the position the reversal tests watch.
    first_row_top: f32,
    /// Renders drawn while any channel was running.
    frames_while_active: usize,
    /// The one driver slot: starting a burst or a reversal replaces the
    /// previous task, so exactly one script can drive the lab at a time.
    driver: Option<Task<()>>,
}

impl MotionLabStory {
    pub(crate) fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self {
            disclosure_open: false,
            burst_remaining: 0,
            indicator: 0,
            reversed: false,
            arrival_generation: 0,
            scrub: None,
            disclosure_sample: ChannelSample::default(),
            indicator_x_sample: ChannelSample::default(),
            indicator_width_sample: ChannelSample::default(),
            rows_running: 0,
            first_row_top: 0.0,
            frames_while_active: 0,
            driver: None,
        }
    }

    /// Toggles the disclosure once, by hand.
    fn toggle_disclosure(&mut self, cx: &mut Context<Self>) {
        self.disclosure_open = !self.disclosure_open;
        cx.notify();
    }

    /// Runs the plan's interruption case: ten toggles at burst cadence.
    fn start_disclosure_burst(&mut self, cx: &mut Context<Self>) {
        self.burst_remaining = BURST_TOGGLES;
        self.driver = Some(cx.spawn(async move |this, cx| {
            for _ in 0..BURST_TOGGLES {
                cx.background_executor().timer(BURST_INTERVAL).await;
                let alive = this.update(cx, |lab, cx| {
                    lab.disclosure_open = !lab.disclosure_open;
                    lab.burst_remaining = lab.burst_remaining.saturating_sub(1);
                    cx.notify();
                });
                if alive.is_err() {
                    return;
                }
            }
        }));
        cx.notify();
    }

    /// Moves the selection indicator to the next, differently sized target.
    fn advance_indicator(&mut self, cx: &mut Context<Self>) {
        self.indicator = (self.indicator + 1) % INDICATOR_TARGETS.len();
        cx.notify();
    }

    /// Sends the indicator to one target directly.
    fn select_indicator(&mut self, index: usize, cx: &mut Context<Self>) {
        self.indicator = index;
        cx.notify();
    }

    /// Reverses the row order once.
    fn reverse_rows(&mut self, cx: &mut Context<Self>) {
        self.reversed = !self.reversed;
        cx.notify();
    }

    /// Reverses the rows, then reverses them again halfway through the
    /// reflow response — the retarget that must turn, not jump.
    fn reverse_rows_halfway(&mut self, cx: &mut Context<Self>) {
        self.reversed = !self.reversed;
        let halfway = MotionTokens::read(cx).reflow_spring().response() / 2;
        self.driver = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(halfway).await;
            this.update(cx, |lab, cx| {
                lab.reversed = !lab.reversed;
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    /// Starts a fresh arrival cascade.
    fn arrive(&mut self, cx: &mut Context<Self>) {
        self.arrival_generation += 1;
        cx.notify();
    }

    /// The slot each row occupies under the current order.
    fn slot_of(&self, row: usize) -> usize {
        if self.reversed {
            ROW_COUNT - 1 - row
        } else {
            row
        }
    }

    /// Left edge of the indicator target at `index`.
    fn target_left(index: usize) -> f32 {
        INDICATOR_TARGETS[..index]
            .iter()
            .map(|width| width + INDICATOR_GAP)
            .sum()
    }
}

impl Render for MotionLabStory {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme_tokens = cx.theme().semantic_tokens();
        let motion = MotionTokens::read(cx).clone();
        let reduced = cx.reduce_motion();
        let scrub = self.scrub;

        // -- Sample every live channel first; the readout strip below shows
        // exactly what this render used. A frozen lab samples nothing.
        let disclosure_target = if self.disclosure_open { 1.0 } else { 0.0 };
        let disclosure = match scrub {
            Some(t) => t,
            None => {
                let value = transition(
                    "lab-disclosure",
                    disclosure_target,
                    Transition::new(motion.standard()).ease(ease_out_cubic),
                    window,
                    cx,
                );
                self.disclosure_sample.observe(disclosure_target, value);
                value
            }
        };

        let selection = motion.selection_spring();
        let selection_policy = Spring::new(selection.response()).with_damping(selection.damping());
        let indicator_left_target = Self::target_left(self.indicator);
        let indicator_width_target = INDICATOR_TARGETS[self.indicator];
        let (indicator_left, indicator_width) = match scrub {
            Some(t) => (
                indicator_left_target * t,
                INDICATOR_TARGETS[0] + (indicator_width_target - INDICATOR_TARGETS[0]) * t,
            ),
            None => {
                let left: Pixels = spring(
                    ("lab-indicator", "x"),
                    px(indicator_left_target),
                    selection_policy,
                    window,
                    cx,
                );
                let width: Pixels = spring(
                    ("lab-indicator", "width"),
                    px(indicator_width_target),
                    selection_policy,
                    window,
                    cx,
                );
                self.indicator_x_sample
                    .observe(indicator_left_target, left.as_f32());
                self.indicator_width_sample
                    .observe(indicator_width_target, width.as_f32());
                (left.as_f32(), width.as_f32())
            }
        };

        let reflow = motion.reflow_spring();
        let reflow_policy = Spring::new(reflow.response()).with_damping(reflow.damping());
        let mut rows_running = 0;
        let row_tops: Vec<f32> = (0..ROW_COUNT)
            .map(|row| {
                let slot_top = self.slot_of(row) as f32 * ROW_PITCH;
                match scrub {
                    Some(t) => {
                        let origin = row as f32 * ROW_PITCH;
                        origin + (slot_top - origin) * t
                    }
                    None => {
                        let top: Pixels = spring(
                            ElementId::NamedInteger("lab-row".into(), row as u64),
                            px(slot_top),
                            reflow_policy,
                            window,
                            cx,
                        );
                        if (top.as_f32() - slot_top).abs() > f32::EPSILON {
                            rows_running += 1;
                        }
                        if row == 0 {
                            self.first_row_top = top.as_f32();
                        }
                        top.as_f32()
                    }
                }
            })
            .collect();
        if scrub.is_none() {
            self.rows_running = rows_running;
        }

        let active_channels = usize::from(self.disclosure_sample.running)
            + usize::from(self.indicator_x_sample.running)
            + usize::from(self.indicator_width_sample.running)
            + self.rows_running;
        if scrub.is_none() && active_channels > 0 {
            self.frames_while_active += 1;
        }

        // -- Environment controls: the switches that must be safe mid-flight.
        let environment = h_flex()
            .gap(theme_tokens.spacing.sm)
            .child(
                Button::new("lab-reduce-motion")
                    .outline()
                    .selected(reduced)
                    .label(if reduced {
                        "Reduced motion: on"
                    } else {
                        "Reduced motion: off"
                    })
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.set_reduce_motion(!reduced);
                        cx.notify();
                    })),
            )
            .child({
                // The preference the policy is resolving to right now,
                // OS signal composed — cycling it exercises the same
                // switch an application would ship.
                use gpui_ai::motion::{MotionPreference, MotionTokens};
                let preference = MotionTokens::read(cx).preference();
                let effective = MotionTokens::effective_preference(cx);
                let next = match preference {
                    MotionPreference::Full => MotionPreference::Crossfade,
                    MotionPreference::Crossfade => MotionPreference::Snap,
                    MotionPreference::Snap => MotionPreference::Full,
                };
                Button::new("lab-motion-preference")
                    .outline()
                    .label(format!(
                        "Preference: {preference:?} (effective {effective:?})"
                    ))
                    .on_click(cx.listener(move |_, _, _, cx| {
                        let tokens = MotionTokens::read(cx).clone();
                        tokens.with_preference(next).set(cx);
                        cx.notify();
                    }))
            })
            .child(
                Button::new("lab-rem-down")
                    .outline()
                    .label("rem −")
                    .on_click(cx.listener(|_, _, window, cx| {
                        let rem = window.rem_size();
                        window.set_rem_size(rem - px(1.));
                        cx.notify();
                    })),
            )
            .child(
                Button::new("lab-rem-up")
                    .outline()
                    .label("rem +")
                    .on_click(cx.listener(|_, _, window, cx| {
                        let rem = window.rem_size();
                        window.set_rem_size(rem + px(1.));
                        cx.notify();
                    })),
            );

        // -- Probe: disclosure.
        let disclosure_probe = v_flex()
            .gap(theme_tokens.spacing.xs)
            .child(
                h_flex()
                    .gap(theme_tokens.spacing.sm)
                    .child(
                        Button::new("lab-disclosure-toggle")
                            .outline()
                            .label(if self.disclosure_open {
                                "Close"
                            } else {
                                "Open"
                            })
                            .on_click(cx.listener(|lab, _, _, cx| lab.toggle_disclosure(cx))),
                    )
                    .child(
                        Button::new("lab-disclosure-burst")
                            .outline()
                            .label(if self.burst_remaining > 0 {
                                "Bursting…"
                            } else {
                                "Toggle ×10 @ 70 ms"
                            })
                            .on_click(cx.listener(|lab, _, _, cx| lab.start_disclosure_burst(cx))),
                    ),
            )
            .child(
                div()
                    .w(px(280.))
                    .h(px(DISCLOSURE_HEIGHT * disclosure))
                    .overflow_hidden()
                    .rounded(theme_tokens.radius.md)
                    .bg(cx.theme().muted)
                    .opacity(0.4 + 0.6 * disclosure)
                    .child(
                        div()
                            .p(theme_tokens.spacing.sm)
                            .text_color(cx.theme().muted_foreground)
                            .child("Disclosed content stays laid out; the panel clips it."),
                    ),
            );

        // -- Probe: selection indicator across unequal targets.
        let indicator_probe = v_flex()
            .gap(theme_tokens.spacing.xs)
            .child(
                Button::new("lab-indicator-cycle")
                    .outline()
                    .label("Next target")
                    .on_click(cx.listener(|lab, _, _, cx| lab.advance_indicator(cx))),
            )
            .child(
                div()
                    .relative()
                    .h(px(34.))
                    .w(px(Self::target_left(INDICATOR_TARGETS.len() - 1)
                        + INDICATOR_TARGETS[INDICATOR_TARGETS.len() - 1]))
                    .child(
                        div()
                            .absolute()
                            .top(px(0.))
                            .left(px(indicator_left))
                            .w(px(indicator_width))
                            .h(px(28.))
                            .rounded(theme_tokens.radius.md)
                            .bg(cx.theme().primary.opacity(0.25)),
                    )
                    .children(INDICATOR_TARGETS.iter().enumerate().map(|(index, width)| {
                        div()
                            .id(ElementId::NamedInteger("lab-target".into(), index as u64))
                            .absolute()
                            .top(px(0.))
                            .left(px(Self::target_left(index)))
                            .w(px(*width))
                            .h(px(28.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(theme_tokens.radius.md)
                            .border_1()
                            .border_color(cx.theme().border)
                            .text_color(cx.theme().muted_foreground)
                            .on_click(
                                cx.listener(move |lab, _, _, cx| lab.select_indicator(index, cx)),
                            )
                            .child(format!("{index}"))
                    })),
            );

        // -- Probe: row reorder with mid-flight reversal.
        let reorder_probe = v_flex()
            .gap(theme_tokens.spacing.xs)
            .child(
                h_flex()
                    .gap(theme_tokens.spacing.sm)
                    .child(
                        Button::new("lab-reverse")
                            .outline()
                            .label("Reverse")
                            .on_click(cx.listener(|lab, _, _, cx| lab.reverse_rows(cx))),
                    )
                    .child(
                        Button::new("lab-reverse-halfway")
                            .outline()
                            .label("Reverse, then back at ½ response")
                            .on_click(cx.listener(|lab, _, _, cx| lab.reverse_rows_halfway(cx))),
                    ),
            )
            .child(
                div()
                    .relative()
                    .w(px(220.))
                    .h(px(ROW_COUNT as f32 * ROW_PITCH))
                    .children(row_tops.iter().enumerate().map(|(row, top)| {
                        div()
                            .absolute()
                            .left(px(0.))
                            .top(px(*top))
                            .w(px(220.))
                            .h(px(ROW_PITCH - 4.0))
                            .px(theme_tokens.spacing.sm)
                            .flex()
                            .items_center()
                            .rounded(theme_tokens.radius.sm)
                            .bg(cx.theme().muted)
                            .text_color(cx.theme().foreground)
                            .child(format!("Row {row}"))
                    })),
            );

        // -- Probe: decelerating arrival cascade.
        let generation = self.arrival_generation;
        let arrival_probe = v_flex()
            .gap(theme_tokens.spacing.xs)
            .child(
                Button::new("lab-arrive")
                    .outline()
                    .label("Arrive ×6, decelerating")
                    .on_click(cx.listener(|lab, _, _, cx| lab.arrive(cx))),
            )
            .child(
                h_flex()
                    .gap(theme_tokens.spacing.xs)
                    .children((0..ARRIVAL_CHIPS).map(|index| {
                        let progress = match scrub {
                            Some(t) => t,
                            // The mount is not an arrival: only a press
                            // declares fresh items, so generation zero sits
                            // at rest instead of cascading over an untouched
                            // lab.
                            None if generation == 0 => 1.0,
                            None => reveal_progress(
                                ("lab-arrival", (generation * ARRIVAL_CHIPS + index) as u64),
                                motion.arrival_stagger(index, ARRIVAL_CHIPS),
                                window,
                                cx,
                            ),
                        };
                        div()
                            .px(theme_tokens.spacing.sm)
                            .py(theme_tokens.spacing.xxs)
                            .rounded(theme_tokens.radius.full)
                            .bg(cx.theme().secondary)
                            .text_color(cx.theme().secondary_foreground)
                            .opacity(progress)
                            .top(px(6.0 * (1.0 - progress)))
                            .child(format!("chip {index}"))
                    })),
            );

        // -- Readouts: what this render sampled.
        let readout_row = |name: &'static str, sample: ChannelSample| {
            div()
                .font_family(cx.theme().mono_font_family.clone())
                .text_color(if sample.running {
                    cx.theme().foreground
                } else {
                    cx.theme().muted_foreground
                })
                .child(format!(
                    "{name:<11} target {:>7.2}  value {:>7.2}  Δ/frame {:>+7.3}  {}",
                    sample.target,
                    sample.value,
                    sample.delta,
                    if sample.running { "running" } else { "settled" },
                ))
        };
        let readouts = v_flex()
            .gap(theme_tokens.spacing.xxs)
            .child(readout_row("disclosure", self.disclosure_sample))
            .child(readout_row("indicator x", self.indicator_x_sample))
            .child(readout_row("indicator w", self.indicator_width_sample))
            .child(
                div()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "rows moving {}  active channels {}  frames while active {}{}",
                        self.rows_running,
                        active_channels,
                        self.frames_while_active,
                        if scrub.is_some() { "  [frozen]" } else { "" },
                    )),
            );

        // -- Scrub: park the choreography at a reviewable pose.
        let scrub_row = h_flex()
            .gap(theme_tokens.spacing.xs)
            .child(
                Button::new("lab-scrub-live")
                    .outline()
                    .selected(scrub.is_none())
                    .label("Live")
                    .on_click(cx.listener(|lab, _, _, cx| {
                        lab.scrub = None;
                        cx.notify();
                    })),
            )
            .children([0.0f32, 0.25, 0.5, 0.75, 1.0].into_iter().map(|t| {
                Button::new(("lab-scrub", (t * 100.0) as u64))
                    .outline()
                    .selected(scrub == Some(t))
                    .label(format!("{:.0}%", t * 100.0))
                    .on_click(cx.listener(move |lab, _, _, cx| {
                        lab.scrub = Some(t);
                        cx.notify();
                    }))
            }));

        v_flex()
            .id("motion-lab")
            .gap(theme_tokens.spacing.md)
            .p(theme_tokens.spacing.md)
            .child(environment)
            .child(disclosure_probe)
            .child(indicator_probe)
            .child(reorder_probe)
            .child(arrival_probe)
            .child(readouts)
            .child(scrub_row)
            .when(reduced, |lab| {
                lab.child(
                    div()
                        .text_color(cx.theme().muted_foreground)
                        .child("Reduced motion: every channel snaps to its target."),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Entity, TestAppContext, VisualTestContext};

    fn open(cx: &mut TestAppContext) -> (Entity<MotionLabStory>, &mut VisualTestContext) {
        cx.update(crate::init);
        let (lab, cx) = cx.add_window_view(MotionLabStory::new);
        cx.update(|window, cx| window.draw(cx).clear(cx));
        (lab, cx)
    }

    fn draw(cx: &mut VisualTestContext) {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    /// Asserts the window's frame demand dies out.
    ///
    /// Driving the lab with `draw` leaves the animation-frame callbacks each
    /// running sampler requested queued; firing them redraws, and a settled
    /// sampler requests nothing further, so the backlog must drain to zero
    /// within a few frames. A channel still animating re-requests every
    /// fire and never reaches zero, which is exactly the failure this
    /// reports.
    fn assert_goes_quiet(cx: &mut VisualTestContext, what: &str) {
        for _ in 0..8 {
            let callbacks = cx.update(|window, cx| window.simulate_next_frame(cx));
            cx.run_until_parked();
            if callbacks == 0 {
                return;
            }
        }
        panic!("{what}: frame demand never died out");
    }

    fn disclosure_value(lab: &Entity<MotionLabStory>, cx: &mut VisualTestContext) -> f32 {
        lab.read_with(cx, |lab, _| lab.disclosure_sample.value)
    }

    #[gpui::test]
    fn ten_rapid_toggles_keep_the_disclosure_continuous(cx: &mut TestAppContext) {
        let (lab, cx) = open(cx);
        lab.update(cx, |lab, cx| lab.start_disclosure_burst(cx));
        cx.run_until_parked();

        let mut previous = disclosure_value(&lab, cx);
        for _ in 0..BURST_TOGGLES {
            cx.executor().advance_clock(BURST_INTERVAL);
            cx.run_until_parked();
            draw(cx);
            let sampled = disclosure_value(&lab, cx);
            // Continuity is the contract: between two toggles the transition
            // retargets from its current sample, so no step may teleport the
            // value across the whole range.
            assert!(
                (sampled - previous).abs() < 0.7,
                "the disclosure jumped from {previous} to {sampled} mid-burst"
            );
            assert!((0.0..=1.0).contains(&sampled), "sampled {sampled}");
            previous = sampled;
        }

        // Let the last retarget play out; a settled lab must go quiet.
        cx.executor()
            .advance_clock(MotionTokens::DEFAULT.standard() * 2);
        draw(cx);
        assert_goes_quiet(cx, "a settled disclosure");
    }

    #[gpui::test]
    fn a_mid_flight_reversal_turns_instead_of_jumping(cx: &mut TestAppContext) {
        let (lab, cx) = open(cx);
        let response = MotionTokens::DEFAULT.reflow_spring().response();

        lab.update(cx, |lab, cx| lab.reverse_rows(cx));
        cx.run_until_parked();
        cx.executor().advance_clock(response / 2);
        draw(cx);
        let mid = lab.read_with(cx, |lab, _| lab.rows_running);
        assert!(mid > 0, "rows should still be travelling at half response");

        // Reverse again mid-flight and step in small increments: the sampled
        // top of the first row must walk back continuously — a velocity-
        // preserving spring decelerates and turns, so no 16 ms step may leap
        // across the track the way a restarted curve or a snap would.
        lab.update(cx, |lab, cx| lab.reverse_rows(cx));
        cx.run_until_parked();
        let mut previous = lab.read_with(cx, |lab, _| lab.first_row_top);
        for _ in 0..24 {
            cx.executor().advance_clock(Duration::from_millis(16));
            draw(cx);
            let top = lab.read_with(cx, |lab, _| lab.first_row_top);
            let step = (top - previous).abs();
            assert!(
                step < ROW_PITCH * 2.0,
                "row 0 leapt {step}px in one 16 ms step (from {previous} to {top})"
            );
            previous = top;
        }

        // And it settles: rows stop moving and the window goes quiet.
        cx.executor().advance_clock(response * 4);
        draw(cx);
        assert_eq!(lab.read_with(cx, |lab, _| lab.rows_running), 0);
        assert_goes_quiet(cx, "settled rows");
    }

    #[gpui::test]
    fn reduced_motion_mid_flight_snaps_to_the_target(cx: &mut TestAppContext) {
        let (lab, cx) = open(cx);
        lab.update(cx, |lab, cx| lab.toggle_disclosure(cx));
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(40));
        draw(cx);
        let mid = disclosure_value(&lab, cx);
        assert!(mid < 1.0, "the disclosure should still be travelling");

        cx.update(|_, cx| cx.set_reduce_motion(true));
        draw(cx);
        assert_eq!(
            disclosure_value(&lab, cx),
            1.0,
            "reduced motion resolves the channel at its target"
        );
        assert_goes_quiet(cx, "a reduced-motion lab");
    }

    #[gpui::test]
    fn a_frozen_lab_samples_nothing_and_schedules_nothing(cx: &mut TestAppContext) {
        let (lab, cx) = open(cx);
        lab.update(cx, |lab, cx| {
            lab.disclosure_open = true;
            lab.scrub = Some(0.5);
            cx.notify();
        });
        cx.run_until_parked();
        draw(cx);
        assert_goes_quiet(cx, "a scrubbed pose");
    }

    #[gpui::test]
    fn a_settled_lab_draws_no_idle_frames_at_rest(cx: &mut TestAppContext) {
        let (_, cx) = open(cx);
        draw(cx);
        assert_goes_quiet(cx, "an untouched lab");
    }
}
