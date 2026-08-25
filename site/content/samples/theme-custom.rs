use gpui::App;
use gpui_component::theme::{Theme, ThemeConfig, ThemeMode, ThemeRegistry};
use std::rc::Rc;

/// Registers a theme pack and switches to one of the themes in it.
///
/// A pack is the same JSON the bundled themes use: a name and a `themes`
/// array, each entry declaring a mode, its colours, and optionally a radius,
/// a shadow flag, and a base font size.
fn install(pack: &str, name: &str, cx: &mut App) -> anyhow::Result<()> {
    ThemeRegistry::global_mut(cx).load_themes_from_str(pack)?;

    let config = ThemeRegistry::global(cx)
        .themes()
        .get(name)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("the pack declares no theme called {name}"))?;

    let mode = config.mode;
    let theme = Theme::global_mut(cx);
    match mode {
        ThemeMode::Dark => theme.dark_theme = config.clone(),
        _ => theme.light_theme = config.clone(),
    }
    Theme::change(mode, None, cx);
    restore_unset_metrics(&config, cx);
    Ok(())
}

/// Puts back the metrics the incoming theme does not mention.
///
/// Applying a theme writes only the metrics that theme names, and what it
/// leaves behind is the *previous* theme's value rather than the default. In
/// an application that installs one theme and keeps it that never shows. In
/// one that lets a person switch, a theme asking for 14px type leaves every
/// theme chosen after it at 14px until the process restarts.
fn restore_unset_metrics(config: &ThemeConfig, cx: &mut App) {
    let defaults = Theme::default();
    let theme = Theme::global_mut(cx);
    if config.font_size.is_none() {
        theme.font_size = defaults.font_size;
    }
    if config.mono_font_size.is_none() {
        theme.mono_font_size = defaults.mono_font_size;
    }
    if config.radius.is_none() {
        theme.radius = defaults.radius;
    }
    if config.radius_lg.is_none() {
        theme.radius_lg = defaults.radius_lg;
    }
    if config.shadow.is_none() {
        theme.shadow = defaults.shadow;
    }
    // The Base layer keeps its own copy of the radius, so a scrollbar would go
    // on painting with the corners the last theme gave it.
    Theme::sync_base(cx);
}
