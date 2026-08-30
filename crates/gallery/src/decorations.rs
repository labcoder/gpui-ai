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
    App, Hsla, IntoElement, ParentElement as _, PathBuilder, Pixels, Rgba, Styled as _, Window,
    canvas, div, img, linear_color_stop, linear_gradient, point, px, relative,
};
use gpui::{BoxShadow, RenderImage};
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
    /// Real frosted glass: the backdrop blurred, lined up with what is behind.
    Frosted,
    /// Glass proper: the backdrop lensed and lifted at the rim.
    Glass,
    /// Translucency with no blur — the cheap look, named for what it is.
    Tint,
    /// Two bright trails travelling the frame, glowing both ways.
    Beam,
    /// The same trails, throwing their light into the panel only.
    BeamInward,
    /// The same trails, throwing their light outward only.
    BeamOutward,
    /// A metallic sheen turning around the frame.
    Metal,
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
    /// Rings that keep coming, on a clock rather than a press.
    Pulse,
    /// A gradient veil over the content rather than under it.
    Veil,
}

impl DecorationKind {
    pub(crate) const ALL: &'static [Self] = &[
        Self::Photo,
        Self::Frosted,
        Self::Glass,
        Self::Tint,
        Self::Beam,
        Self::BeamInward,
        Self::BeamOutward,
        Self::Metal,
        Self::Scrim,
        Self::Dither,
        Self::PopArt,
        Self::Engrave,
        Self::Halftone,
        Self::Pulse,
        Self::Veil,
    ];

    pub(crate) const LABELS: &'static [(&'static str, &'static str)] = &[
        ("photo", "Photo"),
        ("frosted", "Frosted"),
        ("glass", "Glass"),
        ("tint", "Tint"),
        ("beam", "Border beam"),
        ("beam-inward", "Beam · inward"),
        ("beam-outward", "Beam · outward"),
        ("metal", "Liquid metal"),
        ("scrim", "Photo + scrim"),
        ("dither", "Dither"),
        ("pop-art", "Pop art"),
        ("engrave", "Cross-hatch"),
        ("halftone", "Halftone"),
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
            Self::Frosted => {
                "Actual frosted glass: the backdrop blurred on the CPU and \
                 placed by the same numbers as the sharp copy behind it, so \
                 the two line up. GPUI has no backdrop filter — this works \
                 because the parent knows where both are."
            }
            Self::Glass => {
                "Glass rather than frost: a convex bevel, Snell's law across \
                 its normal, and the view under the rim squeezed where the \
                 surface tips over. Frosted blurs what it covers; this lenses \
                 it."
            }
            Self::Tint => {
                "Translucency and an edge highlight, no blur. Cheaper, honest \
                 about it, and what most panels actually need."
            }
            Self::Beam => {
                "Two trails stroked along the rounded path itself rather than \
                 positioned near it, so they turn the corners instead of \
                 cutting them. The glow is thirty overlapping lights per \
                 trail: five you could count, thirty sum into a band."
            }
            Self::BeamInward => {
                "The same trails with the light pushed into the panel. A lamp \
                 behind frosted glass rather than a tube around it — the same \
                 code, one argument different."
            }
            Self::BeamOutward => {
                "And pushed the other way, which is neon. Worth having all \
                 three: the trail is the cheap part, and where its light \
                 falls is what decides what the frame is made of."
            }
            Self::Metal => {
                "The same stroke with a metallic ramp instead of a beam. Metal \
                 is not a light: it snaps from dark to bright and back, \
                 because it reflects a small bright thing rather than emitting."
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
    pub(crate) fn build(self, cx: &App) -> Decoration {
        match self {
            Self::Photo => Decoration::behind(photograph(cx)),
            Self::Frosted => Decoration::behind(frosted_panel(cx)),
            Self::Glass => Decoration::behind(glass_panel(cx)),
            Self::Tint => Decoration::behind(tint_panel(cx)),
            Self::Scrim => Decoration::behind(
                div()
                    .size_full()
                    .child(photograph(cx))
                    // Fixed, not from the theme: this is what an application
                    // reaches for when it wants one look everywhere.
                    .child(div().absolute().inset_0().rounded(shape(cx)).bg(SCRIM)),
            ),
            Self::Dither => Decoration::behind(processed(Treatment::Dither, cx)),
            Self::PopArt => Decoration::behind(processed(Treatment::PopArt, cx)),
            Self::Engrave => Decoration::behind(processed(Treatment::Engrave, cx)),
            Self::Halftone => Decoration::behind(under_content(halftone(cx), cx)),
            Self::Pulse => Decoration::behind(pulse(cx)),
            // Nothing in the slot. A stroke sits astride the component's
            // edge, and the slot clips to the component — so the whole effect
            // is the parent's, and saying so here is the point.
            Self::Beam | Self::BeamInward | Self::BeamOutward | Self::Metal => {
                Decoration::default()
            }
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
    /// Straight RGB, kept for the one decoration that blurs rather than
    /// quantises: frosted glass is the picture out of focus, and a tone map
    /// of it is a different thing entirely.
    rgb: Vec<u8>,
}

/// The size the photograph is drawn at wherever it appears as a backdrop.
///
/// Fixed, and public to the story, because the glass decoration only lines up
/// with what is behind it if both are placed from the same numbers. See
/// [`BACKDROP`].
pub(crate) const BACKDROP: gpui::Size<f32> = gpui::Size {
    width: 560.0,
    height: 320.0,
};

/// The decorated card's size inside the backdrop.
pub(crate) const CARD: gpui::Size<f32> = gpui::Size {
    width: 420.0,
    height: 168.0,
};

/// Where the card sits inside the backdrop.
///
/// The blurred copy is shifted by exactly this, which is the only reason it
/// lines up with the sharp one.
pub(crate) const CARD_INSET: gpui::Size<f32> = gpui::Size {
    width: (BACKDROP.width - CARD.width) / 2.0,
    height: (BACKDROP.height - CARD.height) / 2.0,
};

fn photo() -> &'static Photo {
    static PHOTO: OnceLock<Photo> = OnceLock::new();
    PHOTO.get_or_init(|| {
        let decoded = image::load_from_memory(include_bytes!("../assets/carina-nebula.jpg"))
            .expect("the bundled photograph must decode")
            .to_rgb8();
        Photo {
            width: decoded.width(),
            height: decoded.height(),
            luminance: image::DynamicImage::ImageRgb8(decoded.clone())
                .to_luma8()
                .into_raw(),
            rgb: decoded.into_raw(),
        }
    })
}

/// How far the frosted decoration blurs the picture behind it, in pixels.
const FROST_RADIUS: usize = 14;

/// The photograph out of focus, as a texture that can be drawn behind a
/// translucent panel.
///
/// Three box blurs, which is the usual stand-in for a Gaussian and is what
/// makes this cheap enough to do on the CPU at all. Done once and cached: it
/// does not depend on the theme, so unlike the quantised treatments it never
/// needs redoing.
fn frosted() -> Arc<RenderImage> {
    static FROSTED: OnceLock<Arc<RenderImage>> = OnceLock::new();
    Arc::clone(FROSTED.get_or_init(|| {
        let (width, height) = (BACKDROP.width as usize, BACKDROP.height as usize);
        let mut channels = stage_pixels(Stage::Photo).clone();
        for _ in 0..3 {
            box_blur(&mut channels, width, height, FROST_RADIUS, true);
            box_blur(&mut channels, width, height, FROST_RADIUS, false);
        }
        let mut buffer = ImageBuffer::new(BACKDROP.width as u32, BACKDROP.height as u32);
        for (index, pixel) in channels.as_chunks::<3>().0.iter().enumerate() {
            buffer.put_pixel(
                index as u32 % BACKDROP.width as u32,
                index as u32 / BACKDROP.width as u32,
                // BGRA, as everywhere `RenderImage` is built by hand.
                ImageRgba([pixel[2], pixel[1], pixel[0], 255]),
            );
        }
        Arc::new(RenderImage::new([Frame::new(buffer)]))
    }))
}

/// One separable box-blur pass over an RGB buffer, in place.
///
/// A running sum rather than a window average, so the cost is one add and one
/// subtract per pixel regardless of radius — the difference between a blur
/// that is cheap at fourteen pixels and one that is not.
fn box_blur(data: &mut [u8], width: usize, height: usize, radius: usize, horizontal: bool) {
    let (lanes, span) = if horizontal {
        (height, width)
    } else {
        (width, height)
    };
    let stride = if horizontal { 3 } else { width * 3 };
    let mut lane = vec![0u8; span * 3];
    for index in 0..lanes {
        let base = if horizontal {
            index * width * 3
        } else {
            index * 3
        };
        for step in 0..span {
            let at = base + step * stride;
            lane[step * 3..step * 3 + 3].copy_from_slice(&data[at..at + 3]);
        }
        for channel in 0..3 {
            let mut sum = 0u32;
            let window = radius * 2 + 1;
            for step in 0..=radius.min(span - 1) {
                sum += u32::from(lane[step * 3 + channel]);
            }
            sum += u32::from(lane[channel]) * radius as u32;
            for step in 0..span {
                let at = base + step * stride;
                data[at + channel] = (sum / window as u32) as u8;
                let leaving = step.saturating_sub(radius);
                let arriving = (step + radius + 1).min(span - 1);
                sum += u32::from(lane[arriving * 3 + channel]);
                sum -= u32::from(lane[leaving * 3 + channel]);
            }
        }
    }
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
/// The radius a decoration layer has to round itself to.
///
/// The slot clips rectangularly, because GPUI's content mask is a `Bounds` and
/// there is no rounded one to ask for. So every layer here that paints to the
/// edge carries the component's own radius: an image and a background both
/// clip themselves in the shader, which is the only rounded clipping actually
/// on offer. A layer that never reaches a corner does not need it.
fn shape(cx: &App) -> Pixels {
    cx.theme().semantic_tokens().radius.lg
}

fn photograph(cx: &App) -> gpui::Img {
    img(Arc::new(gpui::Image::from_bytes(
        gpui::ImageFormat::Jpeg,
        include_bytes!("../assets/carina-nebula.jpg").to_vec(),
    )))
    .size_full()
    .object_fit(ObjectFit::Cover)
    .rounded(shape(cx))
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

    under_content(
        img(image)
            .size_full()
            .object_fit(ObjectFit::Cover)
            .rounded(shape(cx)),
        cx,
    )
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
        .child(
            div()
                .absolute()
                .inset_0()
                .rounded(shape(cx))
                .bg(linear_gradient(
                    90.0,
                    linear_color_stop(ground.opacity(0.94), 0.0),
                    linear_color_stop(ground.opacity(0.55), 1.0),
                )),
        )
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

/// Whether this state needs the story to paint a backdrop behind the card.
///
/// The frosted panel is only frosted glass if there is something behind it to
/// be out of focus, and it only lines up if the story places that something
/// from [`BACKDROP`] and [`CARD_INSET`].
pub(crate) fn needs_backdrop(kind: DecorationKind) -> bool {
    matches!(
        kind,
        DecorationKind::Frosted
            | DecorationKind::Glass
            | DecorationKind::Tint
            | DecorationKind::Beam
            | DecorationKind::Metal
    )
}

/// The sharp photograph, at the size the blurred copy is placed against.
pub(crate) fn backdrop(stage: Stage) -> impl IntoElement {
    static SHARP: OnceLock<Vec<(bool, Arc<RenderImage>)>> = OnceLock::new();
    let built = SHARP.get_or_init(|| {
        [Stage::Photo, Stage::Rule]
            .into_iter()
            .map(|which| (which == Stage::Rule, rasterise_stage(which)))
            .collect()
    });
    let image = Arc::clone(
        &built
            .iter()
            .find(|(is_rule, _)| *is_rule == (stage == Stage::Rule))
            .expect("both stages are built")
            .1,
    );
    div()
        .absolute()
        .inset_0()
        .child(img(image).w(px(BACKDROP.width)).h(px(BACKDROP.height)))
}

/// One stage's pixels as an image, drawn at exactly the size sampled.
fn rasterise_stage(stage: Stage) -> Arc<RenderImage> {
    let pixels = stage_pixels(stage);
    {
        let mut buffer = ImageBuffer::new(BACKDROP.width as u32, BACKDROP.height as u32);
        for (index, pixel) in pixels.as_chunks::<3>().0.iter().enumerate() {
            buffer.put_pixel(
                index as u32 % BACKDROP.width as u32,
                index as u32 / BACKDROP.width as u32,
                ImageRgba([pixel[2], pixel[1], pixel[0], 255]),
            );
        }
        Arc::new(RenderImage::new([Frame::new(buffer)]))
    }
}

/// A point on the perimeter of a rounded rectangle, at `t` of the way round.
///
/// Walked as one loop — four straights and four quarter arcs, in order — so a
/// beam travels at an even pace and turns the corners instead of jumping
/// between edges. This is the whole reason the border effects are drawn as
/// paths rather than positioned as boxes: a box can be put near a corner, but
/// only a path goes round one.
fn perimeter_point(t: f32, w: f32, h: f32, r: f32) -> (f32, f32) {
    // Clamped before anything divides by it. The inward glow strokes a path
    // inset by more than the corner radius, which leaves a rectangle with no
    // corners at all — and a quarter arc of length zero is what put a NaN
    // into the path builder and took the whole window down with it.
    let r = r.clamp(0.0, w.min(h) / 2.0);
    let straight_x = (w - r * 2.0).max(0.0);
    let straight_y = (h - r * 2.0).max(0.0);
    let quarter = std::f32::consts::FRAC_PI_2 * r;
    let total = (straight_x + straight_y + quarter * 2.0) * 2.0;
    let mut along = t.rem_euclid(1.0) * total;

    let arc = |centre_x: f32, centre_y: f32, from: f32, span: f32, along: f32| {
        // Zero when there is no arc to walk. An inset past the corner
        // radius leaves a rectangle, and dividing a zero-length arc by its
        // own length is what put a NaN into the path and took the window
        // down before it ever appeared.
        let travelled = if quarter > f32::EPSILON {
            along / quarter
        } else {
            0.0
        };
        let angle = from + span * travelled;
        (centre_x + r * angle.cos(), centre_y + r * angle.sin())
    };
    let pi = std::f32::consts::PI;

    if along < straight_x {
        return (r + along, 0.0);
    }
    along -= straight_x;
    if along < quarter {
        return arc(w - r, r, -pi / 2.0, pi / 2.0, along);
    }
    along -= quarter;
    if along < straight_y {
        return (w, r + along);
    }
    along -= straight_y;
    if along < quarter {
        return arc(w - r, h - r, 0.0, pi / 2.0, along);
    }
    along -= quarter;
    if along < straight_x {
        return (w - r - along, h);
    }
    along -= straight_x;
    if along < quarter {
        return arc(r, h - r, pi / 2.0, pi / 2.0, along);
    }
    along -= quarter;
    if along < straight_y {
        return (0.0, h - r - along);
    }
    along -= straight_y;
    arc(r, r, pi, pi / 2.0, along)
}

/// How many pieces a colour ramp around the border is stroked in.
///
/// Each piece is one flat colour. GPUI's gradients carry exactly two stops —
/// `colors: [LinearColorStop; 2]`, a fixed array handed to the shader — so a
/// ramp of more colours than that has to be built out of pieces, and the only
/// way a ramp reads as light rather than as tiling is for each piece to be
/// small enough that its neighbour is imperceptibly different.
///
/// Ninety-six was not. On a card's perimeter that is a step every twelve
/// pixels, which against a sheen that moves quickly is a visible block. At
/// four hundred and eighty it is a step every two and a half, under the width
/// of the stroke itself.
const BORDER_STEPS: usize = 480;

/// The points one run of the border walks, in order.
///
/// Split out from the stroking so a test can walk exactly what the painter
/// walks. Guessing at plausible parameters is what let a NaN through the last
/// time: the numbers that broke it were the ones the beam actually uses, and
/// a test that invented its own never saw them.
fn run_points(
    size: (f32, f32),
    radius: f32,
    inset: f32,
    from: f32,
    to: f32,
    samples: usize,
) -> impl Iterator<Item = (f32, f32)> {
    let radius = (radius - inset).max(0.0);
    (0..=samples).map(move |step| {
        let t = from + (to - from) * step as f32 / samples as f32;
        perimeter_point(t, size.0, size.1, radius)
    })
}

/// Strokes part of the perimeter in one flat colour, as a single path.
///
/// `inset` shrinks the path it follows, which is how a glow is biased inward:
/// a stroke is centred on its path, so a wide one on a path pulled inside the
/// frame spills mostly into the component instead of evenly both ways.
///
/// One path rather than a run of short ones, and that matters. Consecutive
/// stroked paths meet with butt ends that neither quite abut nor quite
/// overlap, and at nine pixels wide the joins read as notches all the way
/// round. Only a colour that changes along the path needs to be cut up.
#[allow(clippy::too_many_arguments)]
fn stroke_run(
    window: &mut Window,
    box_: gpui::Bounds<Pixels>,
    radius: f32,
    inset: f32,
    from: f32,
    to: f32,
    width: f32,
    colour: Hsla,
    samples: usize,
) {
    if colour.a <= 0.002 || width <= 0.0 {
        return;
    }
    let origin = (
        f32::from(box_.origin.x) + inset,
        f32::from(box_.origin.y) + inset,
    );
    let size = (
        f32::from(box_.size.width) - inset * 2.0,
        f32::from(box_.size.height) - inset * 2.0,
    );
    if size.0 <= 0.0 || size.1 <= 0.0 {
        return;
    }
    let mut path = PathBuilder::stroke(px(width));
    for (step, (x, y)) in run_points(size, radius, inset, from, to, samples).enumerate() {
        let at = point(px(origin.0 + x), px(origin.1 + y));
        if step == 0 {
            path.move_to(at);
        } else {
            path.line_to(at);
        }
    }
    if let Ok(path) = path.build() {
        window.paint_path(path, colour);
    }
}

/// A round glow with a soft falloff, as an image.
///
/// The one thing none of GPUI's primitives will do. A quad has a crisp edge, a
/// stroke is a band with two crisp edges and butt ends, and there is no radial
/// gradient and no blur. A sprite has the falloff drawn into it, and costs one
/// small rasterisation per colour for the life of the process.
///
/// Premultiplied, because `RenderImage` composites that way: straight alpha
/// renders every partly transparent pixel too dark and rings the glow.
fn glow_sprite(colour: Hsla) -> Arc<RenderImage> {
    thread_local! {
        static SPRITES: RefCell<Vec<(u32, Arc<RenderImage>)>> = const { RefCell::new(Vec::new()) };
    }
    const SIZE: u32 = 128;
    let rgba = Rgba::from(colour);
    let key = packed(rgba);
    SPRITES.with(|sprites| {
        let mut sprites = sprites.borrow_mut();
        if let Some((_, sprite)) = sprites.iter().find(|(cached, _)| *cached == key) {
            return Arc::clone(sprite);
        }
        let mut buffer = ImageBuffer::new(SIZE, SIZE);
        let centre = SIZE as f32 / 2.0;
        for y in 0..SIZE {
            for x in 0..SIZE {
                let dx = (x as f32 + 0.5 - centre) / centre;
                let dy = (y as f32 + 0.5 - centre) / centre;
                let distance = (dx * dx + dy * dy).sqrt().min(1.0);
                let fade = 1.0 - distance;
                let alpha = fade * fade * fade;
                buffer.put_pixel(
                    x,
                    y,
                    ImageRgba([
                        (rgba.b * alpha * 255.0) as u8,
                        (rgba.g * alpha * 255.0) as u8,
                        (rgba.r * alpha * 255.0) as u8,
                        (alpha * 255.0) as u8,
                    ]),
                );
            }
        }
        let sprite = Arc::new(RenderImage::new([Frame::new(buffer)]));
        sprites.push((key, Arc::clone(&sprite)));
        sprite
    })
}

/// Where a beam's glow is allowed to fall.
///
/// The same trail reads as three different materials depending on this, which
/// is why it is a choice rather than a constant: light thrown into the panel
/// is a lamp behind frosted glass, light thrown outward is a neon tube, and
/// light confined to the line is an LED strip.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Spill {
    /// Into the component and out of it, evenly.
    Both,
    /// Into the component only.
    Inward,
    /// Out of the component only.
    Outward,
}

/// How many lights make up one trail's glow.
///
/// The number is the whole difference between a glow and a row of spotlights.
/// Five lights spaced along a fifth of the perimeter are five circles you can
/// count; thirty at the same total length overlap by most of their width, and
/// what you see is where they sum — a continuous band whose colour slides
/// along it, which is what the reference actually shows.
const TRAIL_LIGHTS: usize = 30;

/// The glow one trail throws.
fn trail_glow(head: f32, radius: f32, spill: Spill) -> impl IntoElement {
    const REACH: f32 = 96.0;
    div()
        .absolute()
        .inset_0()
        .children((0..TRAIL_LIGHTS).map(move |light| {
            let along = light as f32 / (TRAIL_LIGHTS - 1) as f32;
            let t = head - BEAM_LENGTH * (1.0 - along);
            let (x, y) = perimeter_point(t, CARD.width, CARD.height, radius);
            // Brightest at the head and gone at the tail, so the trail has a
            // direction rather than being a lit arc.
            let strength = along * along * along;
            // Pushed off the line for the one-sided spills. A light centred on
            // the border spills equally; moved half its own width inward, the
            // frame's own clip takes most of the outward half.
            let (nx, ny) = outward_normal(x, y, CARD.width, CARD.height);
            let shift = match spill {
                Spill::Both => 0.0,
                Spill::Inward => -REACH * 0.34,
                Spill::Outward => REACH * 0.34,
            };
            let (cx_, cy_) = (x + nx * shift, y + ny * shift);
            let alpha = strength * 0.5;
            div()
                .absolute()
                .left(px(cx_ - REACH / 2.0))
                .top(px(cy_ - REACH / 2.0))
                .size(px(REACH))
                .opacity(alpha)
                .child(img(glow_sprite(beam_colour(along))).size(px(REACH)))
        }))
}

/// Which way is out of the frame at a point on its perimeter.
///
/// Only ever axis-aligned: a light near a corner is on one of the two edges
/// that meet there, and pushing it along that edge's normal is close enough at
/// the size these are drawn.
fn outward_normal(x: f32, y: f32, width: f32, height: f32) -> (f32, f32) {
    let gaps = [x, width - x, y, height - y];
    let nearest = gaps
        .iter()
        .copied()
        .enumerate()
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map_or(0, |(index, _)| index);
    match nearest {
        0 => (-1.0, 0.0),
        1 => (1.0, 0.0),
        2 => (0.0, -1.0),
        _ => (0.0, 1.0),
    }
}

/// Which border effect a state is drawing.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Border {
    /// Trails travelling the frame, throwing light where `Spill` says.
    Beam(Spill),
    /// A metallic sheen turning around the frame.
    Metal,
}

/// How much of the perimeter the beam occupies.
const BEAM_LENGTH: f32 = 0.22;

/// The colours the beam runs through, head to tail.
const BEAM_COLOURS: [Hsla; 4] = [
    Hsla {
        h: 0.86,
        s: 0.90,
        l: 0.70,
        a: 1.0,
    },
    Hsla {
        h: 0.74,
        s: 0.85,
        l: 0.68,
        a: 1.0,
    },
    Hsla {
        h: 0.58,
        s: 0.85,
        l: 0.65,
        a: 1.0,
    },
    Hsla {
        h: 0.42,
        s: 0.80,
        l: 0.62,
        a: 1.0,
    },
];

/// Reads the beam palette at `along`, 0 at the tail and 1 at the head.
fn beam_colour(along: f32) -> Hsla {
    let scaled = along.clamp(0.0, 1.0) * (BEAM_COLOURS.len() - 1) as f32;
    let index = (scaled.floor() as usize).min(BEAM_COLOURS.len() - 2);
    let mix = scaled - index as f32;
    let (from, to) = (BEAM_COLOURS[index], BEAM_COLOURS[index + 1]);
    Hsla {
        h: from.h + (to.h - from.h) * mix,
        s: from.s + (to.s - from.s) * mix,
        l: from.l + (to.l - from.l) * mix,
        a: 1.0,
    }
}

/// The metallic ramp: two bright bands a half turn apart, on a dark body.
///
/// What makes a surface read as metal rather than as a light is the falloff.
/// A lamp fades smoothly; polished metal snaps from dark to bright and back,
/// because it is reflecting a small bright thing rather than emitting.
fn metal_colour(along: f32) -> Hsla {
    let turns = along.rem_euclid(1.0) * 2.0 * std::f32::consts::TAU;
    // Fourth power rather than sixth: steep enough to read as a reflection
    // rather than a lamp, shallow enough that the ramp is not crossing most
    // of its range inside one piece.
    let sheen = (turns.cos() * 0.5 + 0.5).powi(4);
    Hsla {
        h: 0.62,
        s: 0.10,
        l: 0.28 + sheen * 0.68,
        a: 1.0,
    }
}

/// How many pieces the beam's coloured core is cut into.
///
/// Only the core, because only the core changes colour along its length. The
/// wide passes are single paths, which is what stopped them showing seams.
const CORE_STEPS: usize = 28;

/// Draws the whole border for one frame.
fn paint_border(
    window: &mut Window,
    bounds: gpui::Bounds<Pixels>,
    kind: Border,
    phase: f32,
    radius: f32,
) {
    let resting = Hsla {
        h: 0.0,
        s: 0.0,
        l: 1.0,
        a: 0.10,
    };
    // The frame itself, in one unbroken stroke.
    stroke_run(window, bounds, radius, 0.0, 0.0, 1.0, 1.0, resting, 320);

    match kind {
        Border::Beam(_) => {
            let head = phase;
            let tail = head - BEAM_LENGTH;
            // The core carries the colour, so it is the one thing cut up.
            for trail in 0..2 {
                let tail = tail + trail as f32 * 0.5;
                for step in 0..CORE_STEPS {
                    let along = step as f32 / CORE_STEPS as f32;
                    let from = tail + BEAM_LENGTH * along;
                    let to = tail + BEAM_LENGTH * (step as f32 + 1.3) / CORE_STEPS as f32;
                    let colour = beam_colour(along);
                    // Fades at both ends, so the beam has a head and a tail rather
                    // than two cut edges.
                    let strength = (along * (1.0 - along) * 4.0).clamp(0.0, 1.0);
                    stroke_run(
                        window,
                        bounds,
                        radius,
                        0.0,
                        from,
                        to,
                        2.0,
                        colour.opacity(strength),
                        4,
                    );
                }
            }
        }
        Border::Metal => {
            // No inward glow: metal reflects, it does not throw light. That is
            // the whole difference between the two, and the reason they are
            // separate states rather than one with a switch.
            for step in 0..BORDER_STEPS {
                let from = step as f32 / BORDER_STEPS as f32;
                let to = (step as f32 + 1.3) / BORDER_STEPS as f32;
                let colour = metal_colour(from - phase);
                stroke_run(
                    window,
                    bounds,
                    radius,
                    0.0,
                    from,
                    to,
                    2.0,
                    colour.opacity(0.95),
                    4,
                );
            }
        }
    }
}

/// A border effect, drawn by the parent around the component it frames.
///
/// Out here rather than in the slot because a stroke sits astride the edge —
/// half of it falls outside the component, and the slot clips to the
/// component's shape. An application owns the layout around its own
/// components, so this is where a border belongs.
pub(crate) fn border_effect(kind: Border, radius: f32) -> impl IntoElement {
    let (id, period) = match kind {
        Border::Beam(_) => ("border-beam", Duration::from_millis(3200)),
        Border::Metal => ("border-metal", Duration::from_millis(5200)),
    };
    div()
        .absolute()
        .left(px(CARD_INSET.width))
        .top(px(CARD_INSET.height))
        .w(px(CARD.width))
        .h(px(CARD.height))
        .child(decoration::animated(id, period, move |delta| {
            let layer = div().size_full();
            let layer = match kind {
                Border::Beam(spill) => layer
                    .child(trail_glow(delta, radius, spill))
                    // A second trail, opposite the first. One travelling line
                    // reads as a loading spinner; two reads as a frame that is
                    // alive, which is what the reference does.
                    .child(trail_glow(delta + 0.5, radius, spill)),
                Border::Metal => layer,
            };
            layer.child(
                canvas(
                    |_, _, _| (),
                    move |bounds, (), window, _| paint_border(window, bounds, kind, delta, radius),
                )
                .absolute()
                .inset_0(),
            )
        }))
}

/// The backdrop resampled to exactly the size it is drawn at.
///
/// Everything that reads the backdrop — the blurred copy, the lens — samples
/// this rather than the original file, so there is no `object_fit` arithmetic
/// between what is sampled and what is shown. One buffer, one size, no way for
/// the two to disagree.
/// What a state puts behind the card.
///
/// Two, for a reason worth stating: refraction is only visible on something
/// straight. A photograph has no straight lines, so a lens over one bends
/// nothing you can see and reads as a magnifier. Ruled lines are what make a
/// material legible — which is why every glass demo worth looking at, bezel's
/// included, has a switcher full of rulers and text rather than photographs.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage {
    /// The photograph. What blur is best shown against.
    Photo,
    /// Ruled lines. What refraction is best shown against.
    Rule,
}

