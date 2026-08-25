use gpui::prelude::*;
use gpui_component::ActiveTheme as _;

fn panel(cx: &App) -> impl IntoElement {
    div()
        // Every colour, radius and type style comes from the active theme.
        // There is no gpui-ai styling layer to override and no colour of its
        // own to disagree with yours.
        .bg(cx.theme().secondary)
        .border_1()
        .border_color(cx.theme().border)
        .rounded(cx.theme().radius)
        .text_color(cx.theme().muted_foreground)
        .child("Ran three tools")
}
