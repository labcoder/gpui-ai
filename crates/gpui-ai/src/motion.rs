//! Motion primitives shared by gpui-ai components.
//!
//! Every effect here is built on GPUI's animation system, so reduced-motion
//! mode resolves to a useful static frame automatically: one-shot reveals
//! settle at their end state and repeating effects (shimmer, breathing)
//! render at rest. Components opt in explicitly — nothing in this module
//! installs idle redraw on its own.
//!
//! # Motion roles
//!
//! No component spells out a duration. Every animated timing in the crate
//! resolves through one of three internal roles, each a const-constructible
//! spec whose members are named associated constants:
//!
//! - enter / reveal — `EnterSpec`: `REVEAL`.
//! - progress loop — `ProgressLoopSpec`: `SHIMMER`, `GRID_SWEEP`,
//!   `IMAGE_PULSE`, `STATUS_SPINNER`.
//! - ambient loop — `AmbientLoopSpec`: `BREATHING`, `ORB_LATTICE`.
//!
//! A progress loop is bound to work that ends: the element carrying it is
//! gone once the work finishes. An ambient loop has no completion to report
//! and runs for as long as its element is on screen. The fourth role the
//! roadmap names, immediate feedback, has no member: nothing in the crate
//! delays or eases a direct response to input, and inventing a timing to
//! fill the table would be a new visual decision rather than a relocation
//! of an existing one.
//!
//! The whole seam is `pub(crate)`. Whether any of it becomes consumer-facing
//! — theme JSON, a public token type — is a later decision, and nothing here
//! commits to one.
//!
//! # Example
//!
//! ```ignore
//! use gpui_ai::motion::{Shimmer, reveal};
//!
//! // A "Thinking…" label with a travelling highlight while work runs.
//! Shimmer::new("thinking-label", "Thinking…").active(is_running)
//!
//! // A freshly inserted row fades and rises into place once.
//! reveal(v_flex().child("New tool call"), ("tool-call", 3), window, cx)
//! ```

use gpui::{
    Animation, AnimationElement, AnimationExt as _, App, ElementId, Hsla, IntoElement,
    ParentElement as _, RenderOnce, SharedString, StyleRefinement, Styled, Window, div,
    ease_in_out, pulsating_between, px, relative,
};
use gpui_base::animation::ease_out_cubic;
use gpui_component::{ActiveTheme as _, StyledExt as _};
use std::time::Duration;

/// Width of the travelling shimmer highlight as a fraction of the label
/// width. Highlight geometry rather than tempo, so it stays out of the role
/// specs below.
const SHIMMER_BAND: f32 = 0.45;

/// A one-shot entrance: a newly mounted element settling into place.
///
/// One spec covers every entrance in the crate deliberately. Rows, chips,
/// tool calls, and attachments arrive at one tempo; a second entrance tempo
/// would be a design decision, not a configuration.
#[derive(Debug, Clone, Copy)]
pub(crate) struct EnterSpec {
    /// Mount to settled.
    pub(crate) duration: Duration,
    /// Added per sibling index, so a list ripples rather than snapping.
    pub(crate) stagger: Duration,
    /// Index past which the stagger stops growing, bounding how long the
    /// last item of a long list waits.
    pub(crate) stagger_cap: usize,
    /// Travel from rest, in pixels. An animation distance, not layout.
    pub(crate) rise: f32,
}

impl EnterSpec {
    /// The crate's entrance tempo.
    pub(crate) const REVEAL: Self = Self {
        duration: Duration::from_millis(260),
        stagger: Duration::from_millis(40),
        stagger_cap: 12,
        rise: 6.0,
    };
}

/// A repeating loop that means "this work is in flight". It exists only
/// while the work does: the caller stops rendering the element that carries
/// it once the work finishes.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ProgressLoopSpec {
    /// One full pass, including any rest beat.
    pub(crate) period: Duration,
    /// Fraction of `period` spent moving. Below `1.0` the remainder is a
    /// rest beat, so consecutive passes read as deliberate rather than
    /// frantic.
    pub(crate) duty: f32,
}

impl ProgressLoopSpec {
    /// Label shimmer — the ecosystem's "something is happening" text
    /// treatment.
    pub(crate) const SHIMMER: Self = Self {
        period: Duration::from_millis(1800),
        duty: 0.72,
    };

    /// Diagonal sweep across the pixel-grid loader.
    pub(crate) const GRID_SWEEP: Self = Self {
        period: Duration::from_millis(1400),
        duty: 1.0,
    };