/// Which backdrop a state wants behind it.
pub(crate) fn stage_for(kind: DecorationKind) -> Stage {
    match kind {
        DecorationKind::Glass => Stage::Rule,
        _ => Stage::Photo,
    }
}

/// The border effect a state draws around its component, if any.
pub(crate) fn border_for(kind: DecorationKind) -> Option<Border> {
    match kind {
        DecorationKind::Beam => Some(Border::Beam(Spill::Both)),
        DecorationKind::BeamInward => Some(Border::Beam(Spill::Inward)),
        DecorationKind::BeamOutward => Some(Border::Beam(Spill::Outward)),
        DecorationKind::Metal => Some(Border::Metal),
        _ => None,
    }
}

/// Evenly ruled lines, the width of the stage.
fn ruled_pixels() -> &'static Vec<u8> {
    static RULE: OnceLock<Vec<u8>> = OnceLock::new();
    RULE.get_or_init(|| {
        let (width, height) = (BACKDROP.width as usize, BACKDROP.height as usize);
        let mut out = vec![0u8; width * height * 3];
        for y in 0..height {
            // Close enough together that the rim crosses several of them, so
            // the bend is read as a curve rather than as one line moved.
            let horizontal = y % 13 < 2;
            for x in 0..width {
                let vertical = x % 78 < 2;
                let value: u8 = match (horizontal, vertical) {
                    (true, _) => 208,
                    (_, true) => 120,
                    _ => 16,
                };
                let at = (y * width + x) * 3;
                out[at..at + 3].copy_from_slice(&[value, value, value]);
            }
        }
        out
    })
}

