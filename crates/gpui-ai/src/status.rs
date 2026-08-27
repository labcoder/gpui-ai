//! One status vocabulary for every lifecycle the library displays.
//!
//! Tool chips, task rows, tool-call cards, and plan steps all describe work
//! as pending, running, completed, or failed. [`StatusTone`] maps those
//! meanings onto the theme's semantic colors once, and [`StatusBadge`] is the
//! single compact pill that renders them, so status looks identical wherever
//! it appears.

use crate::motion::swap_progress;
use crate::stream::ProgressState;
use crate::theme::SemanticStyledExt as _;
use gpui::{
    App, ElementId, FontWeight, Hsla, InteractiveElement as _, IntoElement, ParentElement as _,
    RenderOnce, Role, SharedString, StatefulInteractiveElement as _, StyleRefinement, Styled,
    Window, div, prelude::FluentBuilder as _, rems,
};
use gpui_component::{
    ActiveTheme as _, Sizable as _, StyledExt as _, h_flex, spinner::Spinner, v_flex,
};

/// The meaning a status carries, independent of any one component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatusTone {
    /// Not started, idle, or informational-neutral.
    #[default]
    Neutral,
    /// Actively in progress.
    Info,
    /// Finished successfully.
    Success,
    /// Needs attention but is not an error.
    Warning,
    /// Failed or destructive.
    Danger,
}

impl StatusTone {
    /// Resolves the tone to the active theme's semantic color.
    pub fn color(self, cx: &App) -> Hsla {
        match self {
            Self::Neutral => cx.theme().muted_foreground,
            Self::Info => cx.theme().info,
            Self::Success => cx.theme().success,
            Self::Warning => cx.theme().warning,
            Self::Danger => cx.theme().danger,
        }
    }

    /// Maps the shared progressive lifecycle onto a tone.
    pub fn from_progress(state: &ProgressState) -> Self {
        match state {
            ProgressState::Pending => Self::Neutral,
            ProgressState::Running => Self::Info,
            ProgressState::Complete => Self::Success,
            ProgressState::Failed(_) => Self::Danger,
        }
    }
}

/// Human-readable label for a progressive lifecycle state.
pub fn progress_label(state: &ProgressState) -> &'static str {
    match state {
        ProgressState::Pending => "Pending",
        ProgressState::Running => "Running",
        ProgressState::Complete => "Completed",
        ProgressState::Failed(_) => "Failed",
    }
}

/// A compact status pill: a tone-colored dot (or spinner while active) and a
/// short label, exposed as a named status region.
///
/// # Example
///
/// ```ignore
/// StatusBadge::for_progress("call-status", task.state())
/// StatusBadge::new("review", "Needs review").tone(StatusTone::Warning)
/// ```
#[derive(IntoElement)]
pub struct StatusBadge {
    id: ElementId,
    style: StyleRefinement,
    label: SharedString,
    tone: StatusTone,
    active: bool,
    reserve_lifecycle_width: bool,
}

/// What the badge showed last, held in keyed window state so a change can
/// stage the outgoing status while the incoming one settles into the slot.
struct StatusSwap {
    label: SharedString,
    tone: StatusTone,
    active: bool,
    /// Bumped per change; keys the swap clock so each change plays once.
    generation: u64,
    /// The status being faded out, present only while a swap is running.
    /// An outgoing spinner is frozen to its tone's dot: a settling copy of
    /// an animation would be a second active-work signal.
    outgoing: Option<(SharedString, StatusTone)>,
}

impl StatusBadge {
    /// Creates a neutral badge.
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            style: StyleRefinement::default(),
            label: label.into(),
            tone: StatusTone::Neutral,
            active: false,
            reserve_lifecycle_width: false,
        }
    }

    /// Creates a badge describing a progressive lifecycle state.
    ///
    /// Lifecycle badges reserve the width of the widest lifecycle label, so
    /// pending→running→completed/failed never moves the layout around the
    /// pill — the fixed slot the status swap animates inside.
    pub fn for_progress(id: impl Into<ElementId>, state: &ProgressState) -> Self {
        let mut badge = Self::new(id, progress_label(state))
            .tone(StatusTone::from_progress(state))
            .active(matches!(state, ProgressState::Running));
        badge.reserve_lifecycle_width = true;
        badge
    }

    /// Sets the badge tone.
    pub fn tone(mut self, tone: StatusTone) -> Self {
        self.tone = tone;
        self
    }

    /// Shows a spinner instead of the dot while work is in progress.
    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }
}

