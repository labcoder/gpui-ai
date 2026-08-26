//! Expandable progressive reasoning traces.
//!
//! While reasoning runs the disclosure opens on its own, its title shimmers,
//! and a live preview follows the newest steps — until the reader scrolls
//! back through earlier ones, which holds their position until they return to
//! the tail. Once the trace settles it collapses to "Thought for Ns" and the
//! full trace stays one click (or Enter) away. Applications may override the
//! automatic policy with [`Thinking::open`].

use crate::{
    control::composed_button,
    handlers::Handler,
    motion::{Shimmer, reveal},
    stream::{ProgressState, Progressive},
    theme::SemanticStyledExt as _,
};
use gpui::{
    App, ClickEvent, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    RenderOnce, Role, ScrollHandle, SharedString, StatefulInteractiveElement as _, StyleRefinement,
    Styled, Window, div, prelude::FluentBuilder as _,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable as _, StyledExt as _, h_flex, spinner::Spinner,
    text::TextView, v_flex,
};
use std::{
    hash::{DefaultHasher, Hash as _, Hasher as _},
    mem::discriminant,
    time::Duration,
};

/// Status of one step inside a [`ThinkingTrace`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StepStatus {
    /// The step is in progress; it shows a spinner.
    #[default]
    Running,
    /// The step completed.
    Done,
}

/// One step of a reasoning trace: a title plus optional detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThinkingStep {
    title: SharedString,
    detail: Option<SharedString>,
    status: StepStatus,
}

impl ThinkingStep {
    /// Creates a step with a short title.
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            detail: None,
            status: StepStatus::default(),
        }
    }

    /// Adds detail rendered as muted markdown under the title.
    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Sets the step status.
    pub fn status(mut self, status: StepStatus) -> Self {
        self.status = status;
        self
    }
}

/// Typed content of a progressive thinking trace.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ThinkingTrace {
    prose: Option<SharedString>,
    steps: Vec<ThinkingStep>,
    thought_for: Option<Duration>,
}

impl ThinkingTrace {
    /// Creates an empty trace.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets free-form reasoning rendered before structured steps.
    pub fn prose(mut self, prose: impl Into<SharedString>) -> Self {
        self.prose = Some(prose.into());
        self
    }

    /// Sets structured trace steps.
    pub fn steps(mut self, steps: impl IntoIterator<Item = ThinkingStep>) -> Self {
        self.steps = steps.into_iter().collect();
        self
    }

    /// Sets the caller-measured thinking duration.
    pub fn thought_for(mut self, duration: Duration) -> Self {
        self.thought_for = Some(duration);
        self
    }
}

/// An interaction emitted by [`Thinking`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThinkingEvent {
    /// Requests a controlled expansion-state change.
    Toggled {
        /// Stable trace identifier.
        id: SharedString,
        /// Proposed expansion state.
        open: bool,
    },
}

/// An accessible reasoning disclosure with an automatic streaming policy.
#[derive(IntoElement)]
pub struct Thinking {
    id: SharedString,
    style: StyleRefinement,
    open: Option<bool>,
    state: ProgressState,
    trace: ThinkingTrace,
    revision: u64,
    on_event: Option<Handler<ThinkingEvent>>,
}

impl Thinking {
    /// Creates a trace from a progressive snapshot.
    ///
    /// Without an explicit [`Self::open`] the trace is expanded while
    /// reasoning runs and collapsed once it settles.
    pub fn new(id: impl Into<SharedString>, trace: &Progressive<ThinkingTrace>) -> Self {
        Self {
            id: id.into(),
            style: StyleRefinement::default(),
            open: None,
            state: trace.state().clone(),
            trace: trace.content().clone(),
            revision: trace.revision(),
            on_event: None,
        }
    }

    /// Sets the expansion state explicitly, replacing the automatic policy.
    pub fn open(mut self, open: bool) -> Self {
        self.open = Some(open);
        self
    }

