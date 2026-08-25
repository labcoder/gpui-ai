//! Message queue: prompts waiting their turn while the agent is busy.
//!
//! Agent harnesses let people keep typing while a turn runs; those prompts
//! wait in a queue the application owns. [`MessageQueue`] renders the queue
//! above the composer with position, text, optional note, and controls to
//! reorder, edit, send now, or remove, reporting each as a [`QueueEvent`]
//! keyed by stable ID.

use crate::{
    handlers::SharedHandler,
    motion::{reorder, reveal_staggered},
    surface::{eyebrow, icon_button, meta},
    theme::SemanticStyledExt as _,
};
use gpui::{
    AnyElement, App, ClickEvent, ElementId, InteractiveElement as _, IntoElement,
    ParentElement as _, RenderOnce, Role, SharedString, StatefulInteractiveElement as _,
    StyleRefinement, Styled, Window, div, prelude::FluentBuilder as _,
};
use gpui_component::{
    ActiveTheme as _, IconName, Sizable as _, StyledExt as _,
    button::{Button, ButtonVariants as _},
    h_flex, v_flex,
};
use std::rc::Rc;

/// One prompt waiting in the queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedMessage {
    id: SharedString,
    text: SharedString,
    note: Option<SharedString>,
}

impl QueuedMessage {
    /// Creates a queued prompt with a stable identifier.
    pub fn new(id: impl Into<SharedString>, text: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
            note: None,
        }
    }

    /// Adds a short note ("after the current step", "steering").
    pub fn note(mut self, note: impl Into<SharedString>) -> Self {
        self.note = Some(note.into());
        self
    }

    /// Returns the stable identifier.
    pub fn id(&self) -> &SharedString {
        &self.id
    }

    /// Returns the prompt text.
    pub fn text(&self) -> &SharedString {
        &self.text
    }

    /// Returns the note, if any.
    pub fn note_text(&self) -> Option<&SharedString> {
        self.note.as_ref()
    }
}

/// An interaction emitted by [`MessageQueue`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueEvent {
    /// Remove one queued prompt.
    Removed {
        /// Stable message identifier.
        id: SharedString,
    },
    /// Send one queued prompt immediately, ahead of its turn.
    SentNow {
        /// Stable message identifier.
        id: SharedString,
    },
    /// Move one queued prompt one place earlier.
    MovedUp {
        /// Stable message identifier.
        id: SharedString,
    },
    /// Move one queued prompt one place later.
    MovedDown {
        /// Stable message identifier.
        id: SharedString,
    },
    /// Edit one queued prompt before it is sent.
    EditRequested {
        /// Stable message identifier.
        id: SharedString,
    },
    /// Drop every queued prompt.
    Cleared,
}

/// The queue of waiting prompts.
///
/// # Example
///
/// ```ignore
/// MessageQueue::new("queue")
///     .items(queued.iter().cloned())
///     .editable(true)
///     .on_event(|event, _, _| { /* QueueEvent::SentNow { id } … */ })
/// ```
#[derive(IntoElement)]
pub struct MessageQueue {
    id: SharedString,
    style: StyleRefinement,
    items: Vec<QueuedMessage>,
    editable: bool,
    on_event: Option<SharedHandler<QueueEvent>>,
}

impl MessageQueue {
    /// Creates an empty queue.
    pub fn new(id: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            style: StyleRefinement::default(),
            items: Vec::new(),
            editable: false,
            on_event: None,
        }
    }

    /// Sets the queued prompts, first to send first.
    pub fn items(mut self, items: impl IntoIterator<Item = QueuedMessage>) -> Self {
        self.items = items.into_iter().collect();
        self
    }

    /// Offers an edit control per prompt.
    pub fn editable(mut self, editable: bool) -> Self {
        self.editable = editable;
        self
    }

    /// Handles typed interactions. Without a handler the queue is read-only.
    pub fn on_event(
        mut self,
        handler: impl Fn(&QueueEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_event = Some(Rc::new(handler));
        self
    }
}

impl Styled for MessageQueue {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for MessageQueue {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = cx.theme().semantic_tokens();
        let root_id = ElementId::from(self.id.clone());
        let debug_id = self.id.to_string();
        let handler = self.on_event;
        let count = self.items.len();

        if count == 0 {
            return div().id(root_id).hidden().refine_style(&self.style);
        }

        let mut rows = Vec::with_capacity(count);
        for (index, item) in self.items.iter().enumerate() {
            rows.push(render_row(
                &root_id,
                &debug_id,
                index,
                count,
                item,
                self.editable,
                handler.clone(),
                window,
                cx,
            ));
        }
        let clear = handler.clone().map(|handler| {
            let clear_debug = debug_id.clone();
            div()
                .debug_selector(move || format!("queue-clear-{clear_debug}"))
                .child(
                    Button::new((root_id.clone(), "clear"))
                        .ghost()
                        .xsmall()
                        .accessibility_id(format!("{debug_id}-clear"))
                        .label("Clear queue")
                        .on_click(move |_: &ClickEvent, window, cx| {
                            handler(&QueueEvent::Cleared, window, cx)
                        }),
                )
        });
        let root_debug = debug_id.clone();

