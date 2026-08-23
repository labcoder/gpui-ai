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
    Animation, AnimationElement, AnimationExt as _, App, ElementId, Hsla, IntoElement,
    ParentElement as _, RenderOnce, SharedString, StyleRefinement, Styled, Window, div,
    ease_in_out, pulsating_between, px, relative,
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
}
