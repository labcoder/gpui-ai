//! Human-in-the-loop approval gates for agent actions.

use crate::cues::{self, Cue};
use crate::handlers::SharedHandler;
use crate::surface::{card, description, inset, title};
use gpui::{
    AnyElement, App, ClickEvent, IntoElement, ParentElement, RenderOnce, Role, SharedString,
    StatefulInteractiveElement as _, StyleRefinement, Styled, Window, prelude::FluentBuilder as _,
};
use gpui_component::{
    ActiveTheme as _, Sizable as _, StyledExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
};
use std::rc::Rc;

/// A decision gate: the agent proposes an action, the human approves or
/// rejects it.
///
/// The card body is a children slot, so any payload — a diff, a table, plain
/// text — can be placed inside via [`ParentElement`] methods.
///
/// # Example
///
/// ```ignore
/// ApprovalCard::new("gate-1", "Send order confirmation to 3 suppliers?")
///     .description("Emails will go out immediately and cannot be recalled.")
///     .on_event(|event, _, _| match event {
///         ApprovalEvent::Approved { .. } => { /* proceed */ }
///         ApprovalEvent::Rejected { .. } => { /* cancel */ }
///     })
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
            on_event: None,
        }
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
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = cx.theme().semantic_tokens();
        let has_payload = !self.children.is_empty();
        let handler = self.on_event;
        let approve_event = ApprovalEvent::Approved {
            id: self.id.clone(),
        };
        let reject_event = ApprovalEvent::Rejected {
            id: self.id.clone(),
        };
        let accessibility_label = self.title.clone();
        let accessibility_description = self.description.clone();

        // A decision gate is the one card that earns a semantic accent: the
        // warning border says "stop and decide" before any text is read.
        card(self.id.clone(), cx)
            .role(Role::Group)
            .aria_label(accessibility_label)
            .when_some(accessibility_description, |this, description| {
                this.aria_description(description)
            })
            .border_color(cx.theme().warning)
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
            .child(
                h_flex()
                    .gap(tokens.spacing.sm)
                    .when_some(handler.clone(), |this, handler| {
                        this.child(
                            Button::new(format!("{}-approve", self.id))
                                .primary()
                                .small()
                                .accessibility_id(format!("{}-approve", self.id))
                                .label(self.approve_label)
                                .on_click(move |_: &ClickEvent, window, cx| {
                                    cues::emit(cx, Cue::Decided { approved: true });
                                    handler(&approve_event, window, cx)
                                }),
                        )
                    })
                    .when_some(handler, |this, handler| {
                        this.child(
                            Button::new(format!("{}-reject", self.id))
                                .outline()
                                .small()
                                .accessibility_id(format!("{}-reject", self.id))
                                .label(self.reject_label)
                                .on_click(move |_: &ClickEvent, window, cx| {
                                    cues::emit(cx, Cue::Decided { approved: false });
                                    handler(&reject_event, window, cx)
                                }),
                        )
                    }),
            )
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
}
