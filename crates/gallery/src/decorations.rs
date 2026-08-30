//! Six decorations, written the way an application would write them.
//!
//! None of this lives in `gpui-ai`. The library provides the slot and the
//! motion channel; what fills them is expression, and expression belongs to
//! whoever is building the thing. These exist to prove the slot is worth
//! having, and to be read by someone deciding what to put in their own.
//!
//! Every animated one goes through [`gpui_ai::prelude::decoration::animated`],
//! so it stops when it scrolls out of view and holds still under a
//! reduced-motion preference.
//!
//! ## Why there is a photograph in here
//!
//! Four of these process a real image. Dithering and posterising are answers
//! to a question — how do you show a continuous tone with very few inks — and
//! a synthetic gradient does not ask it: it has no grain, no specular
//! highlights, and no dark field for the pattern to open up against. The
//! photograph is 75kB and public domain; see `assets/README.md`.
//!
//! ## Cost
//!
//! Quantising a 640x370 photograph is a quarter of a million pixels of work,
//! which must never happen while drawing. It happens twice: the image is
//! decoded to luminance once for the life of the process, and a treatment is
//! rasterised again only when the palette it was built for changes — which is
//! to say, on a theme switch. Everything between those is a cached
//! `RenderImage` handed to `img()`.

use gpui::{
    App, Hsla, IntoElement, ParentElement as _, Rgba, Styled as _, div, img, linear_color_stop,
    linear_gradient, px, relative,
};
use gpui::{BoxShadow, RenderImage, point};
use gpui::{ObjectFit, StyledImage as _};
use gpui_ai::prelude::{Decoration, decoration};
use gpui_component::ActiveTheme as _;
use image::{Frame, ImageBuffer, Rgba as ImageRgba};
use std::{cell::RefCell, sync::Arc, sync::OnceLock, time::Duration};

/// Which decoration the story is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum DecorationKind {
    /// The photograph as it is, with nothing between it and the content.
    #[default]
    Photo,
    /// The same photograph under a fixed dark scrim, so the text is readable.
    Scrim,
    /// A photograph reduced to four inks by an ordered dither.
    Dither,
    /// The same photograph flattened to flat bands of colour.
    PopArt,
    /// The photograph as an engraving: tone carried by crossing ink.
    Engrave,
    /// A grid of dots that breathe.
    Halftone,
    /// Rings that travel out from a press.
    Ripple,
    /// Rings that keep coming, on a clock rather than a press.
    Pulse,
    /// A gradient veil over the content rather than under it.
    Veil,
}

impl DecorationKind {
    pub(crate) const ALL: &'static [Self] = &[
        Self::Photo,
        Self::Scrim,
        Self::Dither,
        Self::PopArt,
        Self::Engrave,
        Self::Halftone,
        Self::Ripple,
        Self::Pulse,
        Self::Veil,
    ];

    pub(crate) const LABELS: &'static [(&'static str, &'static str)] = &[
        ("photo", "Photo"),
        ("scrim", "Photo + scrim"),
        ("dither", "Dither"),
        ("pop-art", "Pop art"),
        ("engrave", "Cross-hatch"),
        ("halftone", "Halftone"),
        ("ripple", "Ripple"),
        ("pulse", "Pulse"),
        ("veil", "Veil"),
    ];

    pub(crate) fn index(self) -> usize {
        Self::ALL.iter().position(|kind| *kind == self).unwrap_or(0)
    }

    /// What this one is doing, in a line.
    pub(crate) fn note(self) -> &'static str {
        match self {
            Self::Photo => {
                "The photograph with nothing between it and the words. Its own                  colours, not the theme's — a decoration is the application's,                  and nothing here has to follow the palette. Also the reason                  the next one exists."
            }
            Self::Scrim => {
                "The same photograph under a fixed dark scrim — a flat black                  at sixty per cent, chosen by hand and identical in every                  theme. The commonest thing anyone will actually want."
            }
            Self::Dither => {
                "A photograph quantised to four inks by an 8x8 ordered dither, \
                 in the theme's own colours. Rasterised on a theme change, \
                 never while drawing."
            }
            Self::PopArt => {
                "The same photograph, flattened to three flat bands with no \
                 dither between them — the print the dither is avoiding."
            }
            Self::Engrave => {
                "Cross-hatching whose density follows the photograph's tone,                  the way an engraver carries shade — ink laid down or not,                  never half."
            }
            Self::Halftone => {
                "A grid of dots on a travelling wave, on the library's motion \
                 channel. It stops when the panel scrolls out of view."
            }
            Self::Ripple => {
                "Rings driven from a press rather than a clock. The library \
                 eases the value; the rings are the application's own drawing."
            }
            Self::Pulse => {
                "The same rings on the motion channel instead of a press, so                  they keep arriving — and stop when the panel scrolls away."
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
            Self::Photo => Decoration::behind(photograph()),
            Self::Scrim => Decoration::behind(
                div()
                    .size_full()
                    .child(photograph())
                    // Fixed, not from the theme: this is what an application
                    // reaches for when it wants one look everywhere.
                    .child(div().absolute().inset_0().bg(SCRIM)),
            ),
            Self::Dither => Decoration::behind(processed(Treatment::Dither, cx)),
            Self::PopArt => Decoration::behind(processed(Treatment::PopArt, cx)),
            Self::Engrave => Decoration::behind(processed(Treatment::Engrave, cx)),
            Self::Halftone => Decoration::behind(under_content(halftone(cx), cx)),
            Self::Ripple => Decoration::behind(rings(ripple, cx)),
            Self::Pulse => Decoration::behind(pulse(cx)),
            Self::Veil => Decoration::above(veil(cx)),
        }
    }
}

