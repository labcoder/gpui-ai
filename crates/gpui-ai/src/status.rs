//! One status vocabulary for every lifecycle the library displays.
//!
//! Tool chips, task rows, tool-call cards, and plan steps all describe work
//! as pending, running, completed, or failed. [`StatusTone`] maps those
//! meanings onto the theme's semantic colors once, and [`StatusBadge`] is the
//! single compact pill that renders them, so status looks identical wherever
//! it appears.

use crate::stream::ProgressState;
use crate::theme::SemanticStyledExt as _;
use gpui::{
    App, ElementId, FontWeight, Hsla, InteractiveElement as _, IntoElement, ParentElement as _,
    RenderOnce, Role, SharedString, StatefulInteractiveElement as _, StyleRefinement, Styled,
    Window, div,
};
use gpui_component::{ActiveTheme as _, Sizable as _, StyledExt as _, h_flex, spinner::Spinner};

/// The meaning a status carries, independent of any one component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatusTone {
    /// Not started, idle, or informational-neutral.
    #[default]
    Neutral,
    /// Actively in progress.
    Info,
    /// Finished successfully.
    Success,
    /// Needs attention but is not an error.
    Warning,
    /// Failed or destructive.
    Danger,
}

impl StatusTone {
    /// Resolves the tone to the active theme's semantic color.
    pub fn color(self, cx: &App) -> Hsla {
        match self {
            Self::Neutral => cx.theme().muted_foreground,
            Self::Info => cx.theme().info,
            Self::Success => cx.theme().success,
            Self::Warning => cx.theme().warning,
            Self::Danger => cx.theme().danger,
        }
    }

    /// Maps the shared progressive lifecycle onto a tone.
    pub fn from_progress(state: &ProgressState) -> Self {
        match state {
            ProgressState::Pending => Self::Neutral,
            ProgressState::Running => Self::Info,
            ProgressState::Complete => Self::Success,
            ProgressState::Failed(_) => Self::Danger,
        }
    }
}

/// Human-readable label for a progressive lifecycle state.
pub fn progress_label(state: &ProgressState) -> &'static str {
    match state {
        ProgressState::Pending => "Pending",
        ProgressState::Running => "Running",
        ProgressState::Complete => "Completed",
        ProgressState::Failed(_) => "Failed",
    }
}

/// A compact status pill: a tone-colored dot (or spinner while active) and a
/// short label, exposed as a named status region.
///
/// # Example
///
/// ```ignore
/// StatusBadge::for_progress("call-status", task.state())
/// StatusBadge::new("review", "Needs review").tone(StatusTone::Warning)
/// ```
#[derive(IntoElement)]
pub struct StatusBadge {
    id: ElementId,
    style: StyleRefinement,
    label: SharedString,
    tone: StatusTone,
    active: bool,
}

impl StatusBadge {
    /// Creates a neutral badge.
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            style: StyleRefinement::default(),
            label: label.into(),
            tone: StatusTone::Neutral,
            active: false,
        }
    }

    /// Creates a badge describing a progressive lifecycle state.
    pub fn for_progress(id: impl Into<ElementId>, state: &ProgressState) -> Self {
        Self::new(id, progress_label(state))
            .tone(StatusTone::from_progress(state))
            .active(matches!(state, ProgressState::Running))
    }

    /// Sets the badge tone.
    pub fn tone(mut self, tone: StatusTone) -> Self {
        self.tone = tone;
        self
    }

    /// Shows a spinner instead of the dot while work is in progress.
    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }
}

impl Styled for StatusBadge {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for StatusBadge {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = cx.theme().semantic_tokens();
        let color = self.tone.color(cx);
        h_flex()
            .id(self.id)
            .role(Role::Status)
            .aria_label(self.label.clone())
            .flex_none()
            .items_center()
            .gap(tokens.spacing.xs)
            .px(tokens.spacing.sm)
            .py(tokens.spacing.xxs)
            .rounded(tokens.radius.full)
            .bg(color.opacity(0.12))
            .text_token(tokens.typography.xs)
            .font_weight(FontWeight::MEDIUM)
            .text_color(color)
            .child(if self.active {
                Spinner::new().xsmall().color(color).into_any_element()
            } else {
                div()
                    .size_1p5()
                    .rounded(tokens.radius.full)
                    .bg(color)
                    .into_any_element()
            })
            .child(self.label)
            .refine_style(&self.style)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_maps_onto_one_tone_and_label_each() {
        let cases = [
            (ProgressState::Pending, StatusTone::Neutral, "Pending"),
            (ProgressState::Running, StatusTone::Info, "Running"),
            (ProgressState::Complete, StatusTone::Success, "Completed"),
            (
                ProgressState::Failed("offline".into()),
                StatusTone::Danger,
                "Failed",
            ),
        ];
        for (state, tone, label) in cases {
            assert_eq!(StatusTone::from_progress(&state), tone);
            assert_eq!(progress_label(&state), label);
            let badge = StatusBadge::for_progress("badge", &state);
            assert_eq!(badge.tone, tone);
            assert_eq!(badge.label.as_ref(), label);
            assert_eq!(badge.active, matches!(state, ProgressState::Running));
        }
    }
}