fn stage_pixels(stage: Stage) -> &'static Vec<u8> {
    if stage == Stage::Rule {
        return ruled_pixels();
    }
    static STAGE: OnceLock<Vec<u8>> = OnceLock::new();
    STAGE.get_or_init(|| {
        let photo = photo();
        let (width, height) = (BACKDROP.width as usize, BACKDROP.height as usize);
        let mut out = vec![0u8; width * height * 3];
        for y in 0..height {
            for x in 0..width {
                let from = (
                    x as f32 * photo.width as f32 / width as f32,
                    y as f32 * photo.height as f32 / height as f32,
                );
                let pixel = sample_rgb(&photo.rgb, photo.width, photo.height, from.0, from.1);
                let at = (y * width + x) * 3;
                out[at..at + 3].copy_from_slice(&pixel);
            }
        }
        out
    })
}

/// Bilinear sample of an RGB buffer, clamped at the edges.
fn sample_rgb(data: &[u8], width: u32, height: u32, x: f32, y: f32) -> [u8; 3] {
    let x = x.clamp(0.0, width as f32 - 1.001);
    let y = y.clamp(0.0, height as f32 - 1.001);
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let (x1, y1) = (
        (x0 + 1).min(width as usize - 1),
        (y0 + 1).min(height as usize - 1),
    );
    let mut out = [0u8; 3];
    for channel in 0..3 {
        let at = |cx: usize, cy: usize| f32::from(data[(cy * width as usize + cx) * 3 + channel]);
        let top = at(x0, y0) * (1.0 - fx) + at(x1, y0) * fx;
        let bottom = at(x0, y1) * (1.0 - fx) + at(x1, y1) * fx;
        out[channel] = (top * (1.0 - fy) + bottom * fy) as u8;
    }
    out
}