/// How a photograph is reduced to very few inks.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Treatment {
    /// Ordered dither: the eye mixes the inks back into a tone.
    Dither,
    /// Hard bands: the same few inks with nothing between them.
    PopArt,
    /// Crossing ink, dense where the photograph is dark.
    Engrave,
}

/// The photograph, decoded to one luminance byte per pixel.
///
/// Decoded once for the life of the process. Luminance rather than colour
/// because every treatment here maps tone onto the theme's palette — keeping
/// the original hues would put a fixed picture behind a themed component and
/// lose to it in every theme but the one it happened to suit.
struct Photo {
    width: u32,
    height: u32,
    luminance: Vec<u8>,
}

fn photo() -> &'static Photo {
    static PHOTO: OnceLock<Photo> = OnceLock::new();
    PHOTO.get_or_init(|| {
        let decoded = image::load_from_memory(include_bytes!("../assets/carina-nebula.jpg"))
            .expect("the bundled photograph must decode")
            .to_luma8();
        Photo {
            width: decoded.width(),
            height: decoded.height(),
            luminance: decoded.into_raw(),
        }
    })
}

/// The classic 8x8 ordered-dither threshold matrix, as sixty-fourths.
///
/// Recursively defined, but written out: it is a constant, and generating it
/// at startup would trade a table anyone can read for a function nobody
/// checks.
#[rustfmt::skip]
const BAYER: [u8; 64] = [
     0, 32,  8, 40,  2, 34, 10, 42,
    48, 16, 56, 24, 50, 18, 58, 26,
    12, 44,  4, 36, 14, 46,  6, 38,
    60, 28, 52, 20, 62, 30, 54, 22,
     3, 35, 11, 43,  1, 33,  9, 41,
    51, 19, 59, 27, 49, 17, 57, 25,
    15, 47,  7, 39, 13, 45,  5, 37,
    63, 31, 55, 23, 61, 29, 53, 21,
];

/// The scrim over the plain photograph: flat black at sixty per cent.
///
/// Written as a literal rather than taken from the theme, because that is the
/// question this state exists to answer — a decoration is the application's,
/// and nothing about the slot requires it to follow the palette. Every other
/// image state here does follow it, by choice rather than by rule.
const SCRIM: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.0,
    a: 0.6,
};

/// How many inks a treatment reduces the photograph to.
const INKS: u16 = 4;

/// The last treatment rasterised, and what it was rasterised for.
///
/// One entry, replaced rather than accumulated: the only thing that changes
/// the answer is the theme, and a gallery has one theme at a time.
type Rasterised = (Treatment, u32, u32, Arc<RenderImage>);

thread_local! {
    static LAST: RefCell<Option<Rasterised>> = const { RefCell::new(None) };
}

/// The photograph as it was taken, in its own colours.
///
/// No quantising and no cache: GPUI decodes and holds this one itself, which
/// is all an application needs when it is not processing the pixels.
fn photograph() -> impl IntoElement {
    img(Arc::new(gpui::Image::from_bytes(
        gpui::ImageFormat::Jpeg,
        include_bytes!("../assets/carina-nebula.jpg").to_vec(),
    )))
    .size_full()
    .object_fit(ObjectFit::Cover)
}

