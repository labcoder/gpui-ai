//! Four decorations, written the way an application would write them.
//!
//! None of this lives in `gpui-ai`. The library provides the slot and the
//! motion channel; what fills them is expression, and expression belongs to
//! whoever is building the thing. These exist to prove the slot is worth
//! having, and to be read by someone deciding what to put in their own.
//!
//! Every one is drawn in code — no assets, so the repository does not grow a
//! megabyte to demonstrate a feature — and every animated one goes through
//! [`gpui_ai::prelude::decoration::animated`], so it stops when it scrolls
//! out of view and holds still under a reduced-motion preference.

use gpui::{
    App, IntoElement, ParentElement as _, Styled as _, div, linear_color_stop, linear_gradient,
    pattern_slash, px, relative,
};
use gpui_ai::prelude::{Decoration, decoration};
use gpui_component::ActiveTheme as _;
use std::time::Duration;

/// Which decoration the story is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum DecorationKind {
    /// Diagonal hatching from GPUI's own pattern fill.
    #[default]
    Hatch,
    /// A grid of dots that breathe.
    Halftone,
    /// Rings that travel out from a press.
    Ripple,
    /// A gradient veil over the content rather than under it.
    Veil,
}

impl DecorationKind {
    pub(crate) const ALL: &'static [Self] =
        &[Self::Hatch, Self::Halftone, Self::Ripple, Self::Veil];

    pub(crate) const LABELS: &'static [(&'static str, &'static str)] = &[
        ("hatch", "Cross-hatch"),
        ("halftone", "Halftone"),
        ("ripple", "Ripple"),
        ("veil", "Veil"),
    ];

    pub(crate) fn index(self) -> usize {
        Self::ALL.iter().position(|kind| *kind == self).unwrap_or(0)
    }

    /// What this one is doing, in a line.
    pub(crate) fn note(self) -> &'static str {
        match self {
            Self::Hatch => {
                "A GPUI pattern fill. No per-frame cost, no image, and it is \
                 resolution-independent — the whole effect is one background."
            }
            Self::Halftone => {
                "Ninety-six dots on a grid, breathing on the library's motion \
                 channel. It stops when the panel scrolls out of view."
            }
            Self::Ripple => {
                "Rings driven from a press rather than a clock. The library \
                 eases the value; the rings are the application's own drawing."
            }
            Self::Veil => {
                "The over layer, not the under one: a gradient across the \
                 content, passing every click through to what it covers."
            }
        }
    }

    /// Builds the decoration itself.
    pub(crate) fn build(self, ripple: f32, cx: &App) -> Decoration {
        match self {
            Self::Hatch => Decoration::behind(hatch(cx)),
            Self::Halftone => Decoration::behind(halftone(cx)),
            Self::Ripple => Decoration::behind(rings(ripple, cx)),
            Self::Veil => Decoration::above(veil(cx)),
        }
    }
}

/// Diagonal hatching, straight from GPUI's own pattern fill.
fn hatch(cx: &App) -> impl IntoElement {
    div().size_full().bg(pattern_slash(
        cx.theme().muted_foreground.opacity(0.16),
        1.5,
        9.0,
    ))
}

/// Dots on a grid whose size breathes, so the field reads as a texture that
/// is alive rather than a picture of one.
fn halftone(cx: &App) -> impl IntoElement {
    const COLUMNS: usize = 16;
    const ROWS: usize = 6;
    let ink = cx.theme().muted_foreground.opacity(0.22);
    decoration::animated("halftone", Duration::from_secs(6), move |delta| {
        // One wave crossing the grid, so a dot's size depends on where it is
        // as well as when it is — a field rather than a pulse.
        let phase = delta * std::f32::consts::TAU;
        div().size_full().children((0..ROWS).map(move |row| {
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(relative(row as f32 / ROWS as f32))
                .flex()
                .justify_between()
                .children((0..COLUMNS).map(move |column| {
                    let across = (column as f32 / COLUMNS as f32) * std::f32::consts::TAU;
                    let wave = (phase + across + row as f32 * 0.6).sin() * 0.5 + 0.5;
                    let size = px(2.0 + wave * 5.0);
                    div().size(size).rounded_full().bg(ink)
                }))
        }))
    })
}

/// Rings travelling out from the centre, sized by a value the application
/// moves rather than by a clock the decoration owns.
fn rings(progress: f32, cx: &App) -> impl IntoElement {
    let ink = cx.theme().primary;
    div().size_full().children((0..3).map(move |ring| {
        // Each ring trails the one before it, so a single value produces a
        // sequence rather than three circles doing the same thing.
        let offset = ring as f32 * 0.18;
        let travel = (progress - offset).clamp(0.0, 1.0);
        let scale = travel * 1.4;
        div()
            .absolute()
            .top(relative(0.5 - scale / 2.0))
            .left(relative(0.5 - scale / 2.0))
            .w(relative(scale))
            .h(relative(scale))
            .rounded_full()
            .border_2()
            .border_color(ink.opacity((1.0 - travel) * 0.5))
    }))
}

/// A gradient across the content: the over layer, proving it passes input
/// through to everything it covers.
fn veil(cx: &App) -> impl IntoElement {
    div().size_full().bg(linear_gradient(
        // Down and across, so it reads as light falling rather than a band.
        135.0,
        linear_color_stop(cx.theme().primary.opacity(0.0), 0.0),
        linear_color_stop(cx.theme().primary.opacity(0.16), 1.0),
    ))
}