/// Signed distance from `p` to a rounded rectangle centred on the origin.
///
/// Negative inside. The whole lens is built on this: how deep a pixel sits
/// under the surface is what decides how much the glass bends what is under
/// it, and the shape of a card is a rounded rectangle, not a circle.
fn rounded_rect_sdf(px: f32, py: f32, half_w: f32, half_h: f32, radius: f32) -> f32 {
    let qx = px.abs() - (half_w - radius);
    let qy = py.abs() - (half_h - radius);
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    outside + qx.max(qy).min(0.0) - radius
}

/// How thick the lensed rim is, in pixels.
const BEVEL: f32 = 20.0;

/// Refractive index of the glass. Air is 1.
const INDEX: f32 = 1.5;

/// How far, at most, a ray is pushed sideways at the rim.
const LENS_REACH: f32 = 42.0;

/// How much the glass magnifies what is under it.
const MAGNIFY: f32 = 1.09;

/// The backdrop as seen through a lens the shape of the card.
///
/// This is the part that makes it glass rather than a blur. Following the
/// construction in <https://kube.io/blog/liquid-glass-css-svg/>: a convex
/// bevel gives the surface a height, its derivative gives a normal, Snell's
/// law bends the ray across that normal, and the pixel is sampled from where
/// the bent ray lands instead of from straight down. Deep inside the card the
/// surface is flat and nothing bends; within [`BEVEL`] of the edge the normal
/// tips over and the view compresses, which is the lensing you see at the rim
/// of anything actually made of glass.
///
/// Rasterised once, because none of it depends on the theme.
fn lensed() -> Arc<RenderImage> {
    static LENSED: OnceLock<Arc<RenderImage>> = OnceLock::new();
    Arc::clone(LENSED.get_or_init(|| {
        let stage = stage_pixels(Stage::Rule);
        let (stage_w, stage_h) = (BACKDROP.width as u32, BACKDROP.height as u32);
        let (card_w, card_h) = (CARD.width, CARD.height);
        let (half_w, half_h) = (card_w / 2.0, card_h / 2.0);
        let radius = 12.0;
        // Up and to the left, so the lit rim reads as lit from above.
        let diagonal = std::f32::consts::FRAC_1_SQRT_2;
        let (light_x, light_y) = (-diagonal, -diagonal);
        let mut buffer = ImageBuffer::new(card_w as u32, card_h as u32);
        for y in 0..card_h as u32 {
            for x in 0..card_w as u32 {
                let (px, py) = (x as f32 + 0.5 - half_w, y as f32 + 0.5 - half_h);
                let depth = -rounded_rect_sdf(px, py, half_w, half_h, radius);
                // 0 at the rim, 1 once the surface has flattened out.
                let across = (depth / BEVEL).clamp(0.0, 1.0);
                // Convex circle: the profile that keeps rays inside the glass.
                let height = |t: f32| (1.0 - (1.0 - t).powi(2)).max(0.0).sqrt();
                let step = 0.002;
                let slope = (height(across + step) - height(across - step)) / (2.0 * step);
                // The incident ray is straight down, so the angle it makes
                // with the normal is the angle the surface has tipped.
                let incidence = slope.atan();
                let refracted = (incidence.sin() / INDEX).asin();
                let bend = (incidence - refracted).tan() * LENS_REACH;
                // Outward normal of the shape, by central difference.
                let grad =
                    |dx: f32, dy: f32| rounded_rect_sdf(px + dx, py + dy, half_w, half_h, radius);
                let (nx, ny) = (
                    grad(0.5, 0.0) - grad(-0.5, 0.0),
                    grad(0.0, 0.5) - grad(0.0, -0.5),
                );
                let len = (nx * nx + ny * ny).sqrt().max(1e-5);
                let (nx, ny) = (nx / len, ny / len);
                // Sample from further out than the pixel sits: the rim shows a
                // squeezed view of what lies just beyond the glass.
                let sx = px / MAGNIFY + nx * bend;
                let sy = py / MAGNIFY + ny * bend;
                let pixel = sample_rgb(
                    stage,
                    stage_w,
                    stage_h,
                    sx + half_w + CARD_INSET.width,
                    sy + half_h + CARD_INSET.height,
                );
                // A rim light, from the same normal the refraction used.
                let facing = (nx * light_x + ny * light_y).max(0.0);
                let rim = (1.0 - across).powi(2) * facing;
                let lift = |channel: u8| {
                    let value = f32::from(channel) / 255.0;
                    ((value + rim * 1.15).min(1.0) * 255.0) as u8
                };
                buffer.put_pixel(
                    x,
                    y,
                    ImageRgba([lift(pixel[2]), lift(pixel[1]), lift(pixel[0]), 255]),
                );
            }
        }
        Arc::new(RenderImage::new([Frame::new(buffer)]))
    }))
}

