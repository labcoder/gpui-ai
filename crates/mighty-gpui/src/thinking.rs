//! Expandable progressive reasoning traces.

use crate::{
    handlers::Handler,
    stream::{ProgressState, Progressive},
    theme::SemanticStyledExt as _,
};
use gpui::{
    App, ClickEvent, InteractiveElement as _, IntoElement, ParentElement as _, RenderOnce,
    SharedString, StyleRefinement, Styled, Window, div, prelude::FluentBuilder as _,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable as _, StyledExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    spinner::Spinner,
    text::TextView,
    v_flex,
};
use std::time::Duration;

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

/// An accessible, controlled reasoning disclosure.
#[derive(IntoElement)]
pub struct Thinking {
    id: SharedString,
    style: StyleRefinement,
    open: bool,
    state: ProgressState,
    trace: ThinkingTrace,
    on_event: Option<Handler<ThinkingEvent>>,
}

impl Thinking {
    /// Creates a collapsed trace from a progressive snapshot.
    pub fn new(id: impl Into<SharedString>, trace: &Progressive<ThinkingTrace>) -> Self {
        Self {
            id: id.into(),
            style: StyleRefinement::default(),
            open: false,
            state: trace.state().clone(),
            trace: trace.content().clone(),
            on_event: None,
        }
    }

    /// Sets whether the trace is expanded.
    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
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
}

impl Styled for Thinking {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Thinking {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = cx.theme().semantic_tokens();
        let live = self.state == ProgressState::Running;
        let title: SharedString = match (&self.state, self.trace.thought_for) {
            (ProgressState::Running, _) => "Thinking…".into(),
            (ProgressState::Failed(_), _) => "Thinking stopped".into(),
            (_, Some(duration)) => format!("Thought for {:.0}s", duration.as_secs_f64()).into(),
            _ => "Thoughts".into(),
        };
        let chevron = if self.open {
            IconName::ChevronDown
        } else {
            IconName::ChevronRight
        };
        let event = ThinkingEvent::Toggled {
            id: self.id.clone(),
            open: !self.open,
        };
        let failed = match &self.state {
            ProgressState::Failed(reason) => Some(reason.clone()),
            _ => None,
        };

        v_flex()
            .id(self.id.clone())
            .gap(tokens.spacing.xs)
            .child(
                Button::new(self.id.clone())
                    .ghost()
                    .compact()
                    .small()
                    .accessibility_id(format!("{}-toggle", self.id))
                    .child(
                        h_flex()
                            .items_center()
                            .gap(tokens.spacing.xs)
                            .text_color(cx.theme().muted_foreground)
                            .child(Icon::new(chevron).xsmall())
                            .child(title)
                            .when(live, |this| {
                                this.child(
                                    Spinner::new().xsmall().color(cx.theme().muted_foreground),
                                )
                            }),
                    )
                    .when_some(self.on_event, |this, handler| {
                        this.on_click(move |_: &ClickEvent, window, cx| handler(&event, window, cx))
                    }),
            )
            .when(self.open, |this| {
                this.child(
                    v_flex()
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
                            v_flex()
                                .gap(tokens.spacing.xxs)
                                .child(
                                    h_flex()
                                        .items_center()
                                        .gap(tokens.spacing.xs)
                                        .text_token(tokens.typography.sm)
                                        .text_color(cx.theme().foreground)
                                        .child(match step.status {
                                            StepStatus::Running => Spinner::new()
                                                .xsmall()
                                                .color(cx.theme().info)
                                                .into_any_element(),
                                            StepStatus::Done => div()
                                                .size_1p5()
                                                .rounded(tokens.radius.full)
                                                .bg(cx.theme().success)
                                                .into_any_element(),
                                        })
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
                        .when_some(failed, |this, reason| {
                            this.child(
                                div()
                                    .text_token(tokens.typography.sm)
                                    .text_color(cx.theme().danger)
                                    .child(reason),
                            )
                        }),
                )
            })
            .refine_style(&self.style)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
