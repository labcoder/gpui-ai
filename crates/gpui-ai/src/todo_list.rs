//! Agent plan display: a to-do list with live completion status.

use crate::control::composed_button;
use crate::handlers::SharedHandler;
use crate::motion::{ArrivalRoster, MotionTokens, acknowledged_state, reveal_progress};
use crate::theme::SemanticStyledExt as _;
use gpui::{
    App, ClickEvent, ElementId, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, RenderOnce, Role, SharedString, StatefulInteractiveElement as _,
    StyleRefinement, Styled, Window, accesskit, div, prelude::FluentBuilder as _,
};
use gpui_base::animation::ease_out_cubic;
use gpui_base::motion::{Transition, transition};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable as _, StyledExt as _, h_flex, spinner::Spinner,
    v_flex,
};

/// Status of one [`TodoItem`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TodoStatus {
    /// Not started.
    #[default]
    Pending,
    /// The step the agent is on now; shows a spinner.
    Active,
    /// Completed.
    Done,
}

/// One entry in a [`TodoList`].
#[derive(Debug, Clone)]
pub struct TodoItem {
    id: SharedString,
    label: SharedString,
    status: TodoStatus,
}

impl TodoItem {
    /// Creates a pending item.
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            status: TodoStatus::default(),
        }
    }

    /// Sets the status. Default is [`TodoStatus::Pending`].
    pub fn status(mut self, status: TodoStatus) -> Self {
        self.status = status;
        self
    }

    /// Shorthand for `.status(TodoStatus::Done)`.
    pub fn done(self) -> Self {
        self.status(TodoStatus::Done)
    }

    fn toggled_event(&self) -> TodoListEvent {
        TodoListEvent::Toggled {
            id: self.id.clone(),
        }
    }

    fn accessibility_toggled(&self) -> accesskit::Toggled {
        match self.status {
            TodoStatus::Done => accesskit::Toggled::True,
            TodoStatus::Pending | TodoStatus::Active => accesskit::Toggled::False,
        }
    }
}

/// An interaction emitted by [`TodoList`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TodoListEvent {
    /// A plan item was selected for toggling.
    Toggled {
        /// Stable plan-item identifier.
        id: SharedString,
    },
}

/// An agent's plan as a checklist, updating as steps complete.
///
/// Differs from [`TaskRow`](crate::task::TaskRow) in intent: task rows show
/// independent background work with timing and failure detail; a to-do list
/// shows one plan executing in order.
///
/// # Example
///
/// ```ignore
/// TodoList::new("plan")
///     .title("Refactor plan")
///     .items([
///         TodoItem::new("read", "Read existing schema").done(),
///         TodoItem::new("write", "Write migration").status(TodoStatus::Active),
///         TodoItem::new("test", "Run test suite"),
///     ])
/// ```
#[derive(IntoElement)]
pub struct TodoList {
    id: ElementId,
    style: StyleRefinement,
    title: Option<SharedString>,
    items: Vec<TodoItem>,
    on_event: Option<SharedHandler<TodoListEvent>>,
}

impl TodoList {
    /// Creates an empty list.
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            style: StyleRefinement::default(),
            title: None,
            items: Vec::new(),
            on_event: None,
        }
    }

    /// Adds a header title with a completion count.
    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Sets the plan items, in execution order.
    pub fn items(mut self, items: impl IntoIterator<Item = TodoItem>) -> Self {
        self.items = items.into_iter().collect();
        self
    }

    /// Handles typed plan-item interactions.
    pub fn on_event(
        mut self,
        handler: impl Fn(&TodoListEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_event = Some(std::rc::Rc::new(handler));
        self
    }
}

impl Styled for TodoList {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for TodoList {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = cx.theme().semantic_tokens();
        let motion = MotionTokens::read(cx).clone();
        let done = self
            .items
            .iter()
            .filter(|item| item.status == TodoStatus::Done)
            .count();
        let total = self.items.len();
        let handler = self.on_event;

