//! Paged analytical insight cards with metrics and a time series.

use crate::handlers::SharedHandler;
use crate::theme::SemanticStyledExt as _;
use gpui::{
    App, ClickEvent, ElementId, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, RenderOnce, Role, SharedString, StatefulInteractiveElement as _,
    StyleRefinement, Styled, Window, div, prelude::FluentBuilder as _,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, Sizable as _, StyledExt as _,
    button::{Button, ButtonVariants as _},
    chart::LineChart,
    h_flex,
    text::TextView,
    v_flex,
};

/// The direction communicated by an insight metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsightTrend {
    /// The metric is increasing.
    Up,
    /// The metric is decreasing.
    Down,
    /// The metric is unchanged.
    Flat,
}

impl InsightTrend {
    fn accessibility_label(self) -> &'static str {
        match self {
            Self::Up => "increasing",
            Self::Down => "decreasing",
            Self::Flat => "steady",
        }
    }

    fn visible_label(self) -> &'static str {
        match self {
            Self::Up => "↑ Increasing",
            Self::Down => "↓ Decreasing",
            Self::Flat => "→ Steady",
        }
    }
}

/// A labeled value displayed in an [`InsightCard`].
#[derive(Debug, Clone)]
pub struct InsightMetric {
    id: SharedString,
    label: SharedString,
    value: SharedString,
    change: Option<SharedString>,
    trend: InsightTrend,
}

impl InsightMetric {
    /// Creates a metric with stable identity, a label, and a formatted value.
    pub fn new(
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        value: impl Into<SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            value: value.into(),
            change: None,
            trend: InsightTrend::Flat,
        }
    }

    /// Adds a textual change and its semantic direction.
    pub fn change(mut self, change: impl Into<SharedString>, trend: InsightTrend) -> Self {
        self.change = Some(change.into());
        self.trend = trend;
        self
    }

    fn accessibility_name(&self) -> SharedString {
        match &self.change {
            Some(change) => format!(
                "{}, {}, {}, {}",
                self.label,
                self.value,
                change,
                self.trend.accessibility_label()
            )
            .into(),
            None => format!("{}, {}", self.label, self.value).into(),
        }
    }
}

/// One labeled sample in an insight chart series.
#[derive(Debug, Clone)]
pub struct InsightPoint {
    label: SharedString,
    value: f64,
}

impl InsightPoint {
    /// Creates a chart point from its display label and numeric value.
    pub fn new(label: impl Into<SharedString>, value: f64) -> Self {
        Self {
            label: label.into(),
            value,
        }
    }
}

/// An interaction emitted by [`InsightCard`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsightEvent {
    /// Requests the page before the currently displayed insight.
    Previous {
        /// Stable insight identifier.
        id: SharedString,
    },
    /// Requests the page after the currently displayed insight.
    Next {
        /// Stable insight identifier.
        id: SharedString,
    },
    /// Requests a suggested follow-up prompt.
    FollowUp {
        /// Stable insight identifier.
        id: SharedString,
        /// Prompt selected by the user.
        prompt: SharedString,
    },
}

/// A caller-controlled analytical card with metrics, a chart, and paging.
///
/// The application owns the active page and data. Events preserve the stable
/// card identity so callers can update the right domain object after reorder.
///
/// # Example
///
/// ```ignore
/// InsightCard::new("demand", "Demand changed")
///     .body("Mint Chip softened while Vanilla strengthened.")
///     .page(1, 3)
///     .metrics([InsightMetric::new("mint", "Mint Chip", "$2,377.66")])
///     .series("Weekly demand", [InsightPoint::new("Mon", 18.0)])
///     .chart_summary("Weekly demand rose from 18 to 24 orders.")
///     .follow_up("Rebalance flavors")
///     .on_event(|event, _, _| { /* update application-owned state */ });
/// ```
#[derive(IntoElement)]
pub struct InsightCard {
    id: SharedString,
    style: StyleRefinement,
    title: SharedString,
    body: Option<SharedString>,
    current_page: usize,
    total_pages: usize,
    metrics: Vec<InsightMetric>,
    series_name: SharedString,
    points: Vec<InsightPoint>,
    chart_summary: Option<SharedString>,
    follow_up: Option<SharedString>,
    on_event: Option<SharedHandler<InsightEvent>>,
}

