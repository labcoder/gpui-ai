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
    /// Real frosted glass: the backdrop blurred, lined up with what is behind.
    Frosted,
    /// Translucency with no blur — the cheap look, named for what it is.
    Tint,
    /// Coloured light orbiting the frame, painted by the parent and the slot.
    Aurora,
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
        Self::Frosted,
        Self::Tint,
        Self::Aurora,
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
        ("frosted", "Frosted glass"),
        ("tint", "Tint"),
        ("aurora", "Aurora"),
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
            Self::Frosted => {
                "Actual frosted glass: the backdrop blurred on the CPU and \
                 placed by the same numbers as the sharp copy behind it, so \
                 the two line up. GPUI has no backdrop filter — this works \
                 because the parent knows where both are."
            }
            Self::Tint => {
                "Translucency and an edge highlight, no blur. Cheaper, honest \
                 about it, and what most panels actually need."
            }
            Self::Aurora => {
                "Coloured light travelling the frame. The parent paints the \
                 half that bleeds outside, the slot paints the half that \
                 falls inside — the slot clips, so neither could do it alone."
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
            Self::Frosted => Decoration::behind(frosted_panel(cx)),
            Self::Tint => Decoration::behind(tint_panel(cx)),
            Self::Aurora => Decoration::behind(aurora_inside(cx)),
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
        let photo = photo();
        let (width, height) = (photo.width as usize, photo.height as usize);
        let mut channels = photo.rgb.clone();
        for _ in 0..3 {
            box_blur(&mut channels, width, height, FROST_RADIUS, true);
            box_blur(&mut channels, width, height, FROST_RADIUS, false);
        }
        let mut buffer = ImageBuffer::new(photo.width, photo.height);
        for (index, pixel) in channels.as_chunks::<3>().0.iter().enumerate() {
            buffer.put_pixel(
                index as u32 % photo.width,
                index as u32 / photo.width,
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
fn photograph() -> gpui::Img {
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

/// Whether this state needs the story to paint a backdrop behind the card.
///
/// The frosted panel is only frosted glass if there is something behind it to
/// be out of focus, and it only lines up if the story places that something
/// from [`BACKDROP`] and [`CARD_INSET`].
pub(crate) fn needs_backdrop(kind: DecorationKind) -> bool {
    matches!(
        kind,
        DecorationKind::Frosted | DecorationKind::Tint | DecorationKind::Aurora
    )
}

/// The sharp photograph, at the size the blurred copy is placed against.
pub(crate) fn backdrop() -> impl IntoElement {
    div()
        .absolute()
        .inset_0()
        .child(photograph().w(px(BACKDROP.width)).h(px(BACKDROP.height)))
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
            img(frosted())
                .absolute()
                .left(px(-CARD_INSET.width))
                .top(px(-CARD_INSET.height))
                .w(px(BACKDROP.width))
                .h(px(BACKDROP.height)),
        )
        // Frost is not only blur: a little of the panel's own ground, and a
        // lit top edge where the light catches the bevel.
        .child(div().absolute().inset_0().bg(ground.opacity(0.45)))
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
                .bg(cx.theme().background.opacity(0.55)),
        )
        .child(edge_light(cx))
}

/// The lit top edge that makes a translucent panel read as a surface.
fn edge_light(cx: &App) -> impl IntoElement {
    div().absolute().inset_0().bg(linear_gradient(
        180.0,
        linear_color_stop(cx.theme().foreground.opacity(0.14), 0.0),
        linear_color_stop(cx.theme().foreground.opacity(0.0), 0.22),
    ))
}

/// The colours the aurora cycles through, in order.
const AURORA: [Hsla; 4] = [
    Hsla {
        h: 0.45,
        s: 0.85,
        l: 0.60,
        a: 1.0,
    },
    Hsla {
        h: 0.62,
        s: 0.85,
        l: 0.62,
        a: 1.0,
    },
    Hsla {
        h: 0.80,
        s: 0.80,
        l: 0.65,
        a: 1.0,
    },
    Hsla {
        h: 0.95,
        s: 0.85,
        l: 0.66,
        a: 1.0,
    },
];

/// Where a light sits on the frame at `phase`, as a fraction of each edge.
///
/// The perimeter walked as one loop, so a light travels corner to corner at an
/// even pace instead of jumping between edges.
fn on_perimeter(phase: f32, width: f32, height: f32) -> (f32, f32) {
    let perimeter = (width + height) * 2.0;
    let along = phase.rem_euclid(1.0) * perimeter;
    if along < width {
        (along, 0.0)
    } else if along < width + height {
        (width, along - width)
    } else if along < width * 2.0 + height {
        (width - (along - width - height), height)
    } else {
        (0.0, height - (along - width * 2.0 - height))
    }
}

/// A soft round glow, as an image.
///
/// GPUI has no radial gradient, and the obvious substitute — a big blurred
/// box-shadow on a transparent circle — paints the silhouette blurred, which
/// is a filled glow and very nearly right. Very nearly: at these sizes its
/// shader leaves a faint ring at the blur's edge, and four of them overlapping
/// on a dark stage draws visible arcs. A sprite has no such edge, costs one
/// small rasterisation per colour for the life of the process, and gives the
/// falloff curve to choose rather than inherit.
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
        let centre = f32::from(SIZE as u16) / 2.0;
        for y in 0..SIZE {
            for x in 0..SIZE {
                let dx = (x as f32 + 0.5 - centre) / centre;
                let dy = (y as f32 + 0.5 - centre) / centre;
                // Smoothstep on the radius: no hard edge at the rim, and a
                // shoulder near the centre so the light has a core.
                let distance = (dx * dx + dy * dy).sqrt().min(1.0);
                // Cubed rather than smoothstepped: a small bright core with
                // a long faint tail, which is what a light looks like.
                // Smoothstep holds half its alpha out to half its radius and
                // reads as a coloured ball with an edge.
                let fade = 1.0 - distance;
                let alpha = fade * fade * fade;
                // Premultiplied. `RenderImage` is composited as premultiplied
                // BGRA, so straight alpha renders every partly transparent
                // pixel darker than it should be — which draws a dark ring at
                // exactly the radius where the falloff passes through the
                // middle, and is visible as an outline around each light.
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

/// One soft coloured light, centred on a point.
///
/// An outer shadow on a transparent element is the silhouette blurred and
/// drawn behind it — a filled glow, which is wrong for a wavefront and exactly
/// right for this.
fn aurora_light(x: f32, y: f32, colour: Hsla, size: f32, alpha: f32) -> gpui::Div {
    div()
        .absolute()
        .left(px(x - size / 2.0))
        .top(px(y - size / 2.0))
        .size(px(size))
        .opacity(alpha)
        .child(img(glow_sprite(colour)).size(px(size)))
}

/// The half of the aurora that falls inside the component.
///
/// Clipped to the component's shape by the slot, which is what keeps it off
/// the corners. The other half is [`aurora_around`], and neither is the whole
/// effect.
fn aurora_inside(cx: &App) -> impl IntoElement {
    let _ = cx;
    decoration::animated("aurora-inside", Duration::from_millis(4200), |delta| {
        div()
            .size_full()
            .children(AURORA.iter().enumerate().map(move |(index, colour)| {
                let phase = delta + index as f32 / AURORA.len() as f32;
                let (x, y) = on_perimeter(phase, CARD.width, CARD.height);
                aurora_light(x, y, *colour, 170.0, 0.9)
            }))
    })
}

/// The half of the aurora that bleeds outside the component.
///
/// Drawn by the parent, because the decoration slot clips to the component and
/// a glow that leaves its edge cannot come from inside it. An application owns
/// the layout around its own components, so this is where it belongs — and the
/// pair of them is the answer to how far the slot reaches.
pub(crate) fn aurora_around() -> impl IntoElement {
    // Absolute here, not at the call site: `decoration::animated` returns an
    // in-flow `div().size_full()`, which is right inside the slot — the slot
    // positions it — and wrong out here, where it is a sibling of the
    // component and would take layout space from it.
    div().absolute().inset_0().child(decoration::animated(
        "aurora-around",
        Duration::from_millis(4200),
        |delta| {
            div()
                .absolute()
                .inset_0()
                .children(AURORA.iter().enumerate().map(move |(index, colour)| {
                    let phase = delta + index as f32 / AURORA.len() as f32;
                    let (x, y) = on_perimeter(phase, CARD.width, CARD.height);
                    aurora_light(
                        x + CARD_INSET.width,
                        y + CARD_INSET.height,
                        *colour,
                        210.0,
                        0.8,
                    )
                }))
        },
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
