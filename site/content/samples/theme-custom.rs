use gpui::App;
use gpui_component::theme::{Theme, ThemeMode, ThemeRegistry};

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
        ThemeMode::Dark => theme.dark_theme = config,
        _ => theme.light_theme = config,
    }
    Theme::change(mode, None, cx);
    Ok(())
}
