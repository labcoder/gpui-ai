//! Paged analytical insight cards with metrics and a time series.

use crate::control::composed_button;
use crate::handlers::SharedHandler;
use crate::theme::SemanticStyledExt as _;
use gpui::{
    App, ClickEvent, Div, ElementId, InteractiveElement as _, IntoElement, ParentElement as _,
    RenderOnce, Role, SharedString, Stateful, StatefulInteractiveElement as _, StyleRefinement,
    Styled, Window, div, prelude::FluentBuilder as _,
};
use gpui_base::Button;
use gpui_component::{
    ActiveTheme as _, StyledExt as _, chart::LineChart, h_flex, text::TextView, v_flex,
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

#[derive(Clone, Copy)]
enum InsightControlStyle {
    Paging,
    FollowUp,
}

struct InsightControl {
    card_id: SharedString,
    local_id: &'static str,
    label: SharedString,
    disabled: bool,
    event: InsightEvent,
    style: InsightControlStyle,
}

impl InsightControl {
    fn styled_button(self, handler: Option<SharedHandler<InsightEvent>>, cx: &mut App) -> Button {
        let tokens = cx.theme().semantic_tokens();
        let (background, foreground, border, hover, active) = match self.style {
            InsightControlStyle::Paging => (
                cx.theme().transparent,
                cx.theme().foreground,
                cx.theme().transparent,
                cx.theme().button_hover,
                cx.theme().button_active,
            ),
            InsightControlStyle::FollowUp => (
                cx.theme().button_primary,
                cx.theme().button_primary_foreground,
                cx.theme().primary,
                cx.theme().button_primary_hover,
                cx.theme().button_primary_active,
            ),
        };
        let disabled = self.disabled;
        let label = self.label.clone();
        composed_button((ElementId::from(self.card_id), self.local_id), self.label)
            .disabled(disabled)
            .flex()
            .items_center()
            .justify_center()
            .px(tokens.spacing.sm)
            .py(tokens.spacing.xxs)
            .border_1()
            .border_color(border)
            .rounded(tokens.radius.sm)
            .bg(background)
            .text_token(tokens.typography.xs)
            .text_color(foreground)
            .when(!disabled, |button| {
                button
                    .hover(|style| style.bg(hover))
                    .active(|style| style.bg(active))
            })
            .focus(|style| style.border_color(cx.theme().ring))
            .focus_visible(|style| style.border_color(cx.theme().ring))
            .styles(|styles| {
                styles.disabled(|style| {
                    style
                        .bg(cx.theme().muted)
                        .text_color(cx.theme().muted_foreground)
                        .border_color(cx.theme().border)
                })
            })
            .child(div().child(label))
            .when_some(handler, |button, handler| {
                let event = self.event;
                button.on_click(move |_: &ClickEvent, window, cx| handler(&event, window, cx))
            })
    }
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

    fn resolved_chart_summary(&self) -> Option<SharedString> {
        self.chart_summary.clone().or_else(|| {
            if self.points.is_empty() {
                return None;
            }

            let values = self
                .points
                .iter()
                .map(|point| format!("{} {}", point.label, point.value))
                .collect::<Vec<_>>()
                .join(", ");
            Some(format!("{}: {values}.", self.series_name).into())
        })
    }

    fn previous_control(&self) -> InsightControl {
        InsightControl {
            card_id: self.id.clone(),
            local_id: "previous",
            label: "Previous".into(),
            disabled: self.current_page == 1 || self.on_event.is_none(),
            event: self.previous_event(),
            style: InsightControlStyle::Paging,
        }
    }

    fn previous_button(&self, cx: &mut App) -> Button {
        self.previous_control()
            .styled_button(self.on_event.clone(), cx)
    }

    fn next_control(&self) -> InsightControl {
        InsightControl {
            card_id: self.id.clone(),
            local_id: "next",
            label: "Next".into(),
            disabled: self.current_page == self.total_pages || self.on_event.is_none(),
            event: self.next_event(),
            style: InsightControlStyle::Paging,
        }
    }

    fn next_button(&self, cx: &mut App) -> Button {
        self.next_control().styled_button(self.on_event.clone(), cx)
    }

    fn follow_up_control(&self) -> Option<InsightControl> {
        self.follow_up_event().map(|event| InsightControl {
            card_id: self.id.clone(),
            local_id: "follow-up",
            label: self
                .follow_up
                .clone()
                .expect("a follow-up event always preserves its prompt"),
            disabled: self.on_event.is_none(),
            event,
            style: InsightControlStyle::FollowUp,
        })
    }

    fn follow_up_button(&self, cx: &mut App) -> Option<Button> {
        self.follow_up_control()
            .map(|control| control.styled_button(self.on_event.clone(), cx))
    }
}

fn metrics_group(card_id: &SharedString, cx: &mut App) -> Stateful<Div> {
    let tokens = cx.theme().semantic_tokens();
    h_flex()
        .id(format!("{card_id}-metrics"))
        .role(Role::List)
        .aria_label("Insight metrics")
        .flex_wrap()
        .gap(tokens.spacing.sm)
}

fn metric_item(card_id: &SharedString, metric: InsightMetric, cx: &mut App) -> Stateful<Div> {
    let tokens = cx.theme().semantic_tokens();
    let accessibility_name = metric.accessibility_name();
    let trend = metric.trend;
    let trend_color = match trend {
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
                    .child(trend.visible_label()),
            )
        })
}

