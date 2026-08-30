//! A frame meter for the native gallery, toggled with F3.
//!
//! Judging whether a decoration is worth its cost needs a number, and the
//! numbers already in this repository come from the performance harness — a
//! separate binary, behind a feature, driving scripted viewports. That answers
//! "did this regress" in CI. It does not answer "is what I am looking at right
//! now smooth", which is the question anyone has while building an effect.
//!
//! GPUI has its own profiler with far better instruments than these, but it
//! lives behind `gpui/profiler`, which the gallery only enables under its
//! `performance` feature. This works in a plain `cargo run -p gallery`,
//! because that is how the gallery is actually run.
//!
//! ## What the number means
//!
//! While the meter is open the gallery asks for an animation frame every
//! frame, so it draws continuously whether or not anything changed. That is
//! deliberate: an idle window draws nothing, and "0 fps" for a still picture
//! would say nothing about whether the picture was expensive to make. The
//! reading is therefore *how fast this content can be redrawn*, which is the
//! question a decoration has to answer, and not the rate the gallery would
//! sit at on its own. Closing the meter stops both the drawing and the cost.

use gpui::{App, IntoElement, ParentElement as _, Styled as _, div};
use gpui_component::{ActiveTheme as _, StyledExt as _, h_flex, v_flex};
use std::{collections::VecDeque, time::Instant};

/// Frames kept for the reading — about a second and a half at 60Hz.
///
/// Long enough that the number is steady to read, short enough that it
/// answers for the story on screen now rather than the one before it.
const WINDOW: usize = 90;

/// When the meter is drawn but has not yet seen two frames to measure.
const PENDING: &str = "—";

/// Frame arrival times, newest last.
#[derive(Default)]
pub(crate) struct FrameMeter {
    arrivals: VecDeque<Instant>,
}

/// What one look at the meter says.
pub(crate) struct Reading {
    /// Frames per second across the kept window.
    pub(crate) fps: f32,
    /// Mean milliseconds between frames.
    pub(crate) mean_ms: f32,
    /// The longest gap in the window — the hitch, which a mean hides.
    pub(crate) worst_ms: f32,
}

impl FrameMeter {
    /// Notes that a frame was drawn.
    pub(crate) fn record(&mut self, at: Instant) {
        self.arrivals.push_back(at);
        while self.arrivals.len() > WINDOW {
            self.arrivals.pop_front();
        }
    }

    /// The current reading, or `None` until there are two frames to compare.
    pub(crate) fn reading(&self) -> Option<Reading> {
        let first = self.arrivals.front()?;
        let last = self.arrivals.back()?;
        let intervals = self.arrivals.len().checked_sub(1)?;
        if intervals == 0 {
            return None;
        }
        let span = last.duration_since(*first).as_secs_f32();
        if span <= 0.0 {
            return None;
        }
        let worst = self
            .arrivals
            .iter()
            .zip(self.arrivals.iter().skip(1))
            .map(|(earlier, later)| later.duration_since(*earlier).as_secs_f32())
            .fold(0.0_f32, f32::max);
        Some(Reading {
            fps: intervals as f32 / span,
            mean_ms: span / intervals as f32 * 1_000.0,
            worst_ms: worst * 1_000.0,
        })
    }
}

/// The meter itself: a small box that sits over the corner of the window.
///
/// Deliberately plain. It is an instrument for reading the thing behind it,
/// so it competes with that thing for attention as little as it can while
/// staying legible over whatever is underneath.
pub(crate) fn overlay(reading: Option<Reading>, cx: &App) -> impl IntoElement {
    let tokens = cx.theme().semantic_tokens();
    let (fps, mean, worst) = match reading {
        Some(reading) => (
            format!("{:.0}", reading.fps),
            format!("{:.1}", reading.mean_ms),
            format!("{:.1}", reading.worst_ms),
        ),
        None => (PENDING.to_owned(), PENDING.to_owned(), PENDING.to_owned()),
    };

    v_flex()
        .absolute()
        .top(tokens.spacing.md)
        .right(tokens.spacing.md)
        .gap(tokens.spacing.xxs)
        .p(tokens.spacing.sm)
        .rounded(tokens.radius.md)
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().popover)
        .shadow_sm()
        .text_xs()
        .child(
            h_flex()
                .gap(tokens.spacing.sm)
                .justify_between()
                .child(div().text_color(cx.theme().muted_foreground).child("fps"))
                .child(div().font_semibold().child(fps)),
        )
        .child(
            h_flex()
                .gap(tokens.spacing.sm)
                .justify_between()
                .child(div().text_color(cx.theme().muted_foreground).child("frame"))
                .child(div().child(format!("{mean} ms"))),
        )
        .child(
            h_flex()
                .gap(tokens.spacing.sm)
                .justify_between()
                .child(div().text_color(cx.theme().muted_foreground).child("worst"))
                .child(div().child(format!("{worst} ms"))),
        )
        .child(
            div()
                .text_color(cx.theme().muted_foreground)
                .child("F3 · redrawing continuously"),
        )
}