    /// Placeholder pulse shown until generated pixels arrive.
    pub(crate) const IMAGE_PULSE: Self = Self {
        period: Duration::from_millis(1600),
        duty: 1.0,
    };

    /// One rotation of an in-flight status icon.
    pub(crate) const STATUS_SPINNER: Self = Self {
        period: Duration::from_millis(900),
        duty: 1.0,
    };

    /// This loop as a repeating GPUI animation.
    pub(crate) fn looping(self) -> Animation {
        repeating(self.period)
    }
}

/// A repeating loop with nothing to complete. It runs for as long as its
/// element is on screen, which is the state it reports.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AmbientLoopSpec {
    /// One full cycle. Choreographed phase offsets are fractions of this.
    pub(crate) period: Duration,
}

impl AmbientLoopSpec {
    /// Opacity breathing for "still working" indicators.
    pub(crate) const BREATHING: Self = Self {
        period: Duration::from_millis(1600),
    };

    /// One cycle of the orb lattice.
    pub(crate) const ORB_LATTICE: Self = Self {
        period: Duration::from_millis(1700),
    };

    /// The cycle in whole milliseconds, for choreography that phases in
    /// integer beats.
    pub(crate) const fn period_millis(self) -> u64 {
        self.period.as_millis() as u64
    }

    /// This loop as a repeating GPUI animation.
    pub(crate) fn looping(self) -> Animation {
        repeating(self.period)
    }
}

/// A loop over `period`.
///
/// Every repeating effect in the crate is built here, so all of them inherit
/// GPUI's reduced-motion contract by construction: a repeating animation is
/// held at its first frame and schedules nothing. No call site re-implements
/// that check.
fn repeating(period: Duration) -> Animation {
    Animation::new(period).repeat()
}

/// Text with a soft highlight travelling across it — the ecosystem's
/// "something is happening" label treatment for thinking, running tool
/// groups, and streaming plans.
///
/// The base layer is muted text; a clipped bright copy sweeps over it. When
/// [`Shimmer::active`] is false (or reduced motion is on) the label renders
/// as plain muted text, so meaning never depends on the motion.
#[derive(IntoElement)]
pub struct Shimmer {
    id: ElementId,
    style: StyleRefinement,
    text: SharedString,
    active: bool,
    base: Option<Hsla>,
    highlight: Option<Hsla>,
}

impl Shimmer {
    /// Creates an active shimmer label.
    pub fn new(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            style: StyleRefinement::default(),
            text: text.into(),
            active: true,
            base: None,
            highlight: None,
        }
    }

    /// Sets whether the highlight travels. Inactive labels are plain text.
    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    /// Overrides the resting and highlight colors (defaults: muted foreground
    /// and foreground).
    pub fn colors(mut self, base: Hsla, highlight: Hsla) -> Self {
        self.base = Some(base);
        self.highlight = Some(highlight);
        self
    }
}

impl Styled for Shimmer {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Shimmer {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let base = self.base.unwrap_or(cx.theme().muted_foreground);
        let highlight = self.highlight.unwrap_or(cx.theme().foreground);
        if !self.active {
            return div()
                .whitespace_nowrap()
                .text_color(base)
                .child(self.text)
                .refine_style(&self.style)
                .into_any_element();
        }

        let text = self.text;
        div()
            .relative()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_color(base)
            .child(text.clone())
            .refine_style(&self.style)
            .with_animation(
                (self.id, "shimmer"),
                // Frame demand: active while the caller's work is in flight,
                // which is what `active` reports; the caller stops passing
                // `active(true)` when the work ends, so the loop never runs
                // over a settled surface. Reduced motion holds delta at 0 —
                // the band sits fully off the leading edge and the label is
                // plain muted text.
                ProgressLoopSpec::SHIMMER.looping(),
                move |container, delta| {
                    // Travel during the first part of the cycle, then rest
                    // fully off the trailing edge.
                    let progress = ease_in_out((delta / ProgressLoopSpec::SHIMMER.duty).min(1.0));
                    let start = -SHIMMER_BAND + progress * (1.0 + SHIMMER_BAND);
                    container.child(
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left(relative(start))
                            .w(relative(SHIMMER_BAND))
                            .overflow_hidden()
                            .child(
                                div()
                                    .absolute()
                                    .top_0()
                                    // Counter-offset so the bright copy stays
                                    // aligned with the base text underneath.
                                    .left(relative(-start / SHIMMER_BAND))
                                    .whitespace_nowrap()
                                    .text_color(highlight)
                                    .child(text.clone()),
                            ),
                    )
                },
            )
            .into_any_element()
    }
}

