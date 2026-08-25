//! Motion primitives shared by gpui-ai components.
//!
//! Every effect here is built on GPUI's animation system, so reduced-motion
//! mode resolves to a useful static frame automatically: one-shot reveals
//! settle at their end state and repeating effects (shimmer, breathing)
//! render at rest. Components opt in explicitly — nothing in this module
//! installs idle redraw on its own.
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
    Animation, AnimationElement, AnimationExt as _, AnyElement, App, ElementId, Hsla, IntoElement,
    ParentElement as _, Pixels, RenderOnce, SharedString, SpringConfig, SpringState,
    StyleRefinement, Styled, Window, canvas, div, ease_in_out, pulsating_between, px, relative,
};
use gpui_base::animation::ease_out_cubic;
use gpui_component::{ActiveTheme as _, StyledExt as _};
use std::time::Duration;

/// One shimmer sweep including its rest beat.
const SHIMMER_CYCLE: Duration = Duration::from_millis(1800);
/// Fraction of the cycle spent travelling; the remainder is a rest beat so
/// consecutive sweeps read as deliberate rather than frantic.
const SHIMMER_TRAVEL: f32 = 0.72;
/// Width of the travelling highlight as a fraction of the label width.
const SHIMMER_BAND: f32 = 0.45;
/// Duration of a one-shot reveal.
const REVEAL_DURATION: Duration = Duration::from_millis(260);
/// Additional delay applied per index by [`reveal_staggered`].
const REVEAL_STAGGER: Duration = Duration::from_millis(40);
/// Reveal travel distance from rest. An animation distance, not layout.
const REVEAL_RISE: f32 = 6.0;
/// One breathing cycle for ambient "still working" indicators.
const BREATH_CYCLE: Duration = Duration::from_millis(1600);

/// The spring a reordered row travels on.
///
/// Slightly under-damped: a row that arrives dead still reads as a redraw,
/// and one that bounces reads as a toy. This overshoots by a few per cent,
/// which is the amount that says "this row moved" without asking to be
/// watched.
const REORDER_SPRING: SpringConfig = SpringConfig {
    stiffness: 220.0,
    damping: 26.0,
    mass: 1.0,
};

/// Under a pixel of displacement is not a move anyone can see.
const REORDER_EPSILON: f32 = 0.5;

