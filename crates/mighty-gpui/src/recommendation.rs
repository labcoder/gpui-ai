//! Agent suggestion cards with confidence and alternatives.

use crate::handlers::Handler;
use crate::theme::SemanticStyledExt as _;
use gpui::{
    App, ClickEvent, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    RenderOnce, Role, SharedString, StatefulInteractiveElement as _, StyleRefinement, Styled,
    Window, div, prelude::FluentBuilder as _, relative,
};

/// An interaction emitted by [`RecommendationCard`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecommendationEvent {
    /// The recommendation was accepted.
    Accepted {
        /// Stable recommendation identifier.
        id: SharedString,
    },
}
use gpui_component::{
    ActiveTheme as _, Sizable as _, StyledExt as _,
    button::{Button, ButtonVariants as _},
    h_flex, v_flex,
};

/// An agent suggestion: what it proposes, how confident it is, and what else
/// it considered.
///
/// # Example
///
/// ```ignore
/// RecommendationCard::new("rec-1", "Switch supplier to Alpenrose Dairy")
///     .description("Lower unit cost at equal volume; delivery risk unchanged.")
///     .confidence(0.87)
///     .alternatives(["Keep current supplier", "Split volume 60/40"])
///     .on_event(|event, _, _| { /* apply the stable event id */ })
/// ```
#[derive(IntoElement)]
pub struct RecommendationCard {
    id: SharedString,
    style: StyleRefinement,
    title: SharedString,
    description: Option<SharedString>,
    confidence: Option<f32>,
    alternatives: Vec<SharedString>,
    accept_label: SharedString,
    on_event: Option<Handler<RecommendationEvent>>,
}

impl RecommendationCard {
    /// Creates a card with the recommendation headline.
    pub fn new(id: impl Into<SharedString>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            style: StyleRefinement::default(),
            title: title.into(),
            description: None,
            confidence: None,
            alternatives: Vec::new(),
            accept_label: "Apply".into(),
            on_event: None,
        }
    }

    /// Sets the supporting explanation.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets the confidence in `0.0..=1.0`, rendered as a meter. Values are
    /// clamped.
    pub fn confidence(mut self, confidence: f32) -> Self {
        self.confidence = Some(confidence.clamp(0.0, 1.0));
        self
    }

    /// Lists the alternatives the agent considered but ranked lower.
    pub fn alternatives(
        mut self,
        alternatives: impl IntoIterator<Item = impl Into<SharedString>>,
    ) -> Self {
        self.alternatives = alternatives.into_iter().map(Into::into).collect();
        self
    }

    /// Adds an accept button. Default label is "Apply"; override with
    /// [`Self::accept_label`].
    pub fn on_event(
        mut self,
        handler: impl Fn(&RecommendationEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_event = Some(Box::new(handler));
        self
    }

    /// Overrides the accept button label.
    pub fn accept_label(mut self, label: impl Into<SharedString>) -> Self {
        self.accept_label = label.into();
        self
    }
}

impl Styled for RecommendationCard {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for RecommendationCard {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = cx.theme().semantic_tokens();
        let event = RecommendationEvent::Accepted {
            id: self.id.clone(),
        };
        let accessibility_label = self.title.clone();
        let accessibility_description = self.description.clone();
        v_flex()
            .id(self.id.clone())
            .role(Role::Group)
            .aria_label(accessibility_label)
            .when_some(accessibility_description, |this, description| {
                this.aria_description(description)
            })
            .gap(tokens.spacing.md)
            .p(tokens.spacing.lg)
            .bg(cx.theme().background)
            .border_1()
            .border_color(cx.theme().border)
            .rounded(tokens.radius.md)
            .child(
                div()
                    .text_token(tokens.typography.sm)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(cx.theme().foreground)
                    .child(self.title),
            )
            .when_some(self.description, |this, description| {
                this.child(
                    div()
                        .text_token(tokens.typography.sm)
                        .text_color(cx.theme().muted_foreground)
                        .child(description),
                )
            })
            .when_some(self.confidence, |this, confidence| {
                let meter_color = if confidence < 0.4 {
                    cx.theme().danger
                } else if confidence < 0.7 {
                    cx.theme().warning
                } else {
                    cx.theme().success
                };
                this.child(
                    h_flex()
                        .id(format!("{}-confidence", self.id))
                        .role(Role::Meter)
                        .aria_label("Confidence")
                        .aria_min_numeric_value(0.)
                        .aria_max_numeric_value(100.)
                        .aria_numeric_value((confidence * 100.0) as f64)
                        .items_center()
                        .gap(tokens.spacing.sm)
                        .child(
                            div()
                                .flex_1()
                                .h_1p5()
                                .rounded(tokens.radius.full)
                                .bg(cx.theme().muted)
                                .child(
                                    div()
                                        .h_full()
                                        .w(relative(confidence))
                                        .rounded(tokens.radius.full)
                                        .bg(meter_color),
                                ),
                        )
                        .child(
                            div()
                                .flex_none()
                                .text_token(tokens.typography.xs)
                                .font_family(cx.theme().mono_font_family.clone())
                                .text_color(cx.theme().muted_foreground)
                                .child(format!("{:.0}%", confidence * 100.0)),
                        ),
                )
            })
            .when(!self.alternatives.is_empty(), |this| {
                this.child(
                    v_flex()
                        .id(format!("{}-alternatives", self.id))
                        .role(Role::List)
                        .aria_label("Also considered")
                        .gap(tokens.spacing.xs)
                        .child(
                            div()
                                .text_token(tokens.typography.xs)
                                .text_color(cx.theme().muted_foreground)
                                .child("Also considered"),
                        )
                        .children(self.alternatives.into_iter().enumerate().map(|(ix, alt)| {
                            let accessibility_label = alt.clone();
                            h_flex()
                                .id(format!("{}-alternative-{ix}", self.id))
                                .role(Role::ListItem)
                                .aria_label(accessibility_label)
                                .items_center()
                                .gap(tokens.spacing.xs)
                                .text_token(tokens.typography.sm)
                                .text_color(cx.theme().muted_foreground)
                                .child(
                                    div()
                                        .size_1()
                                        .rounded(tokens.radius.full)
                                        .bg(cx.theme().muted_foreground),
                                )
                                .child(alt)
                        })),
                )
            })
            .when_some(self.on_event, |this, handler| {
                this.child(
                    h_flex().child(
                        Button::new(format!("{}-accept", self.id))
                            .primary()
                            .small()
                            .accessibility_id(format!("{}-accept", self.id))
                            .label(self.accept_label)
                            .on_click(move |_: &ClickEvent, window, cx| {
                                handler(&event, window, cx)
                            }),
                    ),
                )
            })
            .refine_style(&self.style)
    }
}
