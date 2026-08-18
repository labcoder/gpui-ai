//! Cards for retrieved knowledge chunks with source attribution.

use crate::handlers::SharedHandler;
use crate::theme::SemanticStyledExt as _;
use gpui::{
    App, ClickEvent, IntoElement, ParentElement as _, RenderOnce, SharedString, StyleRefinement,
    Styled, Window, div, prelude::FluentBuilder as _,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable as _, StyledExt as _,
    button::{Button, ButtonVariants as _},
    h_flex, v_flex,
};
use std::rc::Rc;

/// A retrieved knowledge chunk: source, snippet, and optional relevance.
///
/// # Example
///
/// ```ignore
/// ContextCard::new("ctx-1", "pricing.md")
///     .snippet("Enterprise plans include SSO and a dedicated…")
///     .relevance(0.92)
///     .on_event(|event, _, _| { /* open the stable event id */ })
/// ```
#[derive(IntoElement)]
pub struct ContextCard {
    id: SharedString,
    style: StyleRefinement,
    source: SharedString,
    snippet: Option<SharedString>,
    relevance: Option<f32>,
    on_event: Option<SharedHandler<ContextCardEvent>>,
}

impl ContextCard {
    /// Creates a card for a chunk retrieved from `source` (a document name,
    /// URL, or collection).
    pub fn new(id: impl Into<SharedString>, source: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            style: StyleRefinement::default(),
            source: source.into(),
            snippet: None,
            relevance: None,
            on_event: None,
        }
    }

    /// Sets the retrieved text snippet.
    pub fn snippet(mut self, snippet: impl Into<SharedString>) -> Self {
        self.snippet = Some(snippet.into());
        self
    }

    /// Sets a relevance score in `0.0..=1.0`, shown as a percentage.
    /// Values are clamped.
    pub fn relevance(mut self, relevance: f32) -> Self {
        self.relevance = Some(relevance.clamp(0.0, 1.0));
        self
    }

    /// Makes the card clickable to open the underlying source.
    pub fn on_event(
        mut self,
        handler: impl Fn(&ContextCardEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_event = Some(Rc::new(handler));
        self
    }
}

impl Styled for ContextCard {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for ContextCard {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = cx.theme().semantic_tokens();
        let clickable = self.on_event.is_some();
        let event = ContextCardEvent::Opened {
            id: self.id.clone(),
        };

        let content = v_flex()
            .gap(tokens.spacing.xs)
            .p(tokens.spacing.md)
            .bg(cx.theme().background)
            .border_1()
            .border_color(cx.theme().border)
            .rounded(tokens.radius.md)
            .child(
                h_flex()
                    .items_center()
                    .gap(tokens.spacing.xs)
                    .child(
                        Icon::new(IconName::File)
                            .xsmall()
                            .text_color(cx.theme().muted_foreground),
                    )
                    .child(
                        div()
                            .flex_1()
                            .truncate()
                            .text_token(tokens.typography.xs)
                            .font_family(cx.theme().mono_font_family.clone())
                            .text_color(cx.theme().muted_foreground)
                            .child(self.source),
                    )
                    .when_some(self.relevance, |this, relevance| {
                        this.child(
                            div()
                                .flex_none()
                                .text_token(tokens.typography.xs)
                                .text_color(cx.theme().muted_foreground)
                                .child(format!("{:.0}%", relevance * 100.0)),
                        )
                    })
                    .when(clickable, |this| {
                        this.child(
                            Icon::new(IconName::ExternalLink)
                                .xsmall()
                                .text_color(cx.theme().muted_foreground),
                        )
                    }),
            )
            .when_some(self.snippet, |this, snippet| {
                this.child(
                    div()
                        .text_token(tokens.typography.sm)
                        .text_color(cx.theme().foreground)
                        .line_clamp(3)
                        .child(snippet),
                )
            })
            .refine_style(&self.style);

        if let Some(handler) = self.on_event {
            Button::new(self.id.clone())
                .ghost()
                .compact()
                .w_full()
                .accessibility_id(format!("context-card-{}", self.id))
                .child(content)
                .on_click(move |_: &ClickEvent, window, cx| handler(&event, window, cx))
                .into_any_element()
        } else {
            content.into_any_element()
        }
    }
}
/// An interaction emitted by [`ContextCard`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextCardEvent {
    /// The underlying source was selected.
    Opened {
        /// Stable context identifier.
        id: SharedString,
    },
}
