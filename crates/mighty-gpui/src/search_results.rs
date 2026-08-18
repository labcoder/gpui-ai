//! Web-search tool output: a query and its results with sources.

use crate::handlers::SharedHandler;
use crate::theme::SemanticStyledExt as _;
use gpui::{
    App, ClickEvent, ElementId, InteractiveElement as _, IntoElement, ParentElement as _,
    RenderOnce, SharedString, StyleRefinement, Styled, Window, div, prelude::FluentBuilder as _,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable as _, StyledExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    spinner::Spinner,
    v_flex,
};

/// One search hit inside [`SearchResults`].
#[derive(Debug, Clone)]
pub struct SearchResult {
    id: SharedString,
    title: SharedString,
    domain: Option<SharedString>,
}

impl SearchResult {
    /// Creates a result with its display title.
    pub fn new(id: impl Into<SharedString>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            domain: None,
        }
    }

    /// Sets the source domain shown after the title (`example.com`).
    pub fn domain(mut self, domain: impl Into<SharedString>) -> Self {
        self.domain = Some(domain.into());
        self
    }

    fn opened_event(&self) -> SearchResultsEvent {
        SearchResultsEvent::Opened {
            id: self.id.clone(),
        }
    }
}

/// An interaction emitted by [`SearchResults`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchResultsEvent {
    /// A result was selected.
    Opened {
        /// Stable result identifier.
        id: SharedString,
    },
}

/// The output of a web-search tool call: the query, a live "searching"
/// state, and the results once they arrive.
///
/// # Example
///
/// ```ignore
/// SearchResults::new("search-1", "gpui wasm support")
///     .results([
///         SearchResult::new("gallery", "GPUI Component — Web Gallery")
///             .domain("longbridge.github.io"),
///         SearchResult::new("gpui-web", "zed-industries/zed: crates/gpui_web")
///             .domain("github.com"),
///     ])
///     .on_event(cx.listener(|this, event: &SearchResultsEvent, _, cx| { /* open result */ }))
/// ```
#[derive(IntoElement)]
pub struct SearchResults {
    id: ElementId,
    style: StyleRefinement,
    query: SharedString,
    searching: bool,
    results: Vec<SearchResult>,
    on_event: Option<SharedHandler<SearchResultsEvent>>,
}

impl SearchResults {
    /// Creates the component for a search `query`.
    pub fn new(id: impl Into<ElementId>, query: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            style: StyleRefinement::default(),
            query: query.into(),
            searching: false,
            results: Vec::new(),
            on_event: None,
        }
    }

    /// Marks the search as still running; the header shows a spinner.
    pub fn searching(mut self, searching: bool) -> Self {
        self.searching = searching;
        self
    }

    /// Sets the results.
    pub fn results(mut self, results: impl IntoIterator<Item = SearchResult>) -> Self {
        self.results = results.into_iter().collect();
        self
    }

    /// Handles typed result interactions.
    pub fn on_event(
        mut self,
        handler: impl Fn(&SearchResultsEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_event = Some(std::rc::Rc::new(handler));
        self
    }
}

impl Styled for SearchResults {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for SearchResults {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = cx.theme().semantic_tokens();
        let header: SharedString = if self.searching {
            format!("Searching \u{201c}{}\u{201d}\u{2026}", self.query).into()
        } else {
            format!(
                "{} result{} for \u{201c}{}\u{201d}",
                self.results.len(),
                if self.results.len() == 1 { "" } else { "s" },
                self.query
            )
            .into()
        };
        let handler = self.on_event;

        v_flex()
            .id(self.id)
            .bg(cx.theme().background)
            .border_1()
            .border_color(cx.theme().border)
            .rounded(tokens.radius.md)
            .overflow_hidden()
            .child(
                h_flex()
                    .items_center()
                    .gap(tokens.spacing.xs)
                    .px(tokens.spacing.md)
                    .py(tokens.spacing.xs)
                    .text_token(tokens.typography.xs)
                    .text_color(cx.theme().muted_foreground)
                    .child(if self.searching {
                        Spinner::new()
                            .xsmall()
                            .color(cx.theme().muted_foreground)
                            .into_any_element()
                    } else {
                        Icon::new(IconName::Search)
                            .xsmall()
                            .text_color(cx.theme().muted_foreground)
                            .into_any_element()
                    })
                    .child(header),
            )
            .when(!self.results.is_empty(), |this| {
                this.child(
                    v_flex()
                        .border_t_1()
                        .border_color(cx.theme().border)
                        .py(tokens.spacing.xs)
                        .children(self.results.into_iter().map(|result| {
                            let event = result.opened_event();
                            let row = h_flex()
                                .w_full()
                                .items_center()
                                .gap(tokens.spacing.sm)
                                .child(
                                    Icon::new(IconName::Globe)
                                        .xsmall()
                                        .text_color(cx.theme().muted_foreground),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .truncate()
                                        .text_token(tokens.typography.sm)
                                        .text_color(cx.theme().foreground)
                                        .child(result.title),
                                )
                                .when_some(result.domain, |this, domain| {
                                    this.child(
                                        div()
                                            .flex_none()
                                            .text_token(tokens.typography.xs)
                                            .font_family(cx.theme().mono_font_family.clone())
                                            .text_color(cx.theme().muted_foreground)
                                            .child(domain),
                                    )
                                });

                            match handler.clone() {
                                Some(handler) => Button::new(result.id.clone())
                                    .ghost()
                                    .compact()
                                    .w_full()
                                    .accessibility_id(format!("search-result-{}", result.id))
                                    .child(row)
                                    .on_click(move |_: &ClickEvent, window, cx| {
                                        handler(&event, window, cx)
                                    })
                                    .into_any_element(),
                                None => div()
                                    .w_full()
                                    .px(tokens.spacing.md)
                                    .py(tokens.spacing.xs)
                                    .child(row)
                                    .into_any_element(),
                            }
                        })),
                )
            })
            .refine_style(&self.style)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_titles_emit_distinct_stable_ids() {
        let first = SearchResult::new("first", "Same title");
        let second = SearchResult::new("second", "Same title");
        assert_eq!(
            first.opened_event(),
            SearchResultsEvent::Opened { id: "first".into() }
        );
        assert_eq!(
            second.opened_event(),
            SearchResultsEvent::Opened {
                id: "second".into()
            }
        );
    }
}
