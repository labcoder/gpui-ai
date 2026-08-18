//! Compact chips representing tool calls and code edits.

use crate::handlers::SharedHandler;
use crate::theme::SemanticStyledExt as _;
use gpui::{
    App, ClickEvent, IntoElement, ParentElement as _, RenderOnce, SharedString, StyleRefinement,
    Styled, Window, div, prelude::FluentBuilder as _,
};
use gpui_component::{
    ActiveTheme as _, Sizable as _, StyledExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    spinner::Spinner,
};
use std::rc::Rc;

/// The lifecycle status of the tool call a [`ToolChip`] represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToolStatus {
    /// Queued but not started.
    #[default]
    Pending,
    /// Currently executing; the chip shows a spinner.
    Running,
    /// Completed successfully.
    Success,
    /// Ended with an error.
    Failed,
}

/// A compact, pill-shaped representation of a tool call or code edit.
///
/// # Example
///
/// ```ignore
/// ToolChip::new("edit-1", "edit main.rs")
///     .status(ToolStatus::Running)
///     .detail("+12 −3")
/// ```
#[derive(IntoElement)]
pub struct ToolChip {
    id: SharedString,
    style: StyleRefinement,
    label: SharedString,
    detail: Option<SharedString>,
    status: ToolStatus,
    on_event: Option<SharedHandler<ToolChipEvent>>,
}

impl ToolChip {
    /// Creates a chip with a unique id and a short label (for example the
    /// tool name or the file being edited).
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            style: StyleRefinement::default(),
            label: label.into(),
            detail: None,
            status: ToolStatus::default(),
            on_event: None,
        }
    }

    /// Sets the lifecycle status. Default is [`ToolStatus::Pending`].
    pub fn status(mut self, status: ToolStatus) -> Self {
        self.status = status;
        self
    }

    /// Adds a muted trailing detail, such as a diff stat (`+12 −3`).
    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Makes the chip clickable (for example to open the tool call).
    pub fn on_event(
        mut self,
        handler: impl Fn(&ToolChipEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_event = Some(Rc::new(handler));
        self
    }
}

impl Styled for ToolChip {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for ToolChip {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = cx.theme().semantic_tokens();
        let status_color = match self.status {
            ToolStatus::Pending => cx.theme().muted_foreground,
            ToolStatus::Running => cx.theme().info,
            ToolStatus::Success => cx.theme().success,
            ToolStatus::Failed => cx.theme().danger,
        };

        let event = ToolChipEvent::Activated {
            id: self.id.clone(),
        };
        let content = h_flex()
            .items_center()
            .gap(tokens.spacing.xs)
            .px(tokens.spacing.sm)
            .py(tokens.spacing.xxs)
            .text_token(tokens.typography.xs)
            .font_family(cx.theme().mono_font_family.clone())
            .text_color(cx.theme().foreground)
            .bg(cx.theme().secondary)
            .border_1()
            .border_color(cx.theme().border)
            .rounded(tokens.radius.full)
            .child(match self.status {
                ToolStatus::Running => Spinner::new()
                    .xsmall()
                    .color(status_color)
                    .into_any_element(),
                _ => div()
                    .size_1p5()
                    .rounded(tokens.radius.full)
                    .bg(status_color)
                    .into_any_element(),
            })
            .child(self.label)
            .when_some(self.detail, |this, detail| {
                this.child(div().text_color(cx.theme().muted_foreground).child(detail))
            })
            .refine_style(&self.style);

        if let Some(handler) = self.on_event {
            Button::new(self.id.clone())
                .ghost()
                .compact()
                .accessibility_id(format!("tool-chip-{}", self.id))
                .child(content)
                .on_click(move |_: &ClickEvent, window, cx| handler(&event, window, cx))
                .into_any_element()
        } else {
            content.into_any_element()
        }
    }
}
/// An interaction emitted by [`ToolChip`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolChipEvent {
    /// The represented tool call was selected.
    Activated {
        /// Stable tool-call identifier.
        id: SharedString,
    },
}