/// Progress of a one-shot reveal keyed by `id`, from `0.0` (just mounted) to
/// `1.0` (settled), starting the clock the first time the element renders.
///
/// The start instant lives in keyed element state, so a row that keeps its
/// stable identity across renders plays the reveal only once; it requests
/// animation frames until it settles and returns `1.0` immediately under
/// reduced motion.
pub fn reveal_progress(
    id: impl Into<ElementId>,
    delay: Duration,
    window: &mut Window,
    cx: &mut App,
) -> f32 {
    // Frame demand: the only hand-scheduled effect in the crate, because a
    // reveal reads a clock rather than a GPUI animation. Active while
    // `progress < 1.0`; a settled reveal asks for nothing, and reduced
    // motion asks for nothing and returns the end state. The audit below
    // counts the requests rather than asserting this in prose.
    if cx.reduce_motion() {
        return 1.0;
    }
    let now = cx.background_executor().now();
    let started = *window.use_keyed_state(id, cx, |_, _| now).read(cx);
    let elapsed = now.saturating_duration_since(started);
    let progress = if elapsed <= delay {
        0.0
    } else {
        (elapsed.saturating_sub(delay).as_secs_f32() / EnterSpec::REVEAL.duration.as_secs_f32())
            .min(1.0)
    };
    if progress < 1.0 {
        note_reveal_frame_request();
        window.request_animation_frame();
    }
    ease_out_cubic(progress)
}

