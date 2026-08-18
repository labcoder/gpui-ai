use gallery::{Gallery, GalleryTheme, StoryId, StoryLookupError};
use gpui::{App, ApplicationHandle, Entity};
use gpui_component::theme::{Theme, ThemeMode};
use std::cell::RefCell;
use wasm_bindgen::prelude::*;

thread_local! {
    static APPLICATION: RefCell<Option<ApplicationHandle>> = const { RefCell::new(None) };
    static GALLERY: RefCell<Option<Entity<Gallery>>> = const { RefCell::new(None) };
}

fn parse_story(story: Option<String>) -> Result<StoryId, StoryLookupError> {
    match story {
        Some(slug) => slug.parse(),
        None => Ok(StoryId::All),
    }
}

fn parse_theme(theme: Option<String>) -> Result<GalleryTheme, String> {
    match theme.as_deref() {
        Some("dark") => Ok(GalleryTheme::Dark),
        Some("contrast") => Ok(GalleryTheme::Contrast),
        Some("light") | None => Ok(GalleryTheme::Light),
        Some(theme) => Err(format!("unknown gallery theme: {theme}")),
    }
}

/// Validates a host-provided story slug against the shared Rust registry.
#[cfg_attr(target_family = "wasm", wasm_bindgen)]
pub fn validate_story(story: Option<String>) -> Result<(), JsValue> {
    parse_story(story)
        .map(|_| ())
        .map_err(|error| JsValue::from_str(&error.to_string()))
}

fn apply_theme(mode: ThemeMode, cx: &mut App) {
    Theme::change(mode, None, cx);

    #[cfg(target_family = "wasm")]
    {
        let theme = Theme::global_mut(cx);
        theme.font_family = "IBM Plex Sans".into();
        theme.mono_font_family = "Lilex".into();
    }
}

/// Switches a running gallery between light and dark mode.
#[cfg_attr(target_family = "wasm", wasm_bindgen)]
pub fn set_theme(dark: bool) {
    let (mode, preset) = if dark {
        (ThemeMode::Dark, GalleryTheme::Dark)
    } else {
        (ThemeMode::Light, GalleryTheme::Light)
    };

    APPLICATION.with(|application| {
        if let Some(handle) = application.borrow().as_ref() {
            handle.update(|cx| {
                apply_theme(mode, cx);
                GALLERY.with(|gallery| {
                    if let Some(gallery) = gallery.borrow().as_ref() {
                        gallery.update(cx, |gallery, cx| gallery.set_theme_preset(preset, cx));
                    }
                });
                cx.refresh_windows();
            });
        }
    });
}

/// Starts the gallery for an optional story slug.
#[cfg_attr(target_family = "wasm", wasm_bindgen)]
pub fn run(
    story: Option<String>,
    theme: Option<String>,
    asset_base: Option<String>,
) -> Result<(), JsValue> {
    let selected = parse_story(story).map_err(|error| JsValue::from_str(&error.to_string()))?;
    let theme = parse_theme(theme).map_err(|error| JsValue::from_str(&error))?;
    console_error_panic_hook::set_once();
    #[cfg(not(target_family = "wasm"))]
    let _ = asset_base;

    #[cfg(target_family = "wasm")]
    gpui_platform::web_init();
    #[cfg(not(target_family = "wasm"))]
    let application = gpui_platform::application();
    #[cfg(target_family = "wasm")]
    let application = gpui_platform::single_threaded_web();

    #[cfg(not(target_family = "wasm"))]
    let application = application.with_assets(gpui_component_assets::Assets);
    #[cfg(target_family = "wasm")]
    let application = application.with_assets(gpui_component_assets::Assets::new(
        asset_base.unwrap_or_else(|| "/".to_owned()),
    ));

    let launch = move |cx: &mut App| {
        gallery::init(cx);
        let mode = if theme == GalleryTheme::Light {
            ThemeMode::Light
        } else {
            ThemeMode::Dark
        };
        apply_theme(mode, cx);
        let gallery = gallery::open_gallery_with_theme(selected, theme, cx);
        GALLERY.with(|stored| *stored.borrow_mut() = Some(gallery));
        cx.activate(true);
    };

    #[cfg(target_family = "wasm")]
    APPLICATION.with(|stored| {
        *stored.borrow_mut() = Some(application.run_embedded(launch));
    });
    #[cfg(not(target_family = "wasm"))]
    application.run(launch);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{parse_story, parse_theme};
    use gallery::{GalleryTheme, StoryId};

    #[test]
    fn missing_story_selects_the_catalog() {
        assert_eq!(parse_story(None), Ok(StoryId::All));
    }

    #[test]
    fn known_story_selects_the_matching_registry_entry() {
        assert_eq!(
            parse_story(Some("streaming-text".to_owned())),
            Ok(StoryId::StreamingText)
        );
    }

    #[test]
    fn invalid_story_returns_the_requested_slug() {
        let error = parse_story(Some("missing".to_owned())).expect_err("slug must fail");
        assert_eq!(error.slug(), "missing");
    }

    #[test]
    fn contrast_theme_selects_the_review_preset() {
        assert_eq!(
            parse_theme(Some("contrast".to_owned())),
            Ok(GalleryTheme::Contrast)
        );
    }

    #[test]
    fn invalid_theme_reports_the_requested_name() {
        let error = parse_theme(Some("neon".to_owned())).expect_err("theme must fail");
        assert_eq!(error, "unknown gallery theme: neon");
    }
}