impl InsightCard {
    /// Creates an insight card with stable identity and a headline.
    pub fn new(id: impl Into<SharedString>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            style: StyleRefinement::default(),
            title: title.into(),
            body: None,
            current_page: 1,
            total_pages: 1,
            metrics: Vec::new(),
            series_name: "Series".into(),
            points: Vec::new(),
            chart_summary: None,
            follow_up: None,
            on_event: None,
        }
    }

    /// Sets selectable explanatory Markdown.
    pub fn body(mut self, body: impl Into<SharedString>) -> Self {
        self.body = Some(body.into());
        self
    }

    /// Sets the one-based page position, clamped to the available page count.
    pub fn page(mut self, current: usize, total: usize) -> Self {
        self.total_pages = total.max(1);
        self.current_page = current.clamp(1, self.total_pages);
        self
    }

    /// Replaces the summary metrics displayed by the card.
    pub fn metrics(mut self, metrics: impl IntoIterator<Item = InsightMetric>) -> Self {
        self.metrics = metrics.into_iter().collect();
        self
    }

    /// Sets the chart series label and points.
    pub fn series(
        mut self,
        name: impl Into<SharedString>,
        points: impl IntoIterator<Item = InsightPoint>,
    ) -> Self {
        self.series_name = name.into();
        self.points = points.into_iter().collect();
        self
    }

    /// Sets the nonvisual summary that explains the chart's meaning.
    pub fn chart_summary(mut self, summary: impl Into<SharedString>) -> Self {
        self.chart_summary = Some(summary.into());
        self
    }

    /// Adds a suggested follow-up prompt and its action button.
    pub fn follow_up(mut self, prompt: impl Into<SharedString>) -> Self {
        self.follow_up = Some(prompt.into());
        self
    }

    /// Handles all paging and follow-up interactions from this card.
    pub fn on_event(
        mut self,
        handler: impl Fn(&InsightEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_event = Some(std::rc::Rc::new(handler));
        self
    }

    fn page_label(&self) -> SharedString {
        format!("{} of {}", self.current_page, self.total_pages).into()
    }

    fn previous_event(&self) -> InsightEvent {
        InsightEvent::Previous {
            id: self.id.clone(),
        }
    }

    fn next_event(&self) -> InsightEvent {
        InsightEvent::Next {
            id: self.id.clone(),
        }
    }

    fn follow_up_event(&self) -> Option<InsightEvent> {
        self.follow_up.clone().map(|prompt| InsightEvent::FollowUp {
            id: self.id.clone(),
            prompt,
        })
    }
}

impl Styled for InsightCard {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for InsightCard {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = cx.theme().semantic_tokens();
        let page_label = self.page_label();
        let page_text: SharedString = format!("Insights · {page_label}").into();
        let accessibility_description: SharedString = match &self.chart_summary {
            Some(summary) => format!("{page_text}. {summary}").into(),
            None => page_text.clone(),
        };
        let previous_event = self.previous_event();
        let next_event = self.next_event();
        let follow_up_event = self.follow_up_event();
        let follow_up_prompt = self.follow_up.clone();
        let previous_disabled = self.current_page == 1;
        let next_disabled = self.current_page == self.total_pages;
        let chart_summary = self.chart_summary.clone();
        let chart_label = self.series_name.clone();
        let card_id = self.id.clone();