        // One shared fill carries the plan's completion: it retargets from
        // its current sample when steps complete or reopen, starts at the
        // supplied value on first render, and snaps under reduced motion —
        // the transition contract, not local policy.
        let fraction = if total == 0 {
            0.0
        } else {
            done as f32 / total as f32
        };
        let fill = transition(
            (self.id.clone(), "todo-fill"),
            fraction,
            Transition::new(motion.standard()).ease(ease_out_cubic),
            window,
            cx,
        );

        // Items the list has already shown never re-animate; new stable IDs
        // appended to a mounted list settle in on the capped cascade, and
        // the initial load joins at rest.
        let roster = window.use_keyed_state((self.id.clone(), "arrivals"), cx, |_, _| {
            ArrivalRoster::new()
        });
        roster.update(cx, |roster, _| {
            roster.note(
                self.items
                    .iter()
                    .map(|item| ElementId::Name(SharedString::from(format!("todo-{}", item.id)))),
                true,
                &motion,
            );
        });
        let list_name = self.title.clone().unwrap_or_else(|| "To-do list".into());
        let accessibility_label: SharedString =
            format!("{list_name}, {done} of {total} complete").into();

        v_flex()
            .id(self.id)
            .role(Role::List)
            .aria_label(accessibility_label)
            .p(tokens.spacing.md)
            .gap(tokens.spacing.xs)
            .bg(tokens.colors.surface)
            .border_1()
            .border_color(cx.theme().border)
            .rounded(tokens.radius.md)
            .when_some(self.title, |this, title| {
                this.child(
                    h_flex()
                        .items_center()
                        .justify_between()
                        .pb(tokens.spacing.xs)
                        .child(
                            div()
                                .text_token(tokens.typography.xs)
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(cx.theme().muted_foreground)
                                .child(title),
                        )
                        .child(
                            div()
                                .text_token(tokens.typography.xs)
                                .font_family(cx.theme().mono_font_family.clone())
                                .text_color(cx.theme().muted_foreground)
                                .child(format!("{done}/{total}")),
                        ),
                )
            })
            .child(
                // The shared completion fill under the header; the track is
                // always full width, the fill is the sampled fraction.
                div()
                    .w_full()
                    .h(tokens.spacing.xxs)
                    .rounded(tokens.radius.full)
                    .bg(cx.theme().muted)
                    .child(
                        div()
                            .h_full()
                            .w(gpui::relative(fill))
                            .rounded(tokens.radius.full)
                            .bg(cx.theme().primary),
                    ),
            )
            .children(self.items.into_iter().map(|item| {
                let item_id = item.id.clone();
                let accessibility_label = item.label.clone();
                let accessibility_toggled = item.accessibility_toggled();
                let accessibility_description =
                    (item.status == TodoStatus::Active).then_some("In progress");
                let indicator = match item.status {
                    TodoStatus::Pending => div()
                        .size_3()
                        .rounded(tokens.radius.full)
                        .border_1()
                        .border_color(cx.theme().muted_foreground)
                        .into_any_element(),
                    TodoStatus::Active => Spinner::new()
                        .xsmall()
                        .color(cx.theme().info)
                        .into_any_element(),
                    // The durable completion mark settles in once, after the
                    // controlled status changes; loaded-done items are the
                    // mount state and exempt.
                    TodoStatus::Done => Icon::new(IconName::CircleCheck)
                        .small()
                        .text_color(cx.theme().success)
                        .opacity(acknowledged_state(
                            ElementId::Name(SharedString::from(format!("todo-glyph-{}", item.id))),
                            2,
                            window,
                            cx,
                        ))
                        .into_any_element(),
                };
                let arrival = roster
                    .read(cx)
                    .delay(&ElementId::Name(SharedString::from(format!(
                        "todo-{}",
                        item.id
                    ))))
                    .map(|delay| {
                        reveal_progress(
                            ElementId::Name(SharedString::from(format!("todo-arrive-{}", item.id))),
                            delay,
                            window,
                            cx,
                        )
                    });

                let event = item.toggled_event();
                let row = h_flex()
                    .w_full()
                    .items_center()
                    .gap(tokens.spacing.sm)
                    .child(
                        // A fixed square slot: ring, spinner, and check all
                        // centre in the same box, so a status change never
                        // nudges the label sideways.
                        div()
                            .flex_none()
                            .size(tokens.spacing.lg)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(indicator),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_token(tokens.typography.sm)
                            .map(|this| match item.status {
                                TodoStatus::Done => {
                                    this.line_through().text_color(cx.theme().muted_foreground)
                                }
                                TodoStatus::Active => this.text_color(cx.theme().foreground),
                                TodoStatus::Pending => this.text_color(cx.theme().muted_foreground),
                            })
                            .child(item.label),
                    );

                let row = match arrival {
                    Some(progress) => row
                        .opacity(progress)
                        .top(tokens.spacing.xxs * (1.0 - progress)),
                    None => row,
                };

                match handler.clone() {
                    Some(handler) => composed_button(item.id.clone(), accessibility_label)
                        .w_full()
                        .role(Role::CheckBox)
                        .aria_toggled(accessibility_toggled)
                        .px(tokens.spacing.xs)
                        .py(tokens.spacing.xxs)
                        .rounded(tokens.radius.sm)
                        .hover(|style| style.bg(cx.theme().accent))
                        .active(|style| style.bg(cx.theme().accent.opacity(0.8)))
                        .focus_visible(|style| style.bg(cx.theme().accent))
                        .when_some(accessibility_description, |this, description| {
                            this.aria_description(description)
                        })
                        .child(row)
                        .on_click(move |_: &ClickEvent, window, cx| handler(&event, window, cx))
                        .into_any_element(),
                    None => div()
                        .id(item_id)
                        .role(Role::ListItem)
                        .aria_label(accessibility_label)
                        .when_some(accessibility_description, |this, description| {
                            this.aria_description(description)
                        })
                        .child(row)
                        .into_any_element(),
                }
            }))
            .refine_style(&self.style)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Context, Render, TestAppContext, VisualTestContext, px};