    /// Handles typed trace interactions.
    pub fn on_event(
        mut self,
        handler: impl Fn(&ThinkingEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_event = Some(Box::new(handler));
        self
    }

    /// Whether the body is shown: the explicit value, or "open while running".
    pub fn is_open(&self) -> bool {
        self.open.unwrap_or(self.state == ProgressState::Running)
    }
}

impl Styled for Thinking {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// Live-preview state for one trace, held in keyed window state.
///
/// A [`RenderOnce`] trace remembers nothing across frames, so without this it
/// cannot tell newly streamed reasoning from a re-render and drags the
/// preview back to the tail on every frame. The key carries the trace's
/// stable ID, so a different trace starts with a fresh scroll position and a
/// fresh follow decision.
struct LivePreview {
    scroll: ScrollHandle,
    /// Digest of the content the preview last showed.
    revision: Option<u64>,
    /// Whether the user was at the tail when the preview last rendered.
    follow: bool,
}

impl LivePreview {
    fn new() -> Self {
        Self {
            scroll: ScrollHandle::new(),
            revision: None,
            follow: true,
        }
    }

    /// Records `revision` and the position the user left the preview in, and
    /// answers whether this render should land on the tail. Content the
    /// preview has already shown never scrolls, and neither does new content
    /// once the user has scrolled away to read earlier reasoning.
    fn observe(&mut self, revision: u64, slack: Pixels) -> bool {
        self.follow = follows_tail(self.scroll.offset().y, self.scroll.max_offset().y, slack);
        let arrived = self.revision != Some(revision);
        self.revision = Some(revision);
        arrived && self.follow
    }
}

/// Whether a preview resting at `offset_y` still counts as following the tail.
///
/// GPUI scroll offsets are zero at the top of the content and `-max_offset` at
/// the bottom, so `offset_y + max_offset_y` is the distance left to the tail.
/// `slack` keeps a preview that stopped a fraction short of the edge — wheel
/// and trackpad gestures rarely land exactly on it — still following.
fn follows_tail(offset_y: Pixels, max_offset_y: Pixels, slack: Pixels) -> bool {
    max_offset_y <= Pixels::ZERO || offset_y + max_offset_y <= slack
}

/// Digest of everything the live preview shows.
///
/// Applications rebuild their [`Progressive`] snapshot every frame, so a trace
/// that grew and one that merely re-rendered both report revision zero. The
/// digest folds the trace itself in, which is what makes appended reasoning
/// distinguishable from a repaint.
fn content_revision(revision: u64, trace: &ThinkingTrace) -> u64 {
    let mut hasher = DefaultHasher::new();
    revision.hash(&mut hasher);
    trace.prose.hash(&mut hasher);
    trace.thought_for.hash(&mut hasher);
    for step in &trace.steps {
        step.title.hash(&mut hasher);
        step.detail.hash(&mut hasher);
        discriminant(&step.status).hash(&mut hasher);
    }
    hasher.finish()
}

impl RenderOnce for Thinking {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = cx.theme().semantic_tokens();
        let live = self.state == ProgressState::Running;
        let open = self.is_open();
        let title: SharedString = match (&self.state, self.trace.thought_for) {
            (ProgressState::Running, _) => "Thinking…".into(),
            (ProgressState::Failed(_), _) => "Thinking stopped".into(),
            (_, Some(duration)) => format!("Thought for {:.0}s", duration.as_secs_f64()).into(),
            _ => "Thoughts".into(),
        };
        let chevron = if open {
            IconName::ChevronDown
        } else {
            IconName::ChevronRight
        };
        let event = ThinkingEvent::Toggled {
            id: self.id.clone(),
            open: !open,
        };
        let failed = match &self.state {
            ProgressState::Failed(reason) => Some(reason.clone()),
            _ => None,
        };
        let trace_id = self.id.clone();
        let root_id = ElementId::from(self.id.clone());
        // A collapsed trace has no preview to follow, so it keeps no follow
        // state either: reopening starts again on the newest reasoning.
        let live_revision = (live && open).then(|| content_revision(self.revision, &self.trace));
        let interactive = self.on_event.is_some();
        let header = h_flex()
            .items_center()
            .gap(tokens.spacing.xs)
            .text_token(tokens.typography.sm)
            .text_color(cx.theme().muted_foreground)
            .when(interactive, |this| this.child(Icon::new(chevron).xsmall()))
            .child(
                Shimmer::new((root_id.clone(), "title"), title.clone())
                    .active(live)
                    .text_token(tokens.typography.sm),
            );
        let toggle = match self.on_event {
            Some(handler) => composed_button(format!("{}-toggle", self.id), title.clone())
                .aria_expanded(open)
                .px(tokens.spacing.xs)
                .py(tokens.spacing.xxs)
                .rounded(tokens.radius.sm)
                .hover(|style| style.bg(cx.theme().accent))
                .active(|style| style.bg(cx.theme().accent.opacity(0.8)))
                .focus_visible(|style| style.bg(cx.theme().accent))
                .child(header)
                .on_click(move |_: &ClickEvent, window, cx| handler(&event, window, cx))
                .into_any_element(),
            None => header.into_any_element(),
        };