        v_flex()
            .id(self.id.clone())
            .role(Role::Group)
            .aria_label(self.title.clone())
            .aria_description(accessibility_description)
            .gap(tokens.spacing.md)
            .p(tokens.spacing.lg)
            .bg(cx.theme().background)
            .border_1()
            .border_color(cx.theme().border)
            .rounded(tokens.radius.md)
            .child(
                v_flex()
                    .gap(tokens.spacing.xs)
                    .child(
                        div()
                            .text_token(tokens.typography.xs)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(cx.theme().muted_foreground)
                            .child(page_text),
                    )
                    .child(
                        div()
                            .text_token(tokens.typography.lg)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(cx.theme().foreground)
                            .child(self.title),
                    ),
            )
            .when_some(self.body, |this, body| {
                this.child(
                    TextView::markdown((ElementId::from(card_id.clone()), "body"), body)
                        .selectable(true),
                )
            })
            .when(!self.metrics.is_empty(), |this| {
                this.child(
                    h_flex()
                        .id(format!("{}-metrics", self.id))
                        .role(Role::List)
                        .aria_label("Insight metrics")
                        .flex_wrap()
                        .gap(tokens.spacing.sm)
                        .children(self.metrics.into_iter().map(|metric| {
                            let accessibility_name = metric.accessibility_name();
                            let trend_color = match metric.trend {
                                InsightTrend::Up => cx.theme().success,
                                InsightTrend::Down => cx.theme().danger,
                                InsightTrend::Flat => cx.theme().muted_foreground,
                            };
                            v_flex()
                                .id((
                                    ElementId::from((ElementId::from(card_id.clone()), "metric")),
                                    metric.id.clone(),
                                ))
                                .role(Role::ListItem)
                                .aria_label(accessibility_name)
                                .flex_1()
                                .gap(tokens.spacing.xxs)
                                .p(tokens.spacing.sm)
                                .bg(cx.theme().muted)
                                .rounded(tokens.radius.sm)
                                .child(
                                    div()
                                        .text_token(tokens.typography.xs)
                                        .text_color(cx.theme().muted_foreground)
                                        .child(metric.label),
                                )
                                .child(
                                    div()
                                        .text_token(tokens.typography.md)
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(cx.theme().foreground)
                                        .child(metric.value),
                                )
                                .when_some(metric.change, |this, change| {
                                    this.child(
                                        h_flex()
                                            .gap(tokens.spacing.xs)
                                            .text_token(tokens.typography.xs)
                                            .text_color(trend_color)
                                            .child(change)
                                            .child(metric.trend.visible_label()),
                                    )
                                })
                        })),
                )
            })
            .when(!self.points.is_empty(), |this| {
                this.child(
                    v_flex()
                        .id((ElementId::from(card_id.clone()), "chart-group"))
                        .role(Role::Group)
                        .aria_label(chart_label.clone())
                        .when_some(chart_summary, |this, summary| {
                            this.aria_description(summary)
                        })
                        .h(tokens.spacing.xxl
                            + tokens.spacing.xxl
                            + tokens.spacing.xxl
                            + tokens.spacing.xxl)
                        .child(
                            LineChart::new(self.points)
                                .x(|point| point.label.clone())
                                .y(|point| point.value)
                                .natural()
                                .grid(false)
                                .x_axis(false)
                                .stroke(cx.theme().chart_2)
                                .name(chart_label)
                                .id((ElementId::from(card_id.clone()), "chart")),
                        ),
                )
            })
            .when(self.total_pages > 1 || self.follow_up.is_some(), |this| {
                this.child(
                    h_flex()
                        .flex_wrap()
                        .items_center()
                        .gap(tokens.spacing.sm)
                        .when(self.total_pages > 1, |this| {
                            this.child(
                                div().flex_1().child(
                                    h_flex()
                                        .gap(tokens.spacing.xs)
                                        .child(
                                            Button::new((
                                                ElementId::from(card_id.clone()),
                                                "previous",
                                            ))
                                            .small()
                                            .ghost()
                                            .disabled(previous_disabled || self.on_event.is_none())
                                            .accessibility_id(format!("{}-previous", card_id))
                                            .label("Previous")
                                            .when_some(self.on_event.clone(), |button, handler| {
                                                button.on_click(
                                                    move |_: &ClickEvent, window, cx| {
                                                        handler(&previous_event, window, cx)
                                                    },
                                                )
                                            }),
                                        )
                                        .child(
                                            Button::new((ElementId::from(card_id.clone()), "next"))
                                                .small()
                                                .ghost()
                                                .disabled(next_disabled || self.on_event.is_none())
                                                .accessibility_id(format!("{}-next", card_id))
                                                .label("Next")
                                                .when_some(
                                                    self.on_event.clone(),
                                                    |button, handler| {
                                                        button.on_click(
                                                            move |_: &ClickEvent, window, cx| {
                                                                handler(&next_event, window, cx)
                                                            },
                                                        )
                                                    },
                                                ),
                                        ),
                                ),
                            )
                        })
                        .when_some(
                            follow_up_prompt.zip(follow_up_event),
                            |this, (prompt, event)| {
                                this.child(
                                    Button::new((ElementId::from(card_id.clone()), "follow-up"))
                                        .small()
                                        .primary()
                                        .disabled(self.on_event.is_none())
                                        .accessibility_id(format!("{}-follow-up", card_id))
                                        .label(prompt)
                                        .when_some(self.on_event.clone(), |button, handler| {
                                            button.on_click(move |_: &ClickEvent, window, cx| {
                                                handler(&event, window, cx)
                                            })
                                        }),
                                )
                            },
                        ),
                )
            })
            .refine_style(&self.style)
    }
}

#[cfg(test)]
mod tests {
    use super::{InsightCard, InsightEvent, InsightMetric, InsightTrend};

    #[test]
    fn page_position_is_one_based_and_clamped() {
        assert_eq!(
            InsightCard::new("insight", "Demand changed")
                .page(0, 0)
                .page_label(),
            "1 of 1"
        );
        assert_eq!(
            InsightCard::new("insight", "Demand changed")
                .page(7, 3)
                .page_label(),
            "3 of 3"
        );
        assert_eq!(
            InsightCard::new("insight", "Demand changed")
                .page(2, 3)
                .page_label(),
            "2 of 3"
        );
    }

    #[test]
    fn metric_accessibility_name_never_relies_on_color() {
        let metric = InsightMetric::new("mint", "Mint Chip", "$2,377.66")
            .change("down 4.41%", InsightTrend::Down);
        assert_eq!(
            metric.accessibility_name(),
            "Mint Chip, $2,377.66, down 4.41%, decreasing"
        );
    }

    #[test]
    fn events_preserve_the_stable_insight_identity() {
        let card = InsightCard::new("insight-17", "Demand changed").follow_up("Rebalance flavors");
        assert_eq!(
            card.previous_event(),
            InsightEvent::Previous {
                id: "insight-17".into()
            }
        );
        assert_eq!(
            card.next_event(),
            InsightEvent::Next {
                id: "insight-17".into()
            }
        );
        assert_eq!(
            card.follow_up_event(),
            Some(InsightEvent::FollowUp {
                id: "insight-17".into(),
                prompt: "Rebalance flavors".into(),
            })
        );
    }
}
