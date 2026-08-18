//! Streamed markdown answers with sources and follow-up suggestions.

use crate::handlers::SharedHandler;
use crate::stream::{ProgressState, StreamedContent};
use crate::theme::SemanticStyledExt as _;
use gpui::{
    App, ClickEvent, ElementId, InteractiveElement as _, IntoElement, ParentElement as _,
    RenderOnce, SharedString, StyleRefinement, Styled, Window, div, prelude::FluentBuilder as _,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable as _, StyledExt as _, button::Button, h_flex,
    text::TextView, v_flex,
};
use std::rc::Rc;

/// A source backing a streamed answer, shown as a chip under the text.
#[derive(Debug, Clone)]
pub struct SourceRef {
    title: SharedString,
}

/// A stable follow-up suggestion shown after an answer settles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FollowUp {
    id: SharedString,
    label: SharedString,
}

impl FollowUp {
    /// Creates a follow-up with a stable application-level identifier.
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
        }
    }

    fn selected_event(&self) -> StreamingTextEvent {
        StreamingTextEvent::FollowUpSelected {
            id: self.id.clone(),
        }
    }
}

/// An interaction emitted by [`StreamingText`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamingTextEvent {
    /// A follow-up suggestion was selected.
    FollowUpSelected {
        /// Stable follow-up identifier.
        id: SharedString,
    },
}

impl SourceRef {
    /// Creates a source chip with a display title.
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
        }
    }
}

/// A streamed markdown answer: live text, then sources and follow-ups.
///
/// The component renders a [`StreamedContent`] snapshot — the application
/// owns the content and re-renders as chunks arrive. While streaming, a
/// cursor glyph marks the end of the text; on failure the reason is shown
/// under the answer.
///
/// # Example
///
/// ```ignore
/// StreamingText::new("answer", &self.answer)
///     .sources(["pricing.md", "suppliers.csv"])
///     .follow_ups([
///         FollowUp::new("delivery", "Compare delivery times"),
///         FollowUp::new("history", "Show price history"),
///     ])
///     .on_event(cx.listener(|this, event: &StreamingTextEvent, _, cx| { /* ask it */ }))
/// ```
#[derive(IntoElement)]
pub struct StreamingText {
    id: ElementId,
    style: StyleRefinement,
    text: SharedString,
    state: ProgressState,
    sources: Vec<SourceRef>,
    follow_ups: Vec<FollowUp>,
    on_event: Option<SharedHandler<StreamingTextEvent>>,
}

impl StreamingText {
    /// Creates the component from a snapshot of streamed content.
    pub fn new(id: impl Into<ElementId>, content: &StreamedContent) -> Self {
        Self {
            id: id.into(),
            style: StyleRefinement::default(),
            text: SharedString::from(content.text().to_string()),
            state: content.state().clone(),
            sources: Vec::new(),
            follow_ups: Vec::new(),
            on_event: None,
        }
    }

    /// Adds source chips shown once streaming has finished.
    pub fn sources(mut self, sources: impl IntoIterator<Item = impl Into<SharedString>>) -> Self {
        self.sources = sources.into_iter().map(SourceRef::new).collect();
        self
    }

    /// Adds pre-built [`SourceRef`]s (equivalent to [`Self::sources`] today;
    /// kept separate so richer source metadata can grow here).
    pub fn source_refs(mut self, sources: impl IntoIterator<Item = SourceRef>) -> Self {
        self.sources = sources.into_iter().collect();
        self
    }

    /// Adds follow-up suggestions shown once streaming has finished.
    pub fn follow_ups(mut self, follow_ups: impl IntoIterator<Item = FollowUp>) -> Self {
        self.follow_ups = follow_ups.into_iter().collect();
        self
    }

    /// Handles typed answer interactions.
    pub fn on_event(
        mut self,
        handler: impl Fn(&StreamingTextEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_event = Some(Rc::new(handler));
        self
    }
}

impl Styled for StreamingText {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for StreamingText {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = cx.theme().semantic_tokens();
        let streaming = self.state == ProgressState::Running;
        let settled = self.state == ProgressState::Complete;
        let source = if streaming {
            format!("{}▌", self.text)
        } else {
            self.text.to_string()
        };

        v_flex()
            .id(self.id)
            .gap(tokens.spacing.md)
            .child(
                div()
                    .text_token(tokens.typography.sm)
                    .text_color(cx.theme().foreground)
                    .child(TextView::markdown("answer", source)),
            )
            .when_some(
                match self.state {
                    ProgressState::Failed(reason) => Some(reason),
                    _ => None,
                },
                |this, reason| {
                    this.child(
                        h_flex()
                            .items_center()
                            .gap(tokens.spacing.xs)
                            .text_token(tokens.typography.xs)
                            .text_color(cx.theme().danger)
                            .child(Icon::new(IconName::CircleX).xsmall())
                            .child(reason),
                    )
                },
            )
            .when(settled && !self.sources.is_empty(), |this| {
                this.child(h_flex().flex_wrap().gap(tokens.spacing.xs).children(
                    self.sources.into_iter().map(|source| {
                        h_flex()
                            .items_center()
                            .gap(tokens.spacing.xs)
                            .px(tokens.spacing.sm)
                            .py(tokens.spacing.xxs)
                            .text_token(tokens.typography.xs)
                            .text_color(cx.theme().muted_foreground)
                            .bg(cx.theme().secondary)
                            .border_1()
                            .border_color(cx.theme().border)
                            .rounded(tokens.radius.full)
                            .child(Icon::new(IconName::File).xsmall())
                            .child(source.title)
                    }),
                ))
            })
            .when(settled && !self.follow_ups.is_empty(), |this| {
                let handler = self.on_event.clone();
                this.child(h_flex().flex_wrap().gap(tokens.spacing.xs).children(
                    self.follow_ups.into_iter().map(|follow_up| {
                        let event = follow_up.selected_event();
                        Button::new(follow_up.id.clone())
                            .outline()
                            .small()
                            .accessibility_id(format!("follow-up-{}", follow_up.id))
                            .label(follow_up.label)
                            .when_some(handler.clone(), |this, handler| {
                                this.on_click(move |_: &ClickEvent, window, cx| {
                                    handler(&event, window, cx)
                                })
                            })
                    }),
                ))
            })
            .refine_style(&self.style)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stream::Progressive;

    #[test]
    fn maps_all_shared_lifecycle_states() {
        let cases = [
            Progressive::pending("answer".to_owned()),
            Progressive::running("answer".to_owned()),
            Progressive::complete("answer".to_owned()),
            Progressive::failed("answer".to_owned(), "offline"),
        ];
        for content in cases {
            assert_eq!(
                StreamingText::new("answer", &content).state,
                *content.state()
            );
        }
    }

    #[test]
    fn duplicate_follow_up_labels_emit_distinct_stable_ids() {
        let first = FollowUp::new("first", "Try again");
        let second = FollowUp::new("second", "Try again");
        assert_eq!(
            first.selected_event(),
            StreamingTextEvent::FollowUpSelected { id: "first".into() }
        );
        assert_eq!(
            second.selected_event(),
            StreamingTextEvent::FollowUpSelected {
                id: "second".into()
            }
        );
    }
}
