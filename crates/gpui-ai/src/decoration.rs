//! Layers an application paints into a component's frame.
//!
//! A component owns its structure; what fills the space behind and in front
//! of that structure is the application's business. This is the slot for it:
//! one element under the content, one over it, both clipped to the frame's
//! own shape, neither taking any part in layout.
//!
//! # What can go in one
//!
//! Anything that is an element. A `div` with a patterned background, an
//! `img` — including an animated GIF or WebP — a `canvas` painting arbitrary
//! geometry, or a webview. This crate ships no effects of its own: a shake, a
//! ripple, a halftone field are an application's expression, not a library's.
//!
//! # Example
//!
//! ```
//! use gpui_ai::prelude::Decoration;
//! use gpui::{div, prelude::*};
//!
//! let hatched = Decoration::behind(div().size_full());
//! let with_veil = hatched.and_above(div().size_full());
//! ```
//!
//! # Motion
//!
//! [`animated`] drives a decoration from a looping 0…1 and stops it when the
//! frame it sits in scrolls out of view or the reader has asked for less
//! motion. A decoration that animates on its own clock would keep a scrolled
//! away panel paying for itself forever.

use crate::motion::VisibleAnimationExt as _;
use gpui::{
    AnyElement, App, ElementId, Hsla, IntoElement, ParentElement, PathBuilder, Pixels, Styled,
    Window, canvas, div, point, px,
};
use std::time::Duration;

/// The layers an application paints into one component's frame.
///
/// Both slots are optional and independent. A decoration never affects the
/// component's size: the frame measures its content, and these are painted
/// into the space that measurement produced.
#[derive(Default)]
pub struct Decoration {
    behind: Option<AnyElement>,
    above: Option<AnyElement>,
}

impl Decoration {
    /// A layer painted under the component's content, over its background.
    pub fn behind(element: impl IntoElement) -> Self {
        Self {
            behind: Some(element.into_any_element()),
            above: None,
        }
    }

    /// A layer painted over the component's content.
    ///
    /// Passes pointer input through to the content underneath, so a veil, a
    /// scanline, or a vignette never costs the component a click.
    pub fn above(element: impl IntoElement) -> Self {
        Self {
            behind: None,
            above: Some(element.into_any_element()),
        }
    }

    /// Adds the layer painted over the content.
    pub fn and_above(mut self, element: impl IntoElement) -> Self {
        self.above = Some(element.into_any_element());
        self
    }

    /// Takes the under layer, wrapped in the frame's own shape.
    fn take_under(&mut self, radius: Pixels, frame: Hsla) -> Option<impl IntoElement> {
        self.behind
            .take()
            .map(|layer| clipped(layer, radius, frame))
    }

    /// Takes the over layer, wrapped in the frame's own shape.
    fn take_over(&mut self, radius: Pixels, frame: Hsla) -> Option<impl IntoElement> {
        self.above.take().map(|layer| clipped(layer, radius, frame))
    }
}

/// A layer filling its frame and clipped to the frame's corners.
fn clipped(layer: AnyElement, radius: Pixels, frame: Hsla) -> impl IntoElement {
    div()
        .absolute()
        .inset_0()
        .overflow_hidden()
        .child(layer)
        .child(corner_mask(radius, frame))
}

/// Paints the frame's own colour back into the four corners.
///
/// GPUI masks content with a rectangle — `ContentMask` is a `Bounds`, and
/// `overflow_hidden` clips to the box and never to a corner radius. So a
/// decoration drawn to the edge of a rounded component keeps its square
/// corners and shows outside the frame, which is what this slot spent three
/// releases quietly doing.
///
/// There is no rounded mask to ask for, so the corners are painted over
/// instead: for each one, the region between the square corner and the arc,
/// filled in the colour the frame would have shown there anyway. Exact for an
/// opaque frame, which every component here has.
fn corner_mask(radius: Pixels, frame: Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, (), window, _| {
            let r = f32::from(radius);
            if r <= 0.0 {
                return;
            }
            let (left, top) = (f32::from(bounds.origin.x), f32::from(bounds.origin.y));
            let right = left + f32::from(bounds.size.width);
            let bottom = top + f32::from(bounds.size.height);
            // Each corner: out to where the arc begins, round the arc, and
            // back. `sweep` flips with the winding so every corner is cut the
            // same way round.
            let corners = [
                ((left, top), (left + r, top), (left, top + r), true),
                ((right, top), (right, top + r), (right - r, top), true),
                (
                    (right, bottom),
                    (right - r, bottom),
                    (right, bottom - r),
                    true,
                ),
                ((left, bottom), (left, bottom - r), (left + r, bottom), true),
            ];
            for ((cx, cy), (from_x, from_y), (to_x, to_y), sweep) in corners {
                let mut path = PathBuilder::fill();
                path.move_to(point(px(cx), px(cy)));
                path.line_to(point(px(from_x), px(from_y)));
                let arc = px(r);
                path.arc_to(
                    point(arc, arc),
                    px(0.0),
                    false,
                    sweep,
                    point(px(to_x), px(to_y)),
                );
                path.close();
                if let Ok(path) = path.build() {
                    window.paint_path(path, frame);
                }
            }
        },
    )
}