/// A frame longer than this is a tab that was in the background, and stepping
/// a spring by it would teleport every row.
const LONGEST_FRAME: Duration = Duration::from_millis(64);

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
                Animation::new(SHIMMER_CYCLE).repeat(),
                move |container, delta| {
                    // Travel during the first part of the cycle, then rest
                    // fully off the trailing edge.
                    let progress = ease_in_out((delta / SHIMMER_TRAVEL).min(1.0));
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
    if cx.reduce_motion() {
        return 1.0;
    }
    let now = cx.background_executor().now();
    let started = *window.use_keyed_state(id, cx, |_, _| now).read(cx);
    let elapsed = now.saturating_duration_since(started);
    let progress = if elapsed <= delay {
        0.0
    } else {
        (elapsed.saturating_sub(delay).as_secs_f32() / REVEAL_DURATION.as_secs_f32()).min(1.0)
    };
    if progress < 1.0 {
        window.request_animation_frame();
    }
    ease_out_cubic(progress)
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
    REVEAL_STAGGER * index.min(12) as u32
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

fn apply_reveal<E: Styled>(element: E, progress: f32) -> E {
    element
        .opacity(progress)
        .top(px(REVEAL_RISE * (1.0 - progress)))
}

/// What one row remembers between frames: where it settled, and how far it
/// currently is from there.
///
/// The frame clock is kept in a second slot rather than a field here, so that
/// the type of an instant is never written down. `background_executor().now()`
/// is not `std::time::Instant` — that one is unimplemented on
/// `wasm32-unknown-unknown` and panics the moment it is read — and naming it
/// would compile natively and break the demos.
#[derive(Clone, Copy, Default)]
struct Reorder {
    /// Where the row lays out when nothing is displacing it.
    settled: Option<Pixels>,
    /// The row's displacement from `settled`, and how fast it is closing.
    spring: SpringState,
}

/// Slides a row from where it was to where it is when a list reorders.
///
/// A list whose rows move — a queue being reordered, a plan whose steps are
/// promoted — redraws them in their new places, and a row that is simply
/// somewhere else on the next frame reads as two rows swapping content rather
/// than one row moving. This carries the row: it is drawn where it was and
/// springs to where it belongs.
///
/// Measured rather than calculated from the index. Rows here are not a uniform
/// height — a queued prompt wraps to two lines, its neighbour to one — so
/// index times a row height would be wrong by however much they differ. The
/// row reports where it laid out and the displacement is the difference from
/// last time, which is right whatever the rows contain.
///
/// Keyed by `id`, which must be the row's stable identity rather than its
/// position: keyed by position, every row would "move" whenever any row did,
/// which is the animation this exists to avoid.
///
/// Under reduced motion the row is returned untouched and no state is kept, so
/// a reordered list simply is reordered.
pub fn reorder<E>(
    element: E,
    id: impl Into<ElementId>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement
where
    E: IntoElement + Styled + 'static,
{
    if cx.reduce_motion() {
        return element.into_any_element();
    }

    let id = id.into();
    let now = cx.background_executor().now();
    let state = window.use_keyed_state((id.clone(), "reorder"), cx, |_, _| Reorder::default());
    let clock = window.use_keyed_state((id, "reorder-clock"), cx, |_, _| now);

    // Capped, because a tab that was in the background hands back one enormous
    // frame, and stepping a spring by it would teleport every row.
    let delta = clock
        .update(cx, |last, _| {
            let elapsed = now.saturating_duration_since(*last);
            *last = now;
            elapsed
        })
        .min(LONGEST_FRAME);

    // Advance whatever is left of the last move before drawing this frame, so
    // the displacement below is current rather than one frame stale.
    let displacement = state.update(cx, |row, _| {
        row.spring = REORDER_SPRING.step(row.spring, 0.0, delta.as_secs_f32());
        if REORDER_SPRING.is_settled(row.spring, 0.0, REORDER_EPSILON) {
            row.spring = SpringState::default();
        }
        row.spring.position
    });

    if displacement.abs() > REORDER_EPSILON {
        window.request_animation_frame();
    }

    let measured = state.clone();
    div()
        .relative()
        .w_full()
        .top(px(displacement))
        .child(element)
        // Reports where this row laid out, with the displacement taken back
        // out — otherwise the row would be measuring its own animation and
        // chase itself down the page.
        .child(
            canvas(
                move |bounds, _, cx| {
                    let settled = bounds.origin.y - px(displacement);
                    measured.update(cx, |row, _| {
                        if let Some(previous) = row.settled {
                            let moved = f32::from(previous - settled);
                            if moved.abs() > REORDER_EPSILON {
                                // It is now drawn where it was, and the step
                                // above will carry it the rest of the way.
                                row.spring.position += moved;
                            }
                        }
                        row.settled = Some(settled);
                    });
                },
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0(),
        )
        .into_any_element()
}

/// Slowly breathes an element's opacity — for indicators that mean "still
/// working" without a measurable progress value.
pub fn breathing<E>(element: E, id: impl Into<ElementId>) -> AnimationElement<E>
where
    E: IntoElement + Styled + 'static,
{
    element.with_animation(
        id,
        Animation::new(BREATH_CYCLE)
            .repeat()
            .with_easing(pulsating_between(0.35, 1.0)),
        |element, alpha| element.opacity(alpha),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shimmer_band_travels_fully_across_and_rests_off_the_trailing_edge() {
        // At the start of the cycle the band sits entirely left of the label.
        let start = -SHIMMER_BAND;
        assert!(start + SHIMMER_BAND <= 0.0);
        // At the end of travel it sits entirely right of the label.
        let end = -SHIMMER_BAND + 1.0 * (1.0 + SHIMMER_BAND);
        assert!(end >= 1.0);
        const {
            assert!(SHIMMER_TRAVEL < 1.0, "a rest beat must follow each sweep");
        }
    }

    #[test]
    fn stagger_is_bounded_so_long_lists_do_not_wait_forever() {
        let far = stagger_delay(12);
        let capped = stagger_delay(100);
        assert_eq!(far, capped);
        assert!(stagger_delay(1) < far);
    }

    /// Steps the reorder spring the way a frame loop does, and reports where
    /// the row is after each frame.
    fn travel(from: f32, frames: usize) -> Vec<f32> {
        let mut state = SpringState {
            position: from,
            velocity: 0.0,
        };
        let frame = Duration::from_millis(16).as_secs_f32();
        (0..frames)
            .map(|_| {
                state = REORDER_SPRING.step(state, 0.0, frame);
                state.position
            })
            .collect()
    }

    #[test]
    fn a_reordered_row_travels_to_where_it_belongs_and_stays() {
        // A row displaced by the height of its neighbour closes on zero.
        let path = travel(64.0, 60);
        let arrived = path.last().copied().expect("frames were stepped");
        assert!(
            arrived.abs() < REORDER_EPSILON,
            "a row must arrive; it stopped {arrived}px away"
        );
        assert!(
            REORDER_SPRING.is_settled(
                SpringState {
                    position: arrived,
                    velocity: 0.0
                },
                0.0,
                REORDER_EPSILON
            ),
            "and must be settled once it has, or it redraws for ever"
        );
    }

    #[test]
    fn a_reordered_row_arrives_within_a_second() {
        // Longer than this and the list is still rearranging itself while the
        // reader has moved on; the row is furniture, not an announcement.
        let within_a_second = travel(64.0, 62);
        assert!(
            within_a_second
                .last()
                .copied()
                .expect("frames were stepped")
                .abs()
                < REORDER_EPSILON
        );
    }

    #[test]
    fn a_reordered_row_overshoots_a_little_and_only_once() {
        // Enough to read as movement, not enough to read as a toy: the spring
        // crosses its target and comes back, and the return is small.
        let path = travel(100.0, 90);
        let overshoot = path
            .iter()
            .copied()
            .filter(|position| *position < 0.0)
            .fold(0.0_f32, |worst, position| worst.min(position));
        assert!(
            overshoot < 0.0,
            "a row that never crosses reads as a redraw"
        );
        assert!(
            overshoot.abs() < 10.0,
            "overshot by {overshoot}px of 100, which is a bounce"
        );
    }

    #[test]
    fn a_row_that_has_not_moved_is_not_animated() {
        // The common case by far: a list redraws and nothing changed places.
        // Every frame of that must cost nothing and request nothing.
        let settled = SpringState::default();
        assert!(REORDER_SPRING.is_settled(settled, 0.0, REORDER_EPSILON));
        assert_eq!(travel(0.0, 8), vec![0.0; 8]);
    }

    #[test]
    fn a_long_frame_cannot_teleport_a_row() {
        // A backgrounded tab hands back one enormous frame. Stepping a spring
        // by it lands the row at its target instantly, which is the jump this
        // exists to remove.
        assert!(LONGEST_FRAME <= Duration::from_millis(100));
        let capped = Duration::from_secs(30).min(LONGEST_FRAME);
        assert_eq!(capped, LONGEST_FRAME);
    }
}
