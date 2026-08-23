//! Shared surface and typography helpers.
//!
//! These are the crate's visual grammar in code: one card frame, one title
//! role, one description role, one eyebrow, one quiet icon button. Every
//! component builds from them so radii, spacing, weights, and hover treatment
//! stay identical across the library — and change in one place.

use crate::control::composed_button;
use crate::theme::SemanticStyledExt as _;
use gpui::{
    App, Div, ElementId, FontWeight, InteractiveElement as _, ParentElement as _, SharedString,
    Stateful, StatefulInteractiveElement as _, Styled as _, div,
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
    cx: &App,
) -> Button {
    let tokens = cx.theme().semantic_tokens();
    composed_button(id, accessibility_label)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(tokens.spacing.xl)
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
        .child(Icon::new(icon).xsmall())
}
