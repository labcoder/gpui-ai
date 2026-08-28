//! One hover highlight that glides between the uniform rows of a list.
//!
//! Per-row hover fills flicker: each pointer move extinguishes one row's
//! background and ignites the next. A gliding highlight is one element that
//! chases the pointer between rows at the hover-glide tempo, so hovering
//! reads as a single object in motion. Rows opt in by attaching
//! [`glide_row`], the list container mounts [`glide_highlight`] inside a
//! `relative()` frame it captured with [`glide_frame`], and rows stop
//! painting their own hover fill — the highlight is the hover.
//!
//! Geometry rules: the highlight tweens between *different* rows; a bounds
//! change under the same hovered row (the list scrolled beneath a
//! stationary pointer) repositions instantly, so the highlight never lags
//! its own row. Any reduced motion preference snaps every move. Leaving
//! the rows drops the highlight the way per-row hover would.

use std::collections::HashMap;

use gpui::{
    App, Bounds, Div, ElementId, Entity, InteractiveElement as _, Pixels,
    SharedString, StatefulInteractiveElement, Styled as _, Window, div, px,
};
use gpui_base::ElementExt as _;
use gpui_base::motion::{Transition, transition};
use gpui_component::ActiveTheme as _;

/// Window-keyed hover state shared by one list's rows and its highlight.
pub(crate) struct GlideHover {
    rows: HashMap<SharedString, Bounds<Pixels>>,
    container: Option<Bounds<Pixels>>,
    hovered: Option<SharedString>,
    /// Bumps when hovering starts from nothing or the hovered row moves
    /// beneath the pointer; a fresh generation keys fresh transition
    /// clocks, which start settled at their target — a snap.
    generation: u64,
}

impl GlideHover {
    fn new() -> Self {
        Self {
            rows: HashMap::new(),
            container: None,
            hovered: None,
            generation: 0,
        }
    }
}

/// The list's shared glide state, keyed by the list's own identity.
pub(crate) fn glide_hover_state(
    list: ElementId,
    window: &mut Window,
    cx: &mut App,
) -> Entity<GlideHover> {
    window.use_keyed_state((list, "glide-hover"), cx, |_, _| GlideHover::new())
}

/// Captures the container's bounds so the highlight can position itself in
/// the container's own coordinates. The container must be `relative()`.
pub(crate) fn glide_frame<E>(frame: E, state: &Entity<GlideHover>) -> E
where
    E: gpui::ParentElement,
{
    let state = state.clone();
    frame.on_prepaint(move |bounds, _, cx| {
        // Geometry bookkeeping only: no notify, so a stationary frame
        // never schedules another render for having been measured.
        state.update(cx, |state, _| state.container = Some(bounds));
    })
}

/// Instruments one row: its bounds feed the highlight, and hovering it
/// moves the highlight here. The row should paint no hover fill of its
/// own.
pub(crate) fn glide_row<E>(row: E, key: SharedString, state: &Entity<GlideHover>) -> E
where
    E: StatefulInteractiveElement + gpui::ParentElement,
{
    let bounds_state = state.clone();
    let bounds_key = key.clone();
    let hover_state = state.clone();
    row.on_prepaint(move |bounds, _, cx| {
        bounds_state.update(cx, |state, _| {
            if state.hovered.as_ref() == Some(&bounds_key)
                && state.rows.get(&bounds_key) != Some(&bounds)
                && state.rows.contains_key(&bounds_key)
            {
                // The hovered row moved beneath a stationary pointer —
                // scrolling, most often. Snap with it rather than chase it.
                state.generation += 1;
            }
            state.rows.insert(bounds_key.clone(), bounds);
        });
    })
    .on_hover(move |hovered, _, cx| {
        hover_state.update(cx, |state, cx| {
            if *hovered {
                if state.hovered.is_none() {
                    // Arriving from nothing: appear at the row, no glide
                    // from wherever the highlight last rested.
                    state.generation += 1;
                }
                if state.hovered.as_ref() != Some(&key) {
                    state.hovered = Some(key.clone());
                    cx.notify();
                }
            } else if state.hovered.as_ref() == Some(&key) {
                state.hovered = None;
                cx.notify();
            }
        });
    })
}

/// The one highlight element. Mount it as an early child of the captured
/// `relative()` frame so rows and their content paint above it.
pub(crate) fn glide_highlight(
    list: ElementId,
    state: &Entity<GlideHover>,
    radius: Pixels,
    debug_selector: &'static str,
    window: &mut Window,
    cx: &mut App,
) -> Option<Div> {
    let (target, generation) = {
        let state = state.read(cx);
        let container = state.container?;
        let hovered = state.hovered.as_ref()?;
        let row = *state.rows.get(hovered)?;
        (
            Bounds::new(row.origin - container.origin, row.size),
            state.generation,
        )
    };

    // Between rows the highlight tweens on the glide tempo and the strong
    // ease-out; reduced motion collapses the tween to a snap. A fresh
    // generation keys fresh clocks that begin settled at the target.
    let duration = if crate::motion::motion_is_full(cx) {
        crate::motion::MotionTokens::read(cx).hover_glide()
    } else {
        std::time::Duration::ZERO
    };
    let mut channel = |name: &str, value: f32| {
        transition(
            ElementId::Name(format!("{list:?}-glide-{name}-{generation}").into()),
            value,
            Transition::new(duration).ease(crate::motion::ease_out_quint),
            window,
            cx,
        )
    };
    let x = channel("x", f32::from(target.origin.x));
    let y = channel("y", f32::from(target.origin.y));
    let w = channel("w", f32::from(target.size.width));
    let h = channel("h", f32::from(target.size.height));

    Some(
        div()
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(w))
            .h(px(h))
            .rounded(radius)
            .bg(cx.theme().list_hover)
            .debug_selector(move || debug_selector.to_owned()),
    )
}