    struct ListProbe {
        count: usize,
    }

    impl Render for ListProbe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().w(px(320.)).h(px(320.)).child(
                TodoList::new("probe-plan")
                    .title("Plan")
                    .items((0..self.count).map(|ix| TodoItem::new(format!("step-{ix}"), "Step"))),
            )
        }
    }

    fn draw(cx: &mut VisualTestContext) {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    #[gpui::test]
    fn a_loaded_list_rests_and_an_appended_item_settles_in(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (probe, cx) = cx.add_window_view(|_, _| ListProbe { count: 3 });
        let cx: &mut VisualTestContext = cx;
        draw(cx);
        crate::motion::take_reveal_frame_requests();
        draw(cx);
        assert_eq!(
            crate::motion::take_reveal_frame_requests(),
            0,
            "the initial load joins at rest"
        );

        probe.update(cx, |probe, cx| {
            probe.count = 4;
            cx.notify();
        });
        draw(cx);
        assert!(
            crate::motion::take_reveal_frame_requests() > 0,
            "an item appended to a mounted list must settle in"
        );

        cx.executor()
            .advance_clock(std::time::Duration::from_secs(2));
        draw(cx);
        crate::motion::take_reveal_frame_requests();
        probe.update(cx, |_, cx| cx.notify());
        draw(cx);
        assert_eq!(
            crate::motion::take_reveal_frame_requests(),
            0,
            "a re-render must not replay an acknowledged arrival"
        );
    }

    #[test]
    fn duplicate_labels_emit_distinct_stable_ids() {
        let first = TodoItem::new("first", "Same step");
        let second = TodoItem::new("second", "Same step");
        assert_eq!(
            first.toggled_event(),
            TodoListEvent::Toggled { id: "first".into() }
        );
        assert_eq!(
            second.toggled_event(),
            TodoListEvent::Toggled {
                id: "second".into()
            }
        );
    }

    #[test]
    fn accessibility_checked_state_tracks_completion() {
        assert_eq!(
            TodoItem::new("pending", "Pending").accessibility_toggled(),
            accesskit::Toggled::False
        );
        assert_eq!(
            TodoItem::new("active", "Active")
                .status(TodoStatus::Active)
                .accessibility_toggled(),
            accesskit::Toggled::False
        );
        assert_eq!(
            TodoItem::new("done", "Done").done().accessibility_toggled(),
            accesskit::Toggled::True
        );
    }
}
