//! Human-in-the-loop approval gates for agent actions.

use crate::ButtonLabelExt as _;
use crate::cues::{self, Cue};
use crate::handlers::SharedHandler;
use crate::motion::acknowledged_state;
use crate::status::{StatusBadge, StatusTone};
use crate::surface::{card, description, inset, meta, title};
use gpui::{
    AnyElement, App, ClickEvent, ElementId, InteractiveElement as _, IntoElement, ParentElement,
    RenderOnce, Role, SharedString, StatefulInteractiveElement as _, StyleRefinement, Styled,
    Window, div, prelude::FluentBuilder as _,
};
use gpui_component::{
    ActiveTheme as _, Sizable as _, StyledExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
};
use std::rc::Rc;

/// Whether the gate is still open or how it was closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ApprovalDecision {
    /// Waiting for the human.
    #[default]
    Pending,
    /// The action was approved.
    Approved,
    /// The action was rejected.
    Rejected,
}

impl ApprovalDecision {
    /// The badge label for a closed gate.
    pub fn label(self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::Approved => "Approved",
            Self::Rejected => "Rejected",
        }
    }
}

/// How consequential the proposed action is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ApprovalTone {
    /// Reversible or routine; a warning border says "decide".
    #[default]
    Default,
    /// Irreversible; a danger border and a danger approve button.
    Destructive,
}

/// A decision gate: the agent proposes an action, the human approves or
/// rejects it.
///
/// The card body is a children slot, so any payload — a diff, a table, plain
/// text — can be placed inside via [`ParentElement`] methods.
///
/// # Example
///
/// ```
/// # use gpui_ai::prelude::*;
/// ApprovalCard::new("gate-1", "Send order confirmation to 3 suppliers?")
///     .description("Emails will go out immediately and cannot be recalled.")
///     .on_event(|event, _, _| match event {
///         ApprovalEvent::Approved { .. } => { /* proceed */ }
///         ApprovalEvent::ApprovedAlways { .. } => { /* remember approval and proceed */ }
///         ApprovalEvent::Rejected { .. } => { /* cancel */ }
///     });
/// ```
#[derive(IntoElement)]
pub struct ApprovalCard {
    id: SharedString,
    style: StyleRefinement,
    title: SharedString,
    description: Option<SharedString>,
    approve_label: SharedString,
    reject_label: SharedString,
    children: Vec<AnyElement>,
    decision: ApprovalDecision,
    tone: ApprovalTone,
    allow_always: bool,
    note: Option<SharedString>,
    on_event: Option<SharedHandler<ApprovalEvent>>,
}

impl ApprovalCard {
    /// Creates a gate with the question being asked of the human.
    pub fn new(id: impl Into<SharedString>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            style: StyleRefinement::default(),
            title: title.into(),
            description: None,
            approve_label: "Approve".into(),
            reject_label: "Reject".into(),
            children: Vec::new(),
            decision: ApprovalDecision::Pending,
            tone: ApprovalTone::Default,
            allow_always: false,
            note: None,
            on_event: None,
        }
    }

    /// Closes the gate: resolved cards show the decision instead of buttons.
    pub fn decision(mut self, decision: ApprovalDecision) -> Self {
        self.decision = decision;
        self
    }

    /// Marks the action as irreversible (danger border and approve button).
    pub fn tone(mut self, tone: ApprovalTone) -> Self {
        self.tone = tone;
        self
    }

    /// Offers "Always allow", reported as [`ApprovalEvent::ApprovedAlways`].
    pub fn allow_always(mut self, allow_always: bool) -> Self {
        self.allow_always = allow_always;
        self
    }

    /// Adds a footnote shown once the gate is closed ("Approved by Oscar").
    pub fn note(mut self, note: impl Into<SharedString>) -> Self {
        self.note = Some(note.into());
        self
    }

    /// Sets supporting detail — consequences, scope, reversibility.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Overrides the approve button label (default "Approve").
    pub fn approve_label(mut self, label: impl Into<SharedString>) -> Self {
        self.approve_label = label.into();
        self
    }

    /// Overrides the reject button label (default "Reject").
    pub fn reject_label(mut self, label: impl Into<SharedString>) -> Self {
        self.reject_label = label.into();
        self
    }

    /// Handles typed approval decisions.
    pub fn on_event(
        mut self,
        handler: impl Fn(&ApprovalEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_event = Some(Rc::new(handler));
        self
    }
}

