//! Shared surface and typography helpers.
//!
//! These are the crate's visual grammar in code: one card frame, one title
//! role, one description role, one eyebrow, one quiet icon button. Every
//! component builds from them so radii, spacing, weights, and hover treatment
//! stay identical across the library — and change in one place.

use crate::control::{PressReleaseExt as _, composed_button};
use crate::motion::VisibleAnimationExt as _;
use crate::theme::SemanticStyledExt as _;
use gpui::{
    App, Div, ElementId, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    Pixels, SharedString, Stateful, StatefulInteractiveElement as _, Styled as _, div,
    prelude::FluentBuilder as _,
};
use gpui_base::Button;
use gpui_component::{ActiveTheme as _, Icon, IconNamed, Sizable as _, v_flex};

/// The card's own surface: background, hairline border, and the card
/// radius — the three properties that make a thing look like a card,
/// without the layout a particular card wants.
///
/// Most of the library's cards are not [`card`]: they carry their own
/// padding and gaps, or they are buttons, or they are scroll containers.
/// They stated these three lines each instead, which is how a radius
/// change becomes a ten-file change.
pub(crate) trait CardFrameExt: Sized {
    /// Paints this element as a card: surface, hairline border, card radius.
    fn card_frame(self, cx: &App) -> Self;
}

impl<E: gpui::Styled + Sized> CardFrameExt for E {
    fn card_frame(self, cx: &App) -> Self {
        let tokens = cx.theme().semantic_tokens();
        self.bg(tokens.colors.surface)
            .border_1()
            .border_color(cx.theme().border)
            .rounded(tokens.radius.lg)
    }
}

/// The standard card: [`card_frame`] plus panel padding and a medium
/// content gap, for a card whose layout is a plain vertical stack.
pub(crate) fn card(id: impl Into<ElementId>, cx: &App) -> Stateful<Div> {
    let tokens = cx.theme().semantic_tokens();
    v_flex()
        .id(id)
        .w_full()
        .min_w_0()
        .gap(tokens.spacing.md)
        .p(tokens.spacing.lg)
        .card_frame(cx)
}

/// Loading rows that mirror the layout they stand in for: quiet muted
/// blocks in the coming shape, breathing on one shared clock so a whole
/// skeleton costs a single scheduled animation. Column widths vary
/// deterministically so the placeholder reads as content, not stripes.
/// A reduced motion preference holds the pulse at its middle; the caller
/// keeps the progress semantics (role and label) on its own frame.
pub(crate) fn skeleton_rows(
    id: impl Into<ElementId>,
    rows: usize,
    columns: usize,
    cx: &App,
) -> impl IntoElement {
    let tokens = cx.theme().semantic_tokens();
    let block = cx.theme().muted;
    let gap = tokens.spacing.lg;
    let row_gap = tokens.spacing.sm;
    let radius = tokens.radius.sm;
    let full = crate::motion::motion_is_full(cx);
    let spec = crate::motion::MotionTokens::read(cx).breathing();
    v_flex().w_full().gap(row_gap).with_visible_animation(
        id,
        spec.looping_synced(),
        move |body, delta| {
            let delta = if full { delta } else { 0.5 };
            // A triangle wave keeps the pulse symmetric; the band is
            // narrow so the skeleton stays quiet.
            let wave = (delta * 2.0 - 1.0).abs();
            let pulse = 0.55 + 0.3 * wave;
            body.opacity(pulse).children((0..rows).map(|row| {
                gpui::div()
                    .flex()
                    .w_full()
                    .items_center()
                    .gap(gap)
                    .children((0..columns).map(move |column| {
                        let fraction = 0.5 + 0.4 * (((row * 7 + column * 3) % 5) as f32 / 4.0);
                        gpui::div().flex_1().child(
                            gpui::div()
                                .h(gpui::rems(0.75))
                                .w(gpui::relative(fraction))
                                .rounded(radius)
                                .bg(block),
                        )
                    }))
            }))
        },
    )
}

/// The hairline a structural divider is drawn in: the border color at
/// reduced alpha, so a rule inside a bordered container separates
/// without competing with the frame, on any theme.
pub(crate) fn hairline(cx: &App) -> gpui::Hsla {
    cx.theme().border.opacity(0.6)
}

/// The empty-state anatomy every surface shares: a quiet icon, one line
/// that says why it is empty, and an optional hint — centered, padded,
/// never a bare string in a corner. Callers keep their own identity,
/// role, and status semantics on the wrapper they mount this into.
pub(crate) fn empty_state(
    icon: impl IconNamed,
    title: impl Into<SharedString>,
    note: Option<SharedString>,
    cx: &App,
) -> Div {
    let tokens = cx.theme().semantic_tokens();
    v_flex()
        .w_full()
        .items_center()
        .gap(tokens.spacing.xs)
        .p(tokens.spacing.lg)
        .child(
            Icon::new(icon)
                .small()
                .text_color(cx.theme().muted_foreground),
        )
        .child(
            div()
                .text_token(tokens.typography.sm)
                .text_color(cx.theme().foreground)
                .child(title.into()),
        )
        .when_some(note, |this, note| this.child(hint(note, cx)))
}