/// The photograph under `treatment`, in the theme's own two colours.
fn processed(treatment: Treatment, cx: &App) -> impl IntoElement {
    let ground = Rgba::from(cx.theme().background);
    let ink = Rgba::from(cx.theme().primary);
    let key = (treatment, packed(ground), packed(ink));

    let image = LAST.with(|last| {
        let mut last = last.borrow_mut();
        if let Some((cached_treatment, cached_ground, cached_ink, image)) = last.as_ref()
            && (*cached_treatment, *cached_ground, *cached_ink) == key
        {
            return Arc::clone(image);
        }
        let built = rasterise(treatment, ground, ink);
        *last = Some((key.0, key.1, key.2, Arc::clone(&built)));
        built
    });

    under_content(img(image).size_full().object_fit(ObjectFit::Cover), cx)
}

/// Puts a decoration under the content with a wash between the two.
///
/// Everything here is painted over the component's own background and under
/// its text, so at full strength it wins and the text loses — which is not
/// boldness, it is a broken card. A wash of the component's own ground, heavy
/// where the reading starts and light where it ends, keeps the effect at full
/// strength and the words on top of it legible.
///
/// The library cannot do this for an application: it does not know what is
/// being painted or where the text will fall. Doing it here, once, is the
/// example worth copying.
fn under_content(layer: impl IntoElement, cx: &App) -> impl IntoElement {
    let ground = cx.theme().background;
    div()
        .size_full()
        .child(layer)
        .child(div().absolute().inset_0().bg(linear_gradient(
            90.0,
            linear_color_stop(ground.opacity(0.94), 0.0),
            linear_color_stop(ground.opacity(0.55), 1.0),
        )))
}

/// A colour as one comparable number, so a cache key is cheap and exact.
fn packed(colour: Rgba) -> u32 {
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0) as u32;
    (channel(colour.r) << 24)
        | (channel(colour.g) << 16)
        | (channel(colour.b) << 8)
        | channel(colour.a)
}

/// Quantises the photograph to [`INKS`] levels and paints it in two colours.
///
/// BGRA, because that is what `RenderImage` holds; writing it in the order the
/// name suggests puts the sky in the wrong colour, which is the sort of thing
/// that looks like a theme bug for a week.
fn rasterise(treatment: Treatment, ground: Rgba, ink: Rgba) -> Arc<RenderImage> {
    let photo = photo();
    let top = f32::from(INKS - 1);
    let mut buffer = ImageBuffer::new(photo.width, photo.height);
    for (index, tone) in photo.luminance.iter().enumerate() {
        let x = index as u32 % photo.width;
        let y = index as u32 / photo.width;
        let value = f32::from(*tone) / 255.0;
        // The dither nudges each pixel by where it sits in the 8x8 tile before
        // rounding, so a tone between two inks lands on one or the other in a
        // pattern the eye reads back as the tone between them. Pop art skips
        // the nudge, which is the whole difference between the two.
        let nudged = match treatment {
            Treatment::Dither => {
                let threshold = f32::from(BAYER[((y & 7) * 8 + (x & 7)) as usize]) / 64.0;
                value + (threshold - 0.5) / top
            }
            // Pop art and engraving take the tone as it is: one has no
            // in-between inks to reach for, the other carries shade with the
            // density of its lines instead.
            Treatment::PopArt | Treatment::Engrave => value,
        };
        let level = match treatment {
            // An engraver has no half-tones: ink is either laid down or not,
            // and shade comes from how many directions of it cross. Spacing
            // in device pixels, because that is what a line is.
            Treatment::Engrave => {
                let shade = 1.0 - value;
                let down = (x + y).is_multiple_of(7);
                let up = (x + 7 - (y % 7)).is_multiple_of(7);
                let tight = (x + y).is_multiple_of(4);
                let inked =
                    (shade > 0.40 && down) || (shade > 0.62 && up) || (shade > 0.84 && tight);
                if inked { 1.0 } else { 0.0 }
            }
            _ => (nudged.clamp(0.0, 1.0) * top).round() / top,
        };
        let mix = |from: f32, to: f32| ((from + (to - from) * level) * 255.0) as u8;
        buffer.put_pixel(
            x,
            y,
            ImageRgba([
                mix(ground.b, ink.b),
                mix(ground.g, ink.g),
                mix(ground.r, ink.r),
                255,
            ]),
        );
    }
    Arc::new(RenderImage::new([Frame::new(buffer)]))
}

/// Dots on a grid whose size breathes, so the field reads as a texture that
/// is alive rather than a picture of one.
fn halftone(cx: &App) -> impl IntoElement {
    const COLUMNS: usize = 22;
    const ROWS: usize = 9;
    let ink = cx.theme().primary.opacity(0.55);
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
                    div().size(px(3.0 + wave * 13.0)).rounded_full().bg(ink)
                }))
        }))
    })
}

