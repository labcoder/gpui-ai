//! Shared surface and typography helpers.
//!
//! These are the crate's visual grammar in code: one card frame, one title
//! role, one description role, one eyebrow, one quiet icon button. Every
//! component builds from them so radii, spacing, weights, and hover treatment
//! stay identical across the library — and change in one place.

use crate::control::{PressReleaseExt as _, composed_button};
use crate::theme::SemanticStyledExt as _;
use gpui::{
    App, Div, ElementId, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    Pixels, SharedString, Stateful, StatefulInteractiveElement as _, Styled as _, div,
    prelude::FluentBuilder as _,
};
use gpui_base::Button;
use gpui_component::{ActiveTheme as _, Icon, IconNamed, Sizable as _, v_flex};

/// The standard card: surface background, hairline border, large radius,
/// panel padding, and a medium content gap.
pub(crate) fn card(id: impl Into<ElementId>, cx: &App) -> Stateful<Div> {
    let tokens = cx.theme().semantic_tokens();
    v_flex()
        .id(id)
        .w_full()
        .min_w_0()
        .gap(tokens.spacing.md)
        .p(tokens.spacing.lg)
        .bg(tokens.colors.surface)
        .border_1()
        .border_color(cx.theme().border)
        .rounded(tokens.radius.lg)
}

/// A compact inset panel placed *inside* a card (payloads, code, previews).
/// Uses the muted surface and the medium radius so it nests without a
/// second full-card frame.
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
    hint: Option<SharedString>,
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
        .when_some(hint, |this, hint| {
            this.child(
                div()
                    .text_token(tokens.typography.xs)
                    .text_color(cx.theme().muted_foreground)
                    .child(hint),
            )
        })
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

/// A favicon-style badge: one uppercase initial on a primary tint. Sources,
/// search results, and attachments share it so provenance scans at a glance.
pub(crate) fn initial_badge(initial: impl Into<SharedString>, cx: &App) -> Div {
    let tokens = cx.theme().semantic_tokens();
    div()
        .flex_none()
        .size_4()
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
                .bg(cx.theme().accent)
                .text_color(cx.theme().accent_foreground)
        })
        .active(|style| style.bg(cx.theme().accent.opacity(0.8)))
        .focus_visible(|style| style.border_color(cx.theme().ring))
        .press_release(id, tokens.radius.sm, window, cx)
        .child(Icon::new(icon).xsmall())
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