/// The nesting rule for a rounded surface inside a rounded container:
/// inner radius = container radius − inset, floored so a deep inset
/// cannot square the corner entirely.
pub(crate) fn nested_radius(container: Pixels, inset: Pixels, floor: Pixels) -> Pixels {
    if container > inset {
        (container - inset).max(floor)
    } else {
        floor
    }
}

/// The one selected-surface grammar for rows in lists and pickers.
///
/// An inset, rounded fill spanning the whole row — trailing controls
/// included — whose radius follows the nesting rule against the row's
/// container. Selection paints the theme's list-active token and keeps a
/// visible hover delta; unselected rows hover on the list-hover token.
/// Callers own semantics (aria_selected and friends) and content; this
/// owns only the surface, so every list that selects looks like one
/// family.
pub(crate) fn selection_surface<E>(
    row: E,
    selected: bool,
    container_radius: Pixels,
    inset: Pixels,
    cx: &App,
) -> E
where
    E: gpui::Styled + gpui::InteractiveElement,
{
    let tokens = cx.theme().semantic_tokens();
    let radius = nested_radius(container_radius, inset, tokens.radius.sm);
    let row = row.rounded(radius);
    if selected {
        row.bg(cx.theme().list_active)
            .hover(|style| style.bg(cx.theme().list_active.opacity(0.85)))
    } else {
        row.hover(|style| style.bg(cx.theme().list_hover))
    }
}

/// [`selection_surface`] for rows whose hover is the gliding highlight:
/// the same radius rule and selected fill, but no per-row hover paint —
/// the one highlight element is the hover.
pub(crate) fn selection_surface_glide<E>(
    row: E,
    selected: bool,
    container_radius: Pixels,
    inset: Pixels,
    cx: &App,
) -> E
where
    E: gpui::Styled + gpui::InteractiveElement,
{
    let tokens = cx.theme().semantic_tokens();
    let radius = nested_radius(container_radius, inset, tokens.radius.sm);
    let row = row.rounded(radius);
    if selected {
        row.bg(cx.theme().list_active)
    } else {
        row
    }
}

/// Seats a glyph beside wrappable text, centered on the text's first line.
///
/// The slot is a fixed square box whose side equals the first line's
/// line-height (the size policy's slot tokens name the two the crate
/// uses), so the row itself stays `items_start` and the glyph holds to
/// the first line however far the text wraps. Centering a bare glyph
/// against a whole text block is the misalignment class the 0.4.0 audit
/// found across eight components; a row that can wrap composes this
/// instead.
pub(crate) fn leading_glyph_slot(slot: Pixels, glyph: impl IntoElement) -> Div {
    div()
        .flex_none()
        .size(slot)
        .flex()
        .items_center()
        .justify_center()
        .child(glyph)
}

/// A compact inset panel placed *inside* a card (payloads, code, previews).
/// Uses the muted surface and the medium radius so it nests without a
/// second full-card frame.
pub(crate) fn inset(cx: &App) -> Div {
    let tokens = cx.theme().semantic_tokens();
    div()
        .w_full()
        .min_w_0()
        .p(tokens.spacing.md)
        .bg(cx.theme().muted.opacity(0.45))
        .rounded(tokens.radius.md)
}

/// Card or section title.
pub(crate) fn title(text: impl Into<SharedString>, cx: &App) -> Div {
    let tokens = cx.theme().semantic_tokens();
    div()
        .text_token(tokens.typography.md)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(cx.theme().foreground)
        .child(text.into())
}

/// Supporting prose under a title.
pub(crate) fn description(text: impl Into<SharedString>, cx: &App) -> Div {
    let tokens = cx.theme().semantic_tokens();
    div()
        .text_token(tokens.typography.sm)
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
}

/// A short, quiet section label above a group of content.
pub(crate) fn eyebrow(text: impl Into<SharedString>, cx: &App) -> Div {
    let tokens = cx.theme().semantic_tokens();
    div()
        .text_token(tokens.typography.xs)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
}

/// Small monospace metadata (durations, counts, identifiers).
pub(crate) fn meta(text: impl Into<SharedString>, cx: &App) -> Div {
    let tokens = cx.theme().semantic_tokens();
    div()
        .text_token(tokens.typography.xs)
        .font_family(cx.theme().mono_font_family.clone())
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
}