        let body = v_flex()
            .gap(tokens.spacing.sm)
            .pl(tokens.spacing.md)
            .ml(tokens.spacing.xs)
            .border_l_1()
            .border_color(cx.theme().border)
            .when_some(self.trace.prose, |this, prose| {
                this.child(
                    div()
                        .text_token(tokens.typography.sm)
                        .text_color(cx.theme().muted_foreground)
                        .child(TextView::markdown("prose", prose).selectable(true)),
                )
            })
            .children(self.trace.steps.into_iter().enumerate().map(|(ix, step)| {
                let accessibility_label: SharedString = format!(
                    "{}, {}",
                    step.title,
                    match step.status {
                        StepStatus::Running => "in progress",
                        StepStatus::Done => "complete",
                    }
                )
                .into();
                let indicator = match step.status {
                    StepStatus::Running => Spinner::new()
                        .xsmall()
                        .color(cx.theme().info)
                        .into_any_element(),
                    // Completion settles in with a one-shot reveal; the
                    // step keeps its stable id, so re-renders never replay it.
                    StepStatus::Done => reveal(
                        div()
                            .size_1p5()
                            .rounded(tokens.radius.full)
                            .bg(cx.theme().success),
                        ElementId::NamedInteger(format!("{trace_id}-step-done").into(), ix as u64),
                        window,
                        cx,
                    )
                    .into_any_element(),
                };
                v_flex()
                    .id((trace_id.clone(), ix))
                    .role(Role::ListItem)
                    .aria_label(accessibility_label)
                    .gap(tokens.spacing.xxs)
                    .child(
                        h_flex()
                            .items_center()
                            .gap(tokens.spacing.xs)
                            .text_token(tokens.typography.sm)
                            .text_color(cx.theme().foreground)
                            .child(
                                div()
                                    .flex_none()
                                    .w(tokens.spacing.sm)
                                    .flex()
                                    .justify_center()
                                    .child(indicator),
                            )
                            .child(step.title),
                    )
                    .when_some(step.detail, |this, detail| {
                        this.child(
                            div()
                                .pl(tokens.spacing.md)
                                .text_token(tokens.typography.sm)
                                .text_color(cx.theme().muted_foreground)
                                .child(
                                    TextView::markdown(("step-detail", ix), detail)
                                        .selectable(true),
                                ),
                        )
                    })
            }))
            .when_some(failed.clone(), |this, reason| {
                this.child(
                    div()
                        .text_token(tokens.typography.sm)
                        .text_color(cx.theme().danger)
                        .child(reason),
                )
            })
            .debug_selector(|| format!("thinking-body-{trace_id}"));

        // While reasoning streams, the body is a bounded live preview that
        // follows its newest content; it still scrolls, and the full trace
        // renders unbounded once the state settles.
        let body = match live_revision {
            Some(revision) => {
                let preview =
                    window.use_keyed_state((root_id.clone(), "live-follow"), cx, |_, _| {
                        LivePreview::new()
                    });
                // GPUI applies the request during this preview's own prepaint,
                // so it has to be made while the tree is built — a prepaint or
                // next-frame hook lands a frame late. It is never
                // unconditional: it takes content this preview has not shown
                // yet plus a user who has not scrolled away from the tail.
                let (scroll, follow) = preview.update(cx, |state, _| {
                    (
                        state.scroll.clone(),
                        state.observe(revision, tokens.typography.sm.line_height),
                    )
                });
                if follow {
                    scroll.scroll_to_bottom();
                }
                div()
                    .id((root_id.clone(), "live-preview"))
                    .debug_selector(|| format!("thinking-live-preview-{trace_id}"))
                    .max_h(tokens.spacing.xxl * 4.0)
                    .overflow_y_scroll()
                    .track_scroll(&scroll)
                    .child(body)
                    .into_any_element()
            }
            None => body.into_any_element(),
        };