fn chart_group(
    card_id: &SharedString,
    label: SharedString,
    summary: SharedString,
    points: Vec<InsightPoint>,
    cx: &mut App,
) -> Stateful<Div> {
    let tokens = cx.theme().semantic_tokens();
    v_flex()
        .id((ElementId::from(card_id.clone()), "chart-group"))
        .role(Role::Group)
        .aria_label(label.clone())
        .aria_description(summary)
        .h(tokens.spacing.xxl + tokens.spacing.xxl + tokens.spacing.xxl + tokens.spacing.xxl)
        .child(
            LineChart::new(points)
                .x(|point| point.label.clone())
                .y(|point| point.value)
                .natural()
                .grid(false)
                .x_axis(false)
                .stroke(cx.theme().chart_2)
                .name(label)
                .id((ElementId::from(card_id.clone()), "chart")),
        )
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
        let resolved_chart_summary = self.resolved_chart_summary();
        let accessibility_description: SharedString = match &resolved_chart_summary {
            Some(summary) => format!("{page_text}. {summary}").into(),
            None => page_text.clone(),
        };
        let previous_button = (self.total_pages > 1).then(|| self.previous_button(cx));
        let next_button = (self.total_pages > 1).then(|| self.next_button(cx));
        let follow_up_button = self.follow_up_button(cx);
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
                            .text_color(cx.theme().muted_foreground)
                            .child(page_text),
                    )
                    .child(
                        div()
                            .text_token(tokens.typography.lg)
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
                    metrics_group(&card_id, cx).children(
                        self.metrics
                            .into_iter()
                            .map(|metric| metric_item(&card_id, metric, cx)),
                    ),
                )
            })
            .when(!self.points.is_empty(), |this| {
                this.child(chart_group(
                    &card_id,
                    chart_label,
                    resolved_chart_summary
                        .expect("non-empty chart points always resolve a summary"),
                    self.points,
                    cx,
                ))
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
                                            previous_button
                                                .expect("multi-page cards have a previous button"),
                                        )
                                        .child(
                                            next_button
                                                .expect("multi-page cards have a next button"),
                                        ),
                                ),
                            )
                        })
                        .when_some(follow_up_button, |this, button| this.child(button)),
                )
            })
            .refine_style(&self.style)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        InsightCard, InsightEvent, InsightMetric, InsightPoint, InsightTrend, chart_group,
        metric_item, metrics_group,
    };
    use gpui::{
        Element as _, IntoElement as _, Render, RenderOnce as _, Role, TestAppContext, Window,
        accesskit, canvas,
    };
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Copy)]
    enum ChildProbeKind {
        Metrics,
        Metric,
        Chart,
        Previous,
        Next,
        FollowUp,
    }

    struct CapturedChild {
        role: Option<Role>,
        node: accesskit::Node,
    }

    struct ChildProbe {
        kind: ChildProbeKind,
        captured: Arc<Mutex<Option<CapturedChild>>>,
    }

    impl Render for ChildProbe {
        fn render(
            &mut self,
            _: &mut Window,
            _: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            let kind = self.kind;
            let captured = self.captured.clone();
            canvas(
                move |_, window, cx| {
                    let card_id: gpui::SharedString = "insight".into();
                    let mut card = Some(
                        InsightCard::new(card_id.clone(), "Demand changed")
                            .page(1, 3)
                            .follow_up("Should I rebalance flavors?")
                            .on_event(|_, _, _| {}),
                    );
                    macro_rules! capture_element {
                        ($element:expr) => {{
                            let element = $element.into_element();
                            let role = element.a11y_role();
                            let mut node = accesskit::Node::new(role.unwrap_or(Role::Unknown));
                            element.write_a11y_info(&mut node);
                            CapturedChild { role, node }
                        }};
                    }
                    let child = match kind {
                        ChildProbeKind::Metrics => {
                            capture_element!(metrics_group(&card_id, cx))
                        }
                        ChildProbeKind::Metric => capture_element!(metric_item(
                            &card_id,
                            InsightMetric::new("mint", "Mint Chip", "$2,377.66")
                                .change("down 4.41%", InsightTrend::Down),
                            cx,
                        )),
                        ChildProbeKind::Chart => capture_element!(chart_group(
                            &card_id,
                            "Trend snapshot".into(),
                            "Trend snapshot: Week 1 18, Week 2 24.".into(),
                            vec![
                                InsightPoint::new("Week 1", 18.0),
                                InsightPoint::new("Week 2", 24.0),
                            ],
                            cx,
                        )),
                        ChildProbeKind::Previous => capture_element!(
                            card.take()
                                .expect("probe card should be available")
                                .previous_button(cx)
                                .render(window, cx)
                        ),
                        ChildProbeKind::Next => capture_element!(
                            card.take()
                                .expect("probe card should be available")
                                .next_button(cx)
                                .render(window, cx)
                        ),
                        ChildProbeKind::FollowUp => capture_element!(
                            card.take()
                                .expect("probe card should be available")
                                .follow_up_button(cx)
                                .expect("probe card should have a follow-up")
                                .render(window, cx)
                        ),
                    };
                    *captured.lock().expect("capture mutex should be available") = Some(child);
                },
                |_, _, _, _| {},
            )
        }
    }

    fn capture_child(kind: ChildProbeKind, cx: &mut TestAppContext) -> CapturedChild {
        cx.update(crate::init);
        let captured = Arc::new(Mutex::new(None));
        let result = captured.clone();
        let (_, cx) = cx.add_window_view(move |_, _| ChildProbe { kind, captured });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let child = result
            .lock()
            .expect("capture mutex should be available")
            .take()
            .expect("semantic child should be captured");
        child
    }

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

    #[test]
    fn chart_summary_falls_back_to_series_and_labeled_values() {
        let card = InsightCard::new("insight", "Demand changed").series(
            "Trend snapshot",
            [
                InsightPoint::new("Week 1", 18.0),
                InsightPoint::new("Week 2", 24.0),
            ],
        );

        assert_eq!(
            card.resolved_chart_summary(),
            Some("Trend snapshot: Week 1 18, Week 2 24.".into())
        );
        assert_eq!(
            card.chart_summary("Demand grew overall")
                .resolved_chart_summary(),
            Some("Demand grew overall".into())
        );
    }

    #[gpui::test]
    fn metric_and_chart_children_expose_direct_semantics(cx: &mut TestAppContext) {
        let metrics = capture_child(ChildProbeKind::Metrics, cx);
        assert_eq!(metrics.role, Some(Role::List));
        assert_eq!(metrics.node.label(), Some("Insight metrics"));

        let metric = capture_child(ChildProbeKind::Metric, cx);
        assert_eq!(metric.role, Some(Role::ListItem));
        assert_eq!(
            metric.node.label(),
            Some("Mint Chip, $2,377.66, down 4.41%, decreasing")
        );

        let chart = capture_child(ChildProbeKind::Chart, cx);
        assert_eq!(chart.role, Some(Role::Group));
        assert_eq!(chart.node.label(), Some("Trend snapshot"));
        assert_eq!(
            chart.node.description(),
            Some("Trend snapshot: Week 1 18, Week 2 24.")
        );
    }

    #[gpui::test]
    fn insight_controls_expose_click_actions_and_disabled_boundaries(cx: &mut TestAppContext) {
        let previous = capture_child(ChildProbeKind::Previous, cx);
        let next = capture_child(ChildProbeKind::Next, cx);
        let follow_up = capture_child(ChildProbeKind::FollowUp, cx);
        assert_eq!(previous.role, Some(Role::Button));
        assert_eq!(previous.node.label(), Some("Previous"));
        assert!(!previous.node.supports_action(accesskit::Action::Click));

        assert_eq!(next.role, Some(Role::Button));
        assert_eq!(next.node.label(), Some("Next"));
        assert!(next.node.supports_action(accesskit::Action::Click));

        assert_eq!(follow_up.role, Some(Role::Button));
        assert_eq!(follow_up.node.label(), Some("Should I rebalance flavors?"));
        assert!(follow_up.node.supports_action(accesskit::Action::Click));
    }
}