impl ParentElement for ApprovalCard {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for ApprovalCard {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for ApprovalCard {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = cx.theme().semantic_tokens();
        let has_payload = !self.children.is_empty();
        let handler = self.on_event;
        let approve_event = ApprovalEvent::Approved {
            id: self.id.clone(),
        };
        let reject_event = ApprovalEvent::Rejected {
            id: self.id.clone(),
        };
        let always_event = ApprovalEvent::ApprovedAlways {
            id: self.id.clone(),
        };
        let accessibility_label = self.title.clone();
        let accessibility_description = self.description.clone();
        let root_id = ElementId::from(self.id.clone());
        let debug_id = self.id.to_string();
        let decided = self.decision != ApprovalDecision::Pending;
        let border = match (self.decision, self.tone) {
            (ApprovalDecision::Pending, ApprovalTone::Default) => cx.theme().warning,
            (ApprovalDecision::Pending, ApprovalTone::Destructive) => cx.theme().danger,
            _ => cx.theme().border,
        };
        let destructive = self.tone == ApprovalTone::Destructive;
        let approve_debug = debug_id.clone();
        let reject_debug = debug_id.clone();
        let always_debug = debug_id.clone();
        let decision_debug = debug_id.clone();
        let settled = acknowledged_state(
            ElementId::from((root_id.clone(), "resolved")),
            self.decision as u64,
            window,
            cx,
        );
        let footer: AnyElement = if decided {
            // The event fired the moment the button was pressed; this is
            // acknowledgment, staged after the controlled state. It plays
            // once per decision — a card that mounts already decided shows
            // its resolved text without motion, and rapid decision changes
            // retarget by playing the new state's own acknowledgment.
            h_flex()
                .items_center()
                .gap(tokens.spacing.sm)
                .opacity(settled)
                .top(tokens.spacing.xxs * (1.0 - settled) * crate::motion::travel(cx))
                .child(
                    div()
                        .debug_selector(move || format!("approval-decision-{decision_debug}"))
                        .child(
                            StatusBadge::new((root_id.clone(), "decision"), self.decision.label())
                                .tone(match self.decision {
                                    ApprovalDecision::Approved => StatusTone::Success,
                                    _ => StatusTone::Neutral,
                                }),
                        ),
                )
                .when_some(self.note.clone(), |this, note| this.child(meta(note, cx)))
                .into_any_element()
        } else {
            h_flex()
                .flex_wrap()
                .items_center()
                .gap(tokens.spacing.sm)
                .when_some(handler.clone(), |this, handler| {
                    this.child(
                        div()
                            .debug_selector(move || format!("approval-approve-{approve_debug}"))
                            .child(
                                Button::new(format!("{}-approve", self.id))
                                    .map(|button| {
                                        if destructive {
                                            button.danger()
                                        } else {
                                            button.primary()
                                        }
                                    })
                                    .small()
                                    .accessibility_id(format!("{}-approve", self.id))
                                    .text_label(self.approve_label.clone())
                                    .on_click(move |_: &ClickEvent, window, cx| {
                                        cues::emit(cx, Cue::Decided { approved: true });
                                        handler(&approve_event, window, cx)
                                    }),
                            ),
                    )
                })
                .when_some(handler.clone(), |this, handler| {
                    this.child(
                        div()
                            .debug_selector(move || format!("approval-reject-{reject_debug}"))
                            .child(
                                Button::new(format!("{}-reject", self.id))
                                    .outline()
                                    .small()
                                    .accessibility_id(format!("{}-reject", self.id))
                                    .text_label(self.reject_label.clone())
                                    .on_click(move |_: &ClickEvent, window, cx| {
                                        cues::emit(cx, Cue::Decided { approved: false });
                                        handler(&reject_event, window, cx)
                                    }),
                            ),
                    )
                })
                .when_some(handler.filter(|_| self.allow_always), |this, handler| {
                    this.child(
                        div()
                            .debug_selector(move || format!("approval-always-{always_debug}"))
                            .child(
                                Button::new(format!("{}-always", self.id))
                                    .ghost()
                                    .small()
                                    .accessibility_id(format!("{}-always", self.id))
                                    .text_label("Always allow")
                                    .on_click(move |_: &ClickEvent, window, cx| {
                                        cues::emit(cx, Cue::Decided { approved: true });
                                        handler(&always_event, window, cx)
                                    }),
                            ),
                    )
                })
                .into_any_element()
        };

        // A decision gate is the one card that earns a semantic accent: the
        // warning border says "stop and decide" before any text is read.
        card(self.id.clone(), cx)
            .role(Role::Group)
            .aria_label(accessibility_label)
            .when_some(accessibility_description, |this, description| {
                this.aria_description(description)
            })
            .border_color(border)
            .when(decided, |this| this.bg(cx.theme().muted.opacity(0.25)))
            .child(title(self.title, cx))
            .when_some(self.description, |this, text| {
                this.child(description(text, cx))
            })
            .when(has_payload, |this| {
                this.child(
                    inset(cx)
                        .flex()
                        .flex_col()
                        .gap(tokens.spacing.xs)
                        .children(self.children),
                )
            })
            .child(footer)
            .refine_style(&self.style)
    }
}
/// An interaction emitted by [`ApprovalCard`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalEvent {
    /// The proposed action was approved.
    Approved {
        /// Stable gate identifier.
        id: SharedString,
    },
    /// The proposed action was rejected.
    Rejected {
        /// Stable gate identifier.
        id: SharedString,
    },
    /// The action was approved for this and every later occurrence.
    ApprovedAlways {
        /// Stable gate identifier.
        id: SharedString,
    },
}