/// Places a component's decoration layers.
///
/// Two calls rather than one, because paint order is child order: the under
/// layer has to be the frame's first child and the over layer its last. The
/// `every_framed_component_places_both_decoration_layers` test is what keeps
/// a component from quietly honouring only one of them.
pub(crate) trait DecoratedExt: Sized {
    /// Places the under layer. Call before adding any content.
    fn decoration_under(self, decoration: &mut Decoration, radius: Pixels, frame: Hsla) -> Self;

    /// Places the over layer. Call after adding all content.
    fn decoration_over(self, decoration: &mut Decoration, radius: Pixels, frame: Hsla) -> Self;
}

impl<E: ParentElement + Styled + Sized> DecoratedExt for E {
    fn decoration_under(
        mut self,
        decoration: &mut Decoration,
        radius: Pixels,
        frame: Hsla,
    ) -> Self {
        if let Some(layer) = decoration.take_under(radius, frame) {
            // The frame becomes the positioning context for both layers. Only
            // when there is one: a component with no decoration keeps exactly
            // the layout it had before this existed.
            self.style().position = Some(gpui::Position::Relative);
            self.extend(std::iter::once(layer.into_any_element()));
        }
        self
    }

    fn decoration_over(mut self, decoration: &mut Decoration, radius: Pixels, frame: Hsla) -> Self {
        if let Some(layer) = decoration.take_over(radius, frame) {
            self.style().position = Some(gpui::Position::Relative);
            self.extend(std::iter::once(layer.into_any_element()));
        }
        self
    }
}

/// A decoration that animates, and stops when nobody can see it.
///
/// `build` is handed a 0…1 that loops over `period`, and returns the element
/// to paint at that point in the loop. The loop runs only while the region is
/// on screen and unclipped, and holds at 0 when the reader has asked for less
/// motion — so an application's own effect answers to the same preference
/// every animation in this library does, without the application arranging it.
///
/// # Example
///
/// ```
/// use gpui_ai::prelude::decoration;
/// use gpui::{div, prelude::*, px};
/// use std::time::Duration;
///
/// let drifting = decoration::animated("tide", Duration::from_secs(8), |delta| {
///     div().absolute().top(px(delta * 40.)).size_full()
/// });
/// ```
pub fn animated<E>(
    id: impl Into<ElementId>,
    period: Duration,
    build: impl Fn(f32) -> E + 'static,
) -> impl IntoElement
where
    E: IntoElement + 'static,
{
    let id = id.into();
    // The animator receives the container, not the caller's element, so the
    // caller's closure runs once per frame and its result is the child.
    div().size_full().with_visible_animation(
        id,
        gpui::Animation::new(period).repeat(),
        move |container, delta| container.child(build(delta).into_any_element()),
    )
}

/// Drives a decoration from a value the application already has.
///
/// Where [`animated`] loops on its own, this eases towards `target` whenever
/// the application moves it — a ripple started by a press, a tilt that
/// follows a pointer, a shake that grows as a value approaches its limit.
/// Returns `target` immediately when the reader has asked for less motion.
pub fn toward(
    id: impl Into<gpui_base::motion::TransitionId>,
    target: f32,
    window: &mut Window,
    cx: &mut App,
) -> f32 {
    if !crate::motion::motion_is_full(cx) {
        return target;
    }
    let standard = crate::motion::MotionTokens::read(cx).standard();
    gpui_base::motion::transition(
        id,
        target,
        gpui_base::motion::Transition::new(standard).ease(crate::motion::ease_out_quint),
        window,
        cx,
    )
}