/// Rings travelling out from the centre, sized by a value the application
/// moves rather than by a clock the decoration owns.
fn rings(progress: f32, cx: &App) -> impl IntoElement {
    let ink = cx.theme().primary;
    div().size_full().children((0..4).map(move |ring| {
        // Each ring trails the one before it, so a single value produces a
        // sequence rather than four circles doing the same thing.
        let travel = (progress - ring as f32 * 0.16).clamp(0.0, 1.0);
        // At rest one ring stays, faintly, so the state reads as waiting for a
        // press rather than as an effect that failed to draw.
        if progress <= f32::EPSILON {
            let resting = if ring == 0 { 0.30 } else { 0.0 };
            return lit_circle(0.0, resting, ink);
        }
        lit_circle(travel, (1.0 - travel) * 0.9, ink)
    }))
}

/// Rings that keep arriving, driven by the clock instead of a press.
///
/// The same drawing as the ripple; only where the value comes from differs,
/// which is the whole reason the two sit next to each other.
fn pulse(cx: &App) -> impl IntoElement {
    let ink = cx.theme().primary;
    decoration::animated("pulse", Duration::from_millis(2600), move |delta| {
        div().size_full().children((0..3).map(move |ring| {
            // Thirds of a cycle apart, so one is always arriving as another
            // leaves — a rhythm rather than a flash.
            let travel = (delta + ring as f32 / 3.0).fract();
            lit_circle(travel, (1.0 - travel) * 0.75, ink)
        }))
    })
}

/// A ring that reads as a curved front rather than a drawn outline.
///
/// GPUI has no radial gradient and no blur filter, so depth has to come from
/// the shadow: an inset one lights the inner edge the way a surface catches
/// light, and an outer one blurs further the wider the ring grows, as a
/// wavefront loses definition while it spreads. Sized in pixels because a
/// decorated card is far wider than it is tall, and a fraction of both axes
/// is an ellipse.
fn lit_circle(travel: f32, alpha: f32, ink: Hsla) -> gpui::Div {
    const REST: f32 = 56.0;
    const REACH: f32 = 620.0;
    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .size(px(REST + travel * REACH))
                .rounded_full()
                // Inset only. An outer shadow on a transparent element is
                // the whole silhouette blurred and drawn behind it, which
                // fills the circle in — a glowing disc, not a wavefront. The
                // inset one hugs the edge, and the border keeps it defined
                // once the blur has spread out at full travel.
                .border_2()
                .border_color(ink.opacity(alpha * 0.55))
                .shadow(vec![BoxShadow {
                    color: ink.opacity(alpha),
                    offset: point(px(0.0), px(0.0)),
                    blur_radius: px(12.0),
                    spread_radius: px(0.0),
                    inset: true,
                }]),
        )
}

/// A gradient across the content: the over layer, proving it passes input
/// through to everything it covers.
fn veil(cx: &App) -> impl IntoElement {
    let ink: Hsla = cx.theme().primary;
    div().size_full().bg(linear_gradient(
        // Down and across, so it reads as light falling rather than a band.
        135.0,
        linear_color_stop(ink.opacity(0.0), 0.35),
        linear_color_stop(ink.opacity(0.42), 1.0),
    ))
}

#[cfg(test)]
mod tests {
    use super::DecorationKind;
    use crate::story::{DECORATION_STORY_VARIANTS, StoryId};

    /// The switcher this story draws and the list an address is parsed against
    /// are two tables that have to say the same thing. They are declared apart
    /// because `story.rs` is what the web bridge reads and must not depend on
    /// how the story is drawn — so nothing but this notices them drifting, and
    /// a drift means a link naming a state silently opens a different one.
    #[test]
    fn the_switcher_and_the_addressable_states_are_the_same_list() {
        assert_eq!(DecorationKind::LABELS, DECORATION_STORY_VARIANTS);
        assert_eq!(DecorationKind::ALL.len(), DecorationKind::LABELS.len());
        assert_eq!(StoryId::Decorations.variants(), DECORATION_STORY_VARIANTS);
    }

    /// Every kind has to be reachable from the slug an address carries.
    #[test]
    fn every_kind_sits_at_the_index_its_label_does() {
        for (position, kind) in DecorationKind::ALL.iter().enumerate() {
            assert_eq!(kind.index(), position);
            assert!(!kind.note().is_empty(), "every state explains itself");
        }
    }
}