        v_flex()
            .id(self.id.clone())
            .role(Role::Group)
            .aria_label(title)
            .when_some(failed, |this, reason| this.aria_description(reason))
            .gap(tokens.spacing.xs)
            .child(toggle)
            .when(open, |this| this.child(body))
            .refine_style(&self.style)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{
        Context, Entity, Render, ScrollDelta, ScrollWheelEvent, TestAppContext, VisualTestContext,
        point, px,
    };

    /// Stable trace ID for the window tests. `debug_bounds` takes a literal,
    /// so the selectors below have to spell the ID out.
    const TRACE_ID: &str = "trace";
    const PREVIEW: &str = "thinking-live-preview-trace";
    const BODY: &str = "thinking-body-trace";
    /// Steps enough to overflow the preview's four-`xxl` cap several times.
    const STEPS: usize = 16;

    /// A trace mounted in a host taller than the live preview, so overflow is
    /// the preview's own bound rather than the window's.
    struct TraceProbe {
        state: ProgressState,
        steps: usize,
        open: bool,
    }

    impl TraceProbe {
        fn running() -> Self {
            Self {
                state: ProgressState::Running,
                steps: STEPS,
                open: true,
            }
        }

        /// Rebuilt per frame, exactly as applications rebuild theirs — every
        /// snapshot therefore reports revision zero.
        fn progress(&self) -> Progressive<ThinkingTrace> {
            let trace = ThinkingTrace::new()
                .steps((0..self.steps).map(|ix| ThinkingStep::new(format!("Reasoning step {ix}"))));
            match &self.state {
                ProgressState::Pending => Progressive::pending(trace),
                ProgressState::Running => Progressive::running(trace),
                ProgressState::Complete => Progressive::complete(trace),
                ProgressState::Failed(reason) => Progressive::failed(trace, reason.clone()),
            }
        }
    }

    impl Render for TraceProbe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .w(px(320.))
                .h(px(480.))
                .child(Thinking::new(TRACE_ID, &self.progress()).open(self.open))
        }
    }

