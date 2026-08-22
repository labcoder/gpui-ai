//! Agent plan display: a to-do list with live completion status.

use crate::control::composed_button;
use crate::handlers::SharedHandler;
use crate::theme::SemanticStyledExt as _;
use gpui::{
    App, ClickEvent, ElementId, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, RenderOnce, Role, SharedString, StatefulInteractiveElement as _,
    StyleRefinement, Styled, Window, accesskit, div, prelude::FluentBuilder as _,
};
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
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = cx.theme().semantic_tokens();
        let done = self
            .items
            .iter()
            .filter(|item| item.status == TodoStatus::Done)
            .count();
        let total = self.items.len();
        let handler = self.on_event;
        let list_name = self.title.clone().unwrap_or_else(|| "To-do list".into());
        let accessibility_label: SharedString =
            format!("{list_name}, {done} of {total} complete").into();

        v_flex()
            .id(self.id)
            .role(Role::List)
            .aria_label(accessibility_label)
            .p(tokens.spacing.md)
            .gap(tokens.spacing.xs)
            .bg(cx.theme().background)
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
                    TodoStatus::Done => Icon::new(IconName::CircleCheck)
                        .small()
                        .text_color(cx.theme().success)
                        .into_any_element(),
                };

                let event = item.toggled_event();
                let row = h_flex()
                    .w_full()
                    .items_center()
                    .gap(tokens.spacing.sm)
                    .child(div().flex_none().child(indicator))
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