impl Styled for StatusBadge {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for StatusBadge {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = cx.theme().semantic_tokens();
        let color = self.tone.color(cx);

        // The swap is staged from keyed state: the first render is not a
        // transition, and every later change fades the outgoing status away
        // while the incoming one lifts into the same fixed slot. Semantics
        // never wait — aria_label and the tone below are the incoming state
        // from the first frame of the change.
        let swap = window.use_keyed_state((self.id.clone(), "status-swap"), cx, |_, _| StatusSwap {
            label: self.label.clone(),
            tone: self.tone,
            active: self.active,
            generation: 0,
            outgoing: None,
        });
        let (generation, outgoing) = swap.update(cx, |swap, _| {
            if swap.label != self.label || swap.tone != self.tone || swap.active != self.active {
                // A label or tone change stages the old face; an
                // active-only flip (spinner to dot on the same status) just
                // swaps the indicator without staging a ghost label.
                if swap.label != self.label || swap.tone != self.tone {
                    swap.outgoing = Some((swap.label.clone(), swap.tone));
                }
                swap.generation += 1;
                swap.label = self.label.clone();
                swap.tone = self.tone;
                swap.active = self.active;
            }
            (swap.generation, swap.outgoing.clone())
        });
        let progress = if generation == 0 {
            1.0
        } else {
            swap_progress(
                ElementId::NamedInteger(
                    SharedString::from(format!("{:?}-status-swap", self.id)),
                    generation,
                ),
                window,
                cx,
            )
        };
        if progress >= 1.0 && outgoing.is_some() {
            swap.update(cx, |swap, _| swap.outgoing = None);
        }
        let exit = tokens.spacing.xxs * progress;
        let entry = tokens.spacing.xxs * (1.0 - progress);

        // The indicator slot is fixed — sized for the spinner, centering the
        // smaller dot — so running↔settled never nudges the label sideways.
        let indicator = div()
            .flex_none()
            .size(tokens.spacing.md)
            .flex()
            .items_center()
            .justify_center()
            .child(if self.active {
                Spinner::new().xsmall().color(color).into_any_element()
            } else {
                div()
                    .size_1p5()
                    .rounded(tokens.radius.full)
                    .bg(color)
                    .opacity(progress)
                    .into_any_element()
            });

        let label_slot = v_flex()
            .relative()
            .when(self.reserve_lifecycle_width, |slot| {
                // Zero-height ghosts of every lifecycle label make the slot
                // as wide as the widest one, in whatever face and rem scale
                // the theme resolves — so the whole pending→failed journey
                // fits one width with nothing measured. The pill's
                // aria_label is the accessible name; these never paint.
                slot.children(LIFECYCLE_LABELS.iter().map(|label| {
                    div()
                        .h(rems(0.))
                        .overflow_hidden()
                        .opacity(0.)
                        .child(*label)
                }))
            })
            .child(
                div()
                    .opacity(progress)
                    .top(entry)
                    .child(self.label.clone()),
            )
            .when_some(outgoing, |slot, (label, tone)| {
                // The outgoing face drifts down as it fades, layered over
                // the slot so it never contributes width of its own.
                slot.child(
                    div()
                        .absolute()
                        .left(rems(0.))
                        .top(exit)
                        .opacity(1.0 - progress)
                        .text_color(tone.color(cx))
                        .child(label),
                )
            });

        h_flex()
            .id(self.id)
            .role(Role::Status)
            .aria_label(self.label.clone())
            .flex_none()
            .items_center()
            .gap(tokens.spacing.xs)
            .px(tokens.spacing.sm)
            .py(tokens.spacing.xxs)
            .rounded(tokens.radius.full)
            .bg(color.opacity(0.12))
            .text_token(tokens.typography.xs)
            .font_weight(FontWeight::MEDIUM)
            .text_color(color)
            .child(indicator)
            .child(label_slot)
            .refine_style(&self.style)
    }
}