/// Glass: the backdrop lensed at the rim rather than merely blurred.
fn glass_panel(cx: &App) -> impl IntoElement {
    div()
        .size_full()
        .child(img(lensed()).absolute().inset_0().rounded(shape(cx)))
        // A breath of the panel's own ground, and the lit top edge. Glass is
        // not colourless; it lifts what is under it.
        .child(
            div()
                .absolute()
                .inset_0()
                .rounded(shape(cx))
                .bg(cx.theme().background.opacity(0.16)),
        )
        .child(edge_light(cx))
}

/// The blurred backdrop, placed so it lines up with the sharp one behind.
///
/// This is the whole trick, and it is worth being plain about why it works.
/// GPUI has no backdrop filter: an element cannot ask for what is behind it.
/// What it can do is draw a blurred copy of a backdrop the application already
/// has — and if the copy is drawn at the same size and offset as the original,
/// the result is indistinguishable from a real one, because it is the same
/// pixels out of focus.
///
/// The offset is the catch. This decoration is clipped to the component, and
/// `inset_0` inside a card is not `inset_0` inside the story behind it, so the
/// two only agree if the parent places both from the same numbers. It does:
/// see [`BACKDROP`] and the story's stage. That is also the honest limit — it
/// works for a backdrop the application can rasterise and position, not for
/// arbitrary live content underneath.
fn frosted_panel(cx: &App) -> impl IntoElement {
    let ground = cx.theme().background;
    div()
        .size_full()
        .child(
            // Deliberately unrounded: this one is larger than the slot and
            // positioned by offset, so its corners are nowhere near the
            // frame's. The wash and the edge light above it carry the shape.
            img(frosted())
                .absolute()
                .left(px(-CARD_INSET.width))
                .top(px(-CARD_INSET.height))
                .w(px(BACKDROP.width))
                .h(px(BACKDROP.height)),
        )
        // Frost is not only blur: a little of the panel's own ground, and a
        // lit top edge where the light catches the bevel.
        .child(
            div()
                .absolute()
                .inset_0()
                .rounded(shape(cx))
                .bg(ground.opacity(0.45)),
        )
        .child(edge_light(cx))
}