    fn draw(cx: &mut VisualTestContext) {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    /// Where the trace body sits inside its live preview: zero at the top of
    /// the content, negative once the preview has scrolled down.
    fn scroll_offset(cx: &mut VisualTestContext) -> Pixels {
        let preview = cx.debug_bounds(PREVIEW).expect("the preview should render");
        let body = cx.debug_bounds(BODY).expect("the trace body should render");
        body.top() - preview.top()
    }

    /// How far the end of the content sits below the preview: zero while the
    /// preview is pinned to the tail.
    fn distance_past_tail(cx: &mut VisualTestContext) -> f32 {
        let preview = cx.debug_bounds(PREVIEW).expect("the preview should render");
        let body = cx.debug_bounds(BODY).expect("the trace body should render");
        (body.bottom() - preview.bottom()).as_f32()
    }

    fn assert_pinned_to_tail(cx: &mut VisualTestContext, expectation: &str) {
        let overflow = {
            let preview = cx.debug_bounds(PREVIEW).expect("the preview should render");
            let body = cx.debug_bounds(BODY).expect("the trace body should render");
            (body.size.height - preview.size.height).as_f32()
        };
        assert!(
            overflow > 0.0,
            "the trace must outgrow the preview for this to mean anything"
        );
        assert!(
            distance_past_tail(cx).abs() < 1.0,
            "{expectation} (content ends {} past the preview)",
            distance_past_tail(cx)
        );
    }

    fn append_step(probe: &Entity<TraceProbe>, cx: &mut VisualTestContext) {
        probe.update(cx, |probe, cx| {
            probe.steps += 1;
            cx.notify();
        });
        draw(cx);
    }

    fn set_open(probe: &Entity<TraceProbe>, cx: &mut VisualTestContext, open: bool) {
        probe.update(cx, |probe, cx| {
            probe.open = open;
            cx.notify();
        });
        draw(cx);
    }

    /// Wheels the preview back toward earlier reasoning; positive GPUI offsets
    /// move toward the top of the content.
    fn scroll_up(cx: &mut VisualTestContext, distance: f32) {
        let preview = cx.debug_bounds(PREVIEW).expect("the preview should render");
        cx.simulate_event(ScrollWheelEvent {
            position: preview.center(),
            delta: ScrollDelta::Pixels(point(px(0.), px(distance))),
            ..Default::default()
        });
        draw(cx);
    }

    #[gpui::test]
    fn streamed_reasoning_stays_pinned_to_the_tail_while_following(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (probe, cx) = cx.add_window_view(|_, _| TraceProbe::running());
        let cx: &mut VisualTestContext = cx;
        draw(cx);

        assert_pinned_to_tail(cx, "a live preview opens on the newest reasoning");
        append_step(&probe, cx);
        assert_pinned_to_tail(
            cx,
            "streamed reasoning keeps a followed preview at the tail",
        );
    }

    #[gpui::test]
    fn streamed_reasoning_after_a_scroll_up_keeps_the_users_position(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (probe, cx) = cx.add_window_view(|_, _| TraceProbe::running());
        let cx: &mut VisualTestContext = cx;
        draw(cx);
        let tail = scroll_offset(cx);

        scroll_up(cx, 80.);
        let inspecting = scroll_offset(cx);
        assert!(
            inspecting > tail,
            "the wheel should have moved the preview off the tail"
        );

        append_step(&probe, cx);
        assert_eq!(
            scroll_offset(cx),
            inspecting,
            "streamed reasoning must not steal the position the user chose"
        );
        append_step(&probe, cx);
        assert_eq!(
            scroll_offset(cx),
            inspecting,
            "and it must not creep back over repeated updates"
        );
    }

    #[gpui::test]
    fn collapsing_and_reopening_returns_to_the_newest_reasoning(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (probe, cx) = cx.add_window_view(|_, _| TraceProbe::running());
        let cx: &mut VisualTestContext = cx;
        draw(cx);
        scroll_up(cx, 80.);
        assert!(distance_past_tail(cx) > 1.0, "the preview is off the tail");

        set_open(&probe, cx, false);
        assert!(
            cx.debug_bounds(PREVIEW).is_none(),
            "a collapsed trace renders no live preview"
        );

        // Reasoning keeps streaming behind the collapsed disclosure.
        probe.update(cx, |probe, cx| {
            probe.steps += 1;
            cx.notify();
        });
        set_open(&probe, cx, true);
        assert_pinned_to_tail(cx, "reopening a live trace lands on the newest reasoning");
    }

    #[gpui::test]
    fn a_settled_trace_stops_adjusting_the_scroll_position(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (probe, cx) = cx.add_window_view(|_, _| TraceProbe::running());
        let cx: &mut VisualTestContext = cx;
        draw(cx);
        scroll_up(cx, 80.);

        probe.update(cx, |probe, cx| {
            probe.state = ProgressState::Failed("reasoning interrupted".into());
            cx.notify();
        });
        draw(cx);
        assert!(
            cx.debug_bounds(PREVIEW).is_none(),
            "a settled trace renders the full trace, not a bounded preview"
        );
        let settled = cx.debug_bounds(BODY).expect("the trace body should render");

        append_step(&probe, cx);
        let after = cx.debug_bounds(BODY).expect("the trace body should render");
        assert_eq!(
            after.top(),
            settled.top(),
            "a settled trace never moves its content"
        );
        assert!(
            after.size.height > settled.size.height,
            "the settled trace grows in place instead of scrolling"
        );
    }

    #[gpui::test]
    fn reduced_motion_still_lands_on_the_tail(cx: &mut TestAppContext) {
        cx.update(crate::init);
        cx.update(|cx| cx.set_reduce_motion(true));
        let (probe, cx) = cx.add_window_view(|_, _| TraceProbe::running());
        let cx: &mut VisualTestContext = cx;
        draw(cx);

        assert_pinned_to_tail(cx, "a reduced-motion preview opens at its resting frame");
        append_step(&probe, cx);
        assert_pinned_to_tail(cx, "reduced motion settles on the tail without animating");
    }

    #[gpui::test]
    fn a_constrained_live_preview_keeps_every_step_reachable(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (_, cx) = cx.add_window_view(|_, _| TraceProbe::running());
        let cx: &mut VisualTestContext = cx;
        draw(cx);

        assert_pinned_to_tail(cx, "the end of the trace is on screen while following");
        scroll_up(cx, 4000.);
        assert!(
            scroll_offset(cx).as_f32().abs() < 1.0,
            "scrolling back must reach the first step"
        );
    }

    #[test]
    fn a_preview_with_nothing_to_scroll_follows_the_tail() {
        assert!(follows_tail(Pixels::ZERO, Pixels::ZERO, px(20.)));
    }

    #[test]
    fn following_tolerates_stopping_just_short_of_the_bottom() {
        assert!(follows_tail(px(-200.), px(200.), px(20.)), "at the tail");
        assert!(
            follows_tail(px(-190.), px(200.), px(20.)),
            "ten short of it"
        );
        assert!(
            !follows_tail(px(-160.), px(200.), px(20.)),
            "forty short of it is deliberate"
        );
        assert!(
            !follows_tail(Pixels::ZERO, px(200.), px(20.)),
            "the top of the trace is not the tail"
        );
    }

    #[test]
    fn a_live_preview_follows_only_content_it_has_not_shown() {
        let slack = px(20.);
        let mut preview = LivePreview::new();
        assert!(
            preview.observe(7, slack),
            "the first content a preview shows lands on the tail"
        );
        assert!(
            !preview.observe(7, slack),
            "a re-render of the same content never scrolls"
        );
        assert!(preview.observe(8, slack), "appended reasoning follows");
    }

    #[test]
    fn the_content_digest_sees_growth_that_the_snapshot_revision_cannot() {
        let trace = ThinkingTrace::new().steps([ThinkingStep::new("Reading the schema")]);
        let appended = ThinkingTrace::new().steps([
            ThinkingStep::new("Reading the schema"),
            ThinkingStep::new("Comparing unit prices"),
        ]);
        let finished = ThinkingTrace::new()
            .steps([ThinkingStep::new("Reading the schema").status(StepStatus::Done)]);

        assert_eq!(
            content_revision(0, &trace),
            content_revision(0, &trace.clone())
        );
        assert_ne!(content_revision(0, &trace), content_revision(0, &appended));
        assert_ne!(content_revision(0, &trace), content_revision(0, &finished));
        assert_ne!(
            content_revision(0, &trace),
            content_revision(0, &trace.clone().prose("Considering the schema"))
        );
    }

    #[test]
    fn trace_maps_the_shared_lifecycle() {
        let trace = ThinkingTrace::new().prose("Considering the schema");
        for (progress, state) in [
            (Progressive::pending(trace.clone()), ProgressState::Pending),
            (Progressive::running(trace.clone()), ProgressState::Running),
            (
                Progressive::complete(trace.clone()),
                ProgressState::Complete,
            ),
            (
                Progressive::failed(trace.clone(), "reasoning interrupted"),
                ProgressState::Failed("reasoning interrupted".into()),
            ),
        ] {
            let thinking = Thinking::new("trace", &progress);
            assert_eq!(thinking.state, state);
            assert_eq!(
                thinking.trace.prose.as_deref(),
                Some("Considering the schema")
            );
        }
    }

    #[test]
    fn automatic_policy_opens_while_running_and_collapses_when_settled() {
        let trace = ThinkingTrace::new();
        assert!(Thinking::new("t", &Progressive::running(trace.clone())).is_open());
        assert!(!Thinking::new("t", &Progressive::complete(trace.clone())).is_open());
        assert!(!Thinking::new("t", &Progressive::pending(trace.clone())).is_open());
        assert!(!Thinking::new("t", &Progressive::failed(trace.clone(), "stopped")).is_open());
    }

    #[test]
    fn explicit_open_overrides_the_automatic_policy() {
        let trace = ThinkingTrace::new();
        assert!(
            !Thinking::new("t", &Progressive::running(trace.clone()))
                .open(false)
                .is_open()
        );
        assert!(
            Thinking::new("t", &Progressive::complete(trace))
                .open(true)
                .is_open()
        );
    }
}