/// Every label [`progress_label`] can produce, for the lifecycle slot's
/// width reservation. A new lifecycle label joins both or the reservation
/// silently under-measures.
const LIFECYCLE_LABELS: [&str; 4] = ["Pending", "Running", "Completed", "Failed"];

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Context, Entity, Render, TestAppContext, VisualTestContext, px, size};
    use std::time::Duration;

    struct BadgeProbe {
        state: ProgressState,
    }

    impl Render for BadgeProbe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().w(px(320.)).h(px(80.)).child(
                h_flex()
                    .debug_selector(|| "badge-hug".into())
                    .flex_none()
                    .child(StatusBadge::for_progress("probe-badge", &self.state)),
            )
        }
    }

    fn open(cx: &mut TestAppContext) -> (Entity<BadgeProbe>, &mut VisualTestContext) {
        cx.update(crate::init);
        let (probe, cx) = cx.add_window_view(|_, _| BadgeProbe {
            state: ProgressState::Pending,
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        (probe, cx)
    }

    fn draw(cx: &mut VisualTestContext) {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    fn set_state(probe: &Entity<BadgeProbe>, cx: &mut VisualTestContext, state: ProgressState) {
        probe.update(cx, |probe, cx| {
            probe.state = state;
            cx.notify();
        });
        draw(cx);
    }

    fn settle_swap(cx: &mut VisualTestContext) {
        cx.executor()
            .advance_clock(crate::motion::MotionTokens::DEFAULT.quick() * 2);
        draw(cx);
        draw(cx);
    }

    #[gpui::test]
    fn the_pill_keeps_one_width_across_the_whole_lifecycle(cx: &mut TestAppContext) {
        let (probe, cx) = open(cx);
        let width_at = |cx: &mut VisualTestContext| {
            cx.debug_bounds("badge-hug")
                .expect("the badge should render")
                .size
                .width
        };
        let pending = width_at(cx);

        for state in [
            ProgressState::Running,
            ProgressState::Complete,
            ProgressState::Failed("offline".into()),
        ] {
            set_state(&probe, cx, state.clone());
            settle_swap(cx);
            assert_eq!(
                width_at(cx),
                pending,
                "the lifecycle slot must hold one width; {state:?} moved it"
            );
        }
    }

    #[gpui::test]
    fn a_status_change_stages_a_swap_and_settles_quiet(cx: &mut TestAppContext) {
        let (probe, cx) = open(cx);
        crate::motion::take_reveal_frame_requests();

        set_state(&probe, cx, ProgressState::Complete);
        assert!(
            crate::motion::take_reveal_frame_requests() > 0,
            "a status change must acknowledge itself"
        );

        settle_swap(cx);
        crate::motion::take_reveal_frame_requests();
        draw(cx);
        assert_eq!(
            crate::motion::take_reveal_frame_requests(),
            0,
            "a settled swap must schedule nothing"
        );
    }

    #[gpui::test]
    fn the_first_render_is_not_a_transition(cx: &mut TestAppContext) {
        let (_, cx) = open(cx);
        crate::motion::take_reveal_frame_requests();
        draw(cx);
        assert_eq!(
            crate::motion::take_reveal_frame_requests(),
            0,
            "a badge mounts at rest, whatever state it mounts in"
        );
    }

    #[gpui::test]
    fn reduced_motion_swaps_without_frames(cx: &mut TestAppContext) {
        cx.update(|cx| cx.set_reduce_motion(true));
        let (probe, cx) = open(cx);
        crate::motion::take_reveal_frame_requests();

        set_state(&probe, cx, ProgressState::Failed("offline".into()));
        draw(cx);
        assert_eq!(
            crate::motion::take_reveal_frame_requests(),
            0,
            "reduced motion resolves the swap instantly"
        );
    }

    #[test]
    fn every_lifecycle_label_is_reserved() {
        for state in [
            ProgressState::Pending,
            ProgressState::Running,
            ProgressState::Complete,
            ProgressState::Failed("offline".into()),
        ] {
            assert!(
                LIFECYCLE_LABELS.contains(&progress_label(&state)),
                "the width reservation must know every label for_progress can show"
            );
        }
    }

    #[test]
    fn progress_maps_onto_one_tone_and_label_each() {
        let cases = [
            (ProgressState::Pending, StatusTone::Neutral, "Pending"),
            (ProgressState::Running, StatusTone::Info, "Running"),
            (ProgressState::Complete, StatusTone::Success, "Completed"),
            (
                ProgressState::Failed("offline".into()),
                StatusTone::Danger,
                "Failed",
            ),
        ];
        for (state, tone, label) in cases {
            assert_eq!(StatusTone::from_progress(&state), tone);
            assert_eq!(progress_label(&state), label);
            let badge = StatusBadge::for_progress("badge", &state);
            assert_eq!(badge.tone, tone);
            assert_eq!(badge.label.as_ref(), label);
            assert_eq!(badge.active, matches!(state, ProgressState::Running));
        }
    }
}