/// Translucency with no blur at all.
///
/// The same panel treatment with the blurred copy left out, so the two states
/// next to each other show exactly what the blur is worth.
fn tint_panel(cx: &App) -> impl IntoElement {
    div()
        .size_full()
        .child(
            div()
                .absolute()
                .inset_0()
                .rounded(shape(cx))
                .bg(cx.theme().background.opacity(0.55)),
        )
        .child(edge_light(cx))
}

/// The lit top edge that makes a translucent panel read as a surface.
fn edge_light(cx: &App) -> impl IntoElement {
    div()
        .absolute()
        .inset_0()
        .rounded(shape(cx))
        .bg(linear_gradient(
            180.0,
            linear_color_stop(cx.theme().foreground.opacity(0.14), 0.0),
            linear_color_stop(cx.theme().foreground.opacity(0.0), 0.22),
        ))
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
    div().size_full().rounded(shape(cx)).bg(linear_gradient(
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

    /// Every point the beam and the metal actually put into a path builder.
    ///
    /// Walks the painter's own parameters — the same insets, the same run
    /// bounds, the same sample counts — over a full turn of the animation.
    /// The first version of this test invented its own numbers and passed
    /// while the gallery was panicking on startup, which is the entire reason
    /// it shares `run_points` with the painter now rather than guessing.
    #[test]
    fn no_border_run_produces_a_point_a_path_will_reject() {
        let size = (super::CARD.width, super::CARD.height);
        let radius = 12.0_f32;
        // The frame's own run, plus a sweep of insets: nothing insets the
        // path today, but `stroke_run` still takes one, and an inset past the
        // corner radius is exactly what produced the NaN.
        let runs = [
            (0.0_f32, 320_usize),
            (6.0, 96),
            (12.0, 96),
            (30.0, 96),
            (60.0, 96),
        ];
        for turn in 0..240 {
            let phase = turn as f32 / 240.0;
            let head = phase;
            let tail = head - super::BEAM_LENGTH;
            for (inset, samples) in runs {
                for (x, y) in super::run_points(size, radius, inset, tail, head, samples) {
                    assert!(
                        x.is_finite() && y.is_finite(),
                        "beam inset {inset} phase {phase} gave ({x}, {y})"
                    );
                }
            }
            for piece in 0..super::BORDER_STEPS {
                let from = piece as f32 / super::BORDER_STEPS as f32;
                let to = (piece as f32 + 1.3) / super::BORDER_STEPS as f32;
                for (x, y) in super::run_points(size, radius, 0.0, from, to, 4) {
                    assert!(x.is_finite() && y.is_finite(), "metal gave ({x}, {y})");
                }
            }
        }
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