/// Quiet supporting text: extra-small, muted.
///
/// The most-used text role in the library, and the one that had no name —
/// a domain beside a title, a count beside a label, a hint under a field.
/// [`meta`] is its monospace sibling for values a reader may compare.
pub(crate) fn hint(text: impl Into<SharedString>, cx: &App) -> Div {
    let tokens = cx.theme().semantic_tokens();
    div()
        .text_token(tokens.typography.xs)
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
}

/// A heading inside a card: small, semibold, in the foreground ink.
///
/// Distinct from [`title`], which is a card's own name at the medium size;
/// this is the heading of a section within one.
pub(crate) fn subtitle(text: impl Into<SharedString>, cx: &App) -> Div {
    let tokens = cx.theme().semantic_tokens();
    div()
        .text_token(tokens.typography.sm)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(cx.theme().foreground)
        .child(text.into())
}

/// The trailing unit on a numeric field — "px", "%", "ms".
///
/// A bare string handed to an input's suffix inherits the field's own ink
/// and size, so the unit reads as part of the number. This gives every
/// unit in the library one quiet voice instead.
pub(crate) fn field_unit(unit: impl Into<SharedString>, cx: &App) -> Div {
    hint(unit, cx).flex_none()
}

/// A favicon-style badge: one uppercase initial on a primary tint. Sources,
/// search results, and attachments share it so provenance scans at a glance.
pub(crate) fn initial_badge(initial: impl Into<SharedString>, cx: &App) -> Div {
    let tokens = cx.theme().semantic_tokens();
    div()
        .flex_none()
        // The box is the type's own line box, from the size policy's slot
        // scale — a fixed pixel square held rem-scaled glyphs, so a larger
        // type scale pushed the letter out of its own circle.
        .size(crate::sizing::SizeTokens::read(cx).slot_sm())
        .overflow_hidden()
        .flex()
        .items_center()
        .justify_center()
        .rounded(tokens.radius.sm)
        .bg(cx.theme().primary.opacity(0.14))
        .text_token(tokens.typography.xs)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(cx.theme().primary)
        .child(initial.into())
}

/// First alphanumeric character of `text`, uppercased, for [`initial_badge`].
pub(crate) fn initial_of(text: &str) -> String {
    text.chars()
        .find(|character| character.is_alphanumeric())
        .map(|character| character.to_uppercase().collect())
        .unwrap_or_else(|| "•".to_owned())
}

/// A quiet, square icon-only button with an accessible name.
///
/// Rests muted, lifts to the foreground on hover, and shows the theme ring on
/// keyboard focus. Used for message actions and card toolbars.
pub(crate) fn icon_button(
    id: impl Into<ElementId>,
    icon: impl IconNamed,
    accessibility_label: impl Into<SharedString>,
    window: &mut gpui::Window,
    cx: &mut App,
) -> Button {
    let tokens = cx.theme().semantic_tokens();
    let id = id.into();
    composed_button(id.clone(), accessibility_label)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(crate::sizing::SizeTokens::read(cx).control_sm())
        .rounded(tokens.radius.sm)
        .border_1()
        .border_color(cx.theme().transparent)
        .text_color(cx.theme().muted_foreground)
        .hover(|style| {
            style
                .bg(cx.theme().accent.opacity(0.6))
                .text_color(cx.theme().accent_foreground)
        })
        .active(|style| style.bg(cx.theme().accent))
        .focus_visible(|style| style.border_color(cx.theme().ring))
        // A toggle that reports selected also shows it: the accent fill
        // stays while selected, so state stops being invisible.
        .styles(|styles| {
            styles.selected(|style| {
                style
                    .bg(cx.theme().accent)
                    .text_color(cx.theme().accent_foreground)
            })
        })
        .press_release(id.clone(), tokens.radius.sm, window, cx)
        .child(Icon::new(icon).xsmall().transform({
            // The glyph compresses while pressed and eases back on the
            // same release clock as the tint — SVG transforms are free,
            // so the compression costs no extra frames.
            let (pressed, fade) = crate::control::press_release_state(&id, window, cx);
            let intensity = if pressed { 1.0 } else { fade };
            let scale = 1.0 - 0.03 * intensity;
            gpui::Transformation::scale(gpui::size(scale, scale))
        }))
}

#[cfg(test)]
mod tests {
    use super::nested_radius;
    use gpui::px;

    #[test]
    fn the_nesting_rule_subtracts_the_inset_and_floors() {
        assert_eq!(nested_radius(px(8.), px(4.), px(3.)), px(4.));
        assert_eq!(nested_radius(px(8.), px(6.), px(3.)), px(3.), "floored");
        assert_eq!(
            nested_radius(px(6.), px(8.), px(3.)),
            px(3.),
            "inset past the corner"
        );
        assert_eq!(nested_radius(px(10.), px(2.), px(3.)), px(8.));
    }
}