#[cfg(test)]
thread_local! {
    /// Animation frames reveals have asked for on this thread.
    ///
    /// Thread-local rather than global: the harness runs each test on its own
    /// thread, and a shared counter would report another test's frames.
    static REVEAL_FRAME_REQUESTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Records one animation frame requested by a reveal.
///
/// Frame demand is the property under audit, so it is counted at the single
/// site that creates it instead of inferred from progress values. Nothing
/// outside tests compiles the counter.
#[inline]
fn note_reveal_frame_request() {
    #[cfg(test)]
    REVEAL_FRAME_REQUESTS.with(|count| count.set(count.get().saturating_add(1)));
}

/// Reveal frames requested since the last call, and resets the counter.
#[cfg(test)]
fn take_reveal_frame_requests() -> usize {
    REVEAL_FRAME_REQUESTS.with(|count| count.replace(0))
}

/// Fades and lifts an element into place once when it first mounts.
///
/// Keyed by `id`: a row that keeps its stable identity across renders plays
/// the reveal only on its first frames, never on every content update.
pub fn reveal<E>(element: E, id: impl Into<ElementId>, window: &mut Window, cx: &mut App) -> E
where
    E: Styled,
{
    apply_reveal(element, reveal_progress(id, Duration::ZERO, window, cx))
}

/// The per-item delay of a staggered reveal, capped so long lists settle
/// in a bounded time.
fn stagger_delay(index: usize) -> Duration {
    let spec = EnterSpec::REVEAL;
    spec.stagger * index.min(spec.stagger_cap) as u32
}

/// Like [`reveal`], but item `index` waits `index` stagger beats before it
/// starts, so a list of chips or rows ripples into place.
pub fn reveal_staggered<E>(
    element: E,
    id: impl Into<ElementId>,
    index: usize,
    window: &mut Window,
    cx: &mut App,
) -> E
where
    E: Styled,
{
    let delay = stagger_delay(index);
    apply_reveal(element, reveal_progress(id, delay, window, cx))
}

/// Reveal travel from rest, in pixels.
///
/// A derived alias carrying no number of its own: the pixel-discipline gate
/// pins the displacement call site below by its exact expression, so the
/// distance stays a named local rather than an inline field access.
const REVEAL_RISE: f32 = EnterSpec::REVEAL.rise;

fn apply_reveal<E: Styled>(element: E, progress: f32) -> E {
    element
        .opacity(progress)
        .top(px(REVEAL_RISE * (1.0 - progress)))
}

/// Slowly breathes an element's opacity — for indicators that mean "still
/// working" without a measurable progress value.
pub fn breathing<E>(element: E, id: impl Into<ElementId>) -> AnimationElement<E>
where
    E: IntoElement + Styled + 'static,
{
    element.with_animation(
        id,
        // Frame demand: ambient — active for as long as the caller keeps the
        // element mounted, which is the "still working, nothing to report"
        // state itself, so there is no settled frame to reach. Reduced
        // motion holds delta at 0, the middle of the opacity range.
        AmbientLoopSpec::BREATHING
            .looping()
            .with_easing(pulsating_between(0.35, 1.0)),
        |element, alpha| element.opacity(alpha),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Context, Render, TestAppContext, VisualTestContext};
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn shimmer_band_travels_fully_across_and_rests_off_the_trailing_edge() {
        // At the start of the cycle the band sits entirely left of the label.
        let start = -SHIMMER_BAND;
        assert!(start + SHIMMER_BAND <= 0.0);
        // At the end of travel it sits entirely right of the label.
        let end = -SHIMMER_BAND + 1.0 * (1.0 + SHIMMER_BAND);
        assert!(end >= 1.0);
        const {
            assert!(
                ProgressLoopSpec::SHIMMER.duty < 1.0,
                "a rest beat must follow each sweep"
            );
        }
    }

    #[test]
    fn stagger_is_bounded_so_long_lists_do_not_wait_forever() {
        let far = stagger_delay(12);
        let capped = stagger_delay(100);
        assert_eq!(far, capped);
        assert!(stagger_delay(1) < far);
    }

    #[test]
    fn every_loop_resolves_through_a_role_at_its_documented_tempo() {
        // The values a component used to own privately. Changing one here is
        // a visual change, which is what this assertion is for.
        assert_eq!(
            ProgressLoopSpec::GRID_SWEEP.period,
            Duration::from_millis(1400)
        );
        assert_eq!(
            ProgressLoopSpec::IMAGE_PULSE.period,
            Duration::from_millis(1600)
        );
        assert_eq!(
            ProgressLoopSpec::STATUS_SPINNER.period,
            Duration::from_millis(900)
        );
        assert_eq!(
            AmbientLoopSpec::ORB_LATTICE.period,
            Duration::from_millis(1700)
        );
        assert_eq!(AmbientLoopSpec::ORB_LATTICE.period_millis(), 1700);
    }

    /// Runs one reveal per draw and remembers what it returned.
    struct RevealProbe {
        delay: Duration,
        progress: Rc<Cell<f32>>,
    }

    impl Render for RevealProbe {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            self.progress
                .set(reveal_progress("reveal-probe", self.delay, window, cx));
            div()
        }
    }

    fn reveal_probe(
        reduce_motion: bool,
        cx: &mut TestAppContext,
    ) -> (Rc<Cell<f32>>, &mut VisualTestContext) {
        let progress = Rc::new(Cell::new(f32::NAN));
        let (_, cx) = cx.add_window_view({
            let progress = progress.clone();
            move |_, _| RevealProbe {
                delay: Duration::ZERO,
                progress,
            }
        });
        cx.update(|_, cx| cx.set_reduce_motion(reduce_motion));
        // Opening the window may already have drawn; the audit starts here.
        take_reveal_frame_requests();
        (progress, cx)
    }

    fn draw(cx: &mut VisualTestContext) -> usize {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        take_reveal_frame_requests()
    }

    #[gpui::test]
    fn an_active_reveal_requests_a_frame_per_draw_until_it_settles(cx: &mut TestAppContext) {
        let (progress, cx) = reveal_probe(false, cx);

        assert_eq!(draw(cx), 1, "a reveal at zero progress must keep drawing");
        assert_eq!(progress.get(), 0.0);

        cx.executor().advance_clock(EnterSpec::REVEAL.duration / 2);
        assert_eq!(draw(cx), 1, "a half-played reveal must keep drawing");
        assert!(
            (0.0..1.0).contains(&progress.get()),
            "progress={}",
            progress.get()
        );
    }

    #[gpui::test]
    fn a_settled_reveal_requests_no_further_frames(cx: &mut TestAppContext) {
        let (progress, cx) = reveal_probe(false, cx);
        draw(cx);

        cx.executor().advance_clock(EnterSpec::REVEAL.duration);
        assert_eq!(draw(cx), 0, "the settling draw must be the last one");
        assert_eq!(progress.get(), 1.0);

        // Redraws for unrelated reasons must not restart the demand.
        assert_eq!(draw(cx), 0);
        assert_eq!(draw(cx), 0);
        assert_eq!(progress.get(), 1.0);
    }

    #[gpui::test]
    fn reduced_motion_requests_no_frames_and_returns_the_end_state(cx: &mut TestAppContext) {
        let (progress, cx) = reveal_probe(true, cx);

        assert_eq!(draw(cx), 0, "a reduced-motion reveal must not animate");
        assert_eq!(progress.get(), 1.0);

        cx.executor().advance_clock(EnterSpec::REVEAL.duration);
        assert_eq!(draw(cx), 0);
        assert_eq!(progress.get(), 1.0);
    }
}