        v_flex()
            .id(root_id)
            .role(Role::List)
            .aria_label(format!("Queued messages, {count} waiting"))
            .debug_selector(move || format!("queue-{root_debug}"))
            .w_full()
            .min_w_0()
            .gap(tokens.spacing.xs)
            .p(tokens.spacing.sm)
            .bg(cx.theme().muted.opacity(0.35))
            .border_1()
            .border_color(cx.theme().border)
            .rounded(tokens.radius.md)
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap(tokens.spacing.sm)
                    .child(eyebrow(format!("Queued · {count}"), cx))
                    .children(clear),
            )
            .children(rows)
            .refine_style(&self.style)
    }
}

#[allow(clippy::too_many_arguments)]
fn render_row(
    root_id: &ElementId,
    queue_debug: &str,
    index: usize,
    count: usize,
    item: &QueuedMessage,
    editable: bool,
    handler: Option<SharedHandler<QueueEvent>>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let tokens = cx.theme().semantic_tokens();
    let row_id = ElementId::from((root_id.clone(), format!("item-{}", item.id)));
    let debug_id = format!("{queue_debug}-{}", item.id);
    let label: SharedString = format!("Queued {} of {count}: {}", index + 1, item.text).into();

    let control = |suffix: &'static str,
                   icon: IconName,
                   name: String,
                   enabled: bool,
                   event: QueueEvent,
                   cx: &mut App| {
        let handler = handler.clone();
        let selector = format!("queue-{suffix}-{debug_id}");
        icon_button((row_id.clone(), suffix), icon, name, cx)
            .disabled(!enabled || handler.is_none())
            .debug_selector(move || selector.clone())
            .on_click(move |_: &ClickEvent, window, cx| {
                if let Some(handler) = &handler {
                    handler(&event, window, cx)
                }
            })
    };
    let id = item.id.clone();
    let controls = h_flex()
        .flex_none()
        .items_center()
        .gap(tokens.spacing.xxs)
        .child(control(
            "up",
            IconName::ChevronUp,
            format!("Move earlier: {}", item.text),
            index > 0,
            QueueEvent::MovedUp { id: id.clone() },
            cx,
        ))
        .child(control(
            "down",
            IconName::ChevronDown,
            format!("Move later: {}", item.text),
            index + 1 < count,
            QueueEvent::MovedDown { id: id.clone() },
            cx,
        ))
        .when(editable, |this| {
            this.child(control(
                "edit",
                IconName::Replace,
                format!("Edit: {}", item.text),
                true,
                QueueEvent::EditRequested { id: id.clone() },
                cx,
            ))
        })
        .child(control(
            "send",
            IconName::ArrowUp,
            format!("Send now: {}", item.text),
            true,
            QueueEvent::SentNow { id: id.clone() },
            cx,
        ))
        .child(control(
            "remove",
            IconName::Close,
            format!("Remove: {}", item.text),
            true,
            QueueEvent::Removed { id: id.clone() },
            cx,
        ));
    let row_debug = debug_id.clone();
    let row = h_flex()
        .id(row_id.clone())
        .role(Role::ListItem)
        .aria_label(label)
        .debug_selector(move || format!("queue-item-{row_debug}"))
        .items_center()
        .w_full()
        .min_w_0()
        .gap(tokens.spacing.sm)
        .px(tokens.spacing.sm)
        .py(tokens.spacing.xs)
        .rounded(tokens.radius.sm)
        .bg(tokens.colors.surface)
        .border_1()
        .border_color(cx.theme().border)
        .child(meta((index + 1).to_string(), cx))
        .child(
            div()
                .flex()
                .flex_col()
                .min_w_0()
                .flex_1()
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_token(tokens.typography.sm)
                        .text_color(cx.theme().foreground)
                        .child(item.text.clone()),
                )
                .when_some(item.note.clone(), |this, note| {
                    this.child(
                        div()
                            .text_token(tokens.typography.xs)
                            .text_color(cx.theme().muted_foreground)
                            .child(note),
                    )
                }),
        )
        .child(controls);
    // Reveal for a row that has just arrived, reorder for one that was already
    // here and has moved: Move up and Move down change a row's neighbours, and
    // a row that is simply somewhere else on the next frame reads as two rows
    // swapping their contents rather than one row moving.
    //
    // Keyed by the message id, never by the index — keyed by index every row
    // would appear to move whenever any row did.
    reorder(
        reveal_staggered(
            div().w_full().min_w_0().child(row),
            (root_id.clone(), format!("reveal-{}", item.id)),
            index,
            window,
            cx,
        ),
        (root_id.clone(), format!("reorder-{}", item.id)),
        index,
        window,
        cx,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_are_optional() {
        let item = QueuedMessage::new("a", "Compare prices").note("after the current step");
        assert_eq!(
            item.note_text().map(|n| n.as_ref()),
            Some("after the current step")
        );
        assert!(QueuedMessage::new("b", "x").note_text().is_none());
    }
}
