//! An animated pixel-grid loading state with optional elapsed time.

use crate::theme::SemanticStyledExt as _;
use gpui::{
    Animation, AnimationExt as _, App, IntoElement, ParentElement as _, RenderOnce, SharedString,
    StyleRefinement, Styled, Window, div, prelude::FluentBuilder as _, px,
};
use gpui_component::{ActiveTheme as _, StyledExt as _, h_flex};
use std::time::Duration;

/// Grid dimensions of the loader.
const ROWS: usize = 3;
const COLS: usize = 3;
/// One full shimmer sweep across the grid.
const SWEEP: Duration = Duration::from_millis(1400);

/// A pixel-grid loader for "the agent is working" moments.
///
/// A small grid of squares pulses in a diagonal sweep, next to a label and an
/// optional elapsed-time readout. The component holds no timer: the caller
/// owns the clock and re-renders to tick the elapsed display (the sweep
/// itself animates independently via GPUI's animation system).
///
/// # Example
///
/// ```ignore
/// LoadingState::new()
///     .label("Reasoning about your request")
///     .elapsed(Duration::from_secs(7))
/// ```
#[derive(IntoElement)]
pub struct LoadingState {
    style: StyleRefinement,
    label: SharedString,
    elapsed: Option<Duration>,
}

impl LoadingState {
    /// Creates a loader with the default label.
    pub fn new() -> Self {
        Self {
            style: StyleRefinement::default(),
            label: "Working…".into(),
            elapsed: None,
        }
    }

    /// Sets the label describing what is happening.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = label.into();
        self
    }

    /// Shows an elapsed-time readout after the label.
    pub fn elapsed(mut self, elapsed: Duration) -> Self {
        self.elapsed = Some(elapsed);
        self
    }
}

impl Default for LoadingState {
    fn default() -> Self {
        Self::new()
    }
}

impl Styled for LoadingState {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for LoadingState {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = cx.theme().semantic_tokens();
        let color = cx.theme().primary;

        let grid = div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .children((0..ROWS).map(|row| {
                h_flex().gap(px(2.)).children((0..COLS).map(move |col| {
                    // Each cell's pulse is phase-shifted along the diagonal,
                    // producing a sweep from the top-left corner.
                    let phase = (row + col) as f32 / ((ROWS + COLS) as f32);
                    div().size(px(5.)).rounded(px(1.)).bg(color).with_animation(
                        ("loading-cell", (row * COLS + col) as u64),
                        Animation::new(SWEEP).repeat(),
                        move |this, delta| {
                            let wave = ((delta - phase).rem_euclid(1.0) * 2.0 - 1.0).abs();
                            this.opacity(0.15 + 0.85 * wave)
                        },
                    )
                }))
            }));

        h_flex()
            .items_center()
            .gap(tokens.spacing.md)
            .child(grid)
            .child(
                div()
                    .text_token(tokens.typography.sm)
                    .text_color(cx.theme().muted_foreground)
                    .child(self.label),
            )
            .when_some(self.elapsed, |this, elapsed| {
                this.child(
                    div()
                        .text_token(tokens.typography.xs)
                        .font_family(cx.theme().mono_font_family.clone())
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("{:.0}s", elapsed.as_secs_f64())),
                )
            })
            .refine_style(&self.style)
    }
}
