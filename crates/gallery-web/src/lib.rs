use gallery::{Gallery, GalleryTheme, StoryId, StoryLookupError};
use gpui::{App, ApplicationHandle, Entity};
use gpui_component::theme::{Theme, ThemeMode, ThemeRegistry};
use std::cell::{Cell, RefCell};
use wasm_bindgen::prelude::*;

thread_local! {
    static SELECTED: Cell<Option<StoryId>> = const { Cell::new(None) };
    static APPLICATION: RefCell<Option<ApplicationHandle>> = const { RefCell::new(None) };
    static GALLERY: RefCell<Option<Entity<Gallery>>> = const { RefCell::new(None) };
    static ACTIVE_THEME: Cell<Option<GalleryTheme>> = const { Cell::new(None) };
}

fn parse_story(story: Option<String>) -> Result<StoryId, StoryLookupError> {
    match story {
        Some(slug) => slug.parse(),
        None => Ok(StoryId::All),
    }
}

fn parse_theme(theme: Option<String>) -> Result<GalleryTheme, String> {
    match theme.as_deref() {
        None => Ok(GalleryTheme::LIGHT),
        Some(slug) => {
            GalleryTheme::from_slug(slug).ok_or_else(|| format!("unknown gallery theme: {slug}"))
        }
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
    let theme = if dark { "dark" } else { "light" };
    let _ = set_gallery_theme(theme.to_owned());
}

/// Switches a running gallery to a named review theme.
#[cfg_attr(target_family = "wasm", wasm_bindgen)]
pub fn set_gallery_theme(theme: String) -> Result<bool, JsValue> {
    let preset = parse_theme(Some(theme)).map_err(|error| JsValue::from_str(&error))?;
    let applied = Cell::new(false);
    APPLICATION.with(|application| {
        if let Some(handle) = application.borrow().as_ref() {
            handle.update(|cx| {
                if !cx.has_global::<ThemeRegistry>() {
                    return;
                }
                let Some(gallery) = GALLERY.with(|gallery| gallery.borrow().clone()) else {
                    return;
                };
                gallery::apply_gallery_theme(preset, None, cx);
                #[cfg(target_family = "wasm")]
                {
                    let theme = Theme::global_mut(cx);
                    theme.font_family = "IBM Plex Sans".into();
                    theme.mono_font_family = "Lilex".into();
                }
                gallery.update(cx, |gallery, cx| gallery.set_theme_preset(preset, cx));
                ACTIVE_THEME.with(|active| active.set(Some(preset)));
                cx.refresh_windows();
                applied.set(true);
            });
        }
    });
    Ok(applied.get())
}

/// Turns reduced motion on or off in the running gallery.
///
/// The library's whole claim about motion is that a reduced-motion run lands on
/// a useful static frame rather than an empty one — one-shot reveals settle at
/// their end state and repeating effects render at rest. Nothing on the web
/// could ask for that: GPUI reads the preference from the platform, and the
/// web platform has none, so every demo shimmered at a reader who had asked
/// their machine for stillness.
///
/// Returns whether a running gallery took it.
#[cfg_attr(target_family = "wasm", wasm_bindgen)]
pub fn set_reduced_motion(reduced: bool) -> bool {
    let applied = Cell::new(false);
    APPLICATION.with(|application| {
        if let Some(handle) = application.borrow().as_ref() {
            handle.update(|cx| {
                cx.set_reduce_motion(reduced);
                cx.refresh_windows();
                applied.set(true);
            });
        }
    });
    applied.get()
}

/// Whether the running gallery is drawing with reduced motion.
#[cfg_attr(target_family = "wasm", wasm_bindgen)]
pub fn reduced_motion() -> bool {
    let reduced = Cell::new(false);
    APPLICATION.with(|application| {
        if let Some(handle) = application.borrow().as_ref() {
            handle.update(|cx| reduced.set(cx.reduce_motion()));
        }
    });
    reduced.get()
}

/// Puts the running story back to the state it opened in.
///
/// The page's Reload button used to replace the whole frame, which tears down
/// a seventeen-megabyte WebAssembly instance and builds another one to get
/// back to a state the story can reach in a frame. This is that, without the
/// download.
///
/// Returns whether a running gallery took it.
#[cfg_attr(target_family = "wasm", wasm_bindgen)]
pub fn reset_story() -> bool {
    let reset = Cell::new(false);
    APPLICATION.with(|application| {
        if let Some(handle) = application.borrow().as_ref() {
            handle.update(|cx| {
                let Some(gallery) = GALLERY.with(|gallery| gallery.borrow().clone()) else {
                    return;
                };
                gallery.update(cx, |gallery, cx| gallery.reset_story(cx));
                cx.refresh_windows();
                reset.set(true);
            });
        }
    });
    reset.get()
}

/// Reads the approval story's decision without dispatching an action.
#[cfg_attr(target_family = "wasm", wasm_bindgen)]
pub fn is_approval_granted(id: String) -> bool {
    let granted = Cell::new(false);
    APPLICATION.with(|application| {
        if let Some(handle) = application.borrow().as_ref() {
            handle.update(|cx| {
                if let Some(gallery) = GALLERY.with(|gallery| gallery.borrow().clone()) {
                    granted.set(gallery.read(cx).is_approval_granted(&id));
                }
            });
        }
    });
    granted.get()
}

/// The states the running story offers, as slugs, in the order it draws them.
///
/// Five of the thirty-four stories have any: the chat story and the four
/// tables. The rest return an empty list, which is the honest answer and not a
/// failure.
#[cfg_attr(target_family = "wasm", wasm_bindgen)]
pub fn story_variants() -> Vec<String> {
    SELECTED
        .with(|selected| selected.get())
        .map(|story| {
            story
                .variants()
                .iter()
                .map(|(slug, _)| (*slug).to_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// Which state the running story is showing, if it offers any.
#[cfg_attr(target_family = "wasm", wasm_bindgen)]
pub fn story_variant() -> Option<String> {
    let story = SELECTED.with(|selected| selected.get())?;
    let index = gallery::active_variant_index()?;
    story
        .variants()
        .get(index)
        .map(|(slug, _)| (*slug).to_owned())
}

/// Puts the running story into one of the states it offers.
///
/// This is what lets an address name a state. Without it a link to a demo
/// always opened the story where the story opens, so Copy link handed over
/// something other than what the sender was looking at.
///
/// Returns whether the story took it: an unknown slug, a story with no states,
/// or a story that has not drawn yet are all "no" rather than an error, because
/// the caller is an address bar and a wrong guess there is not exceptional.
#[cfg_attr(target_family = "wasm", wasm_bindgen)]
pub fn set_story_variant(variant: String) -> bool {
    let Some(story) = SELECTED.with(|selected| selected.get()) else {
        return false;
    };
    let Some(index) = story
        .variants()
        .iter()
        .position(|(slug, _)| *slug == variant)
    else {
        return false;
    };

    let applied = Cell::new(false);
    APPLICATION.with(|application| {
        if let Some(handle) = application.borrow().as_ref() {
            handle.update(|cx| {
                if gallery::set_active_variant(index, cx) {
                    cx.refresh_windows();
                    applied.set(true);
                }
            });
        }
    });
    applied.get()
}

/// Returns the theme preset most recently applied by the running Rust gallery.
#[cfg_attr(target_family = "wasm", wasm_bindgen)]
pub fn gallery_theme() -> Option<String> {
    ACTIVE_THEME.with(|active| active.get().map(|theme| theme.slug().to_owned()))
}

/// What the running story last laid out at, in logical pixels.
///
/// The page around the embed sizes its frame from the numbers in the catalog,
/// which were measured at one width. A story's height is a function of the
/// width it is given and not a step function — prose rewraps a line at a time
/// — so on a phone, a tablet, or a half-width window those numbers are wrong,
/// and the story scrolls inside its own canvas rather than being shown.
///
/// `None` until the first frame has been laid out.
#[cfg_attr(target_family = "wasm", wasm_bindgen)]
pub fn story_height() -> Option<u32> {
    gallery::measured_story_height()
}

/// Starts the gallery for an optional story slug.
#[cfg_attr(target_family = "wasm", wasm_bindgen)]
pub fn run(
    story: Option<String>,
    theme: Option<String>,
    asset_base: Option<String>,
) -> Result<(), JsValue> {
    let selected = parse_story(story).map_err(|error| JsValue::from_str(&error.to_string()))?;
    SELECTED.with(|current| current.set(Some(selected)));
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
        let mode = if theme.is_dark() {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        };
        apply_theme(mode, cx);
        let gallery = gallery::open_gallery_with_theme(selected, theme, cx);
        // The page frames each demo and offers its own theme picker, so the
        // embed must not draw a second title and control inside that frame.
        gallery.update(cx, |gallery, cx| {
            gallery.set_chrome(gallery::GalleryChrome::Embedded, cx);
        });
        GALLERY.with(|stored| *stored.borrow_mut() = Some(gallery));
        ACTIVE_THEME.with(|active| active.set(Some(theme)));
        cx.activate(true);
    };

    // ponytail: the demos draw in fonts GPUI's web platform embeds, and upstream
    // is moving that bundle into the application - a browser build will start
    // with an empty font database. When it lands, the release test "the canvas
    // draws glyphs, not just its furniture" goes red, and the fix is here:
    // vendor IBM Plex Sans Regular/SemiBold/Italic and Lilex Regular beside this
    // file and, inside `launch` before the window opens,
    // `cx.text_system().add_fonts(..)` with their bytes. Four faces, ~784 KB,
    // which is half of what the platform bundles today; our themes set no bold
    // or italic syntax, so the other four buy nothing. Not vendored yet because
    // until that day their bytes would sit in the binary beside upstream's.
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
    use super::{gallery_theme, parse_story, parse_theme, set_gallery_theme};
    use gallery::StoryId;

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
        let preset = parse_theme(Some("contrast".to_owned())).expect("contrast must resolve");
        assert_eq!(preset.slug(), "contrast");
        assert!(preset.is_dark());
    }

    #[test]
    fn a_vendored_upstream_theme_resolves_by_slug() {
        let preset =
            parse_theme(Some("tokyo-night".to_owned())).expect("the upstream pack must resolve");
        assert_eq!(preset.group(), "gpui-component");
    }

    #[test]
    fn invalid_theme_reports_the_requested_name() {
        let error = parse_theme(Some("neon".to_owned())).expect_err("theme must fail");
        assert_eq!(error, "unknown gallery theme: neon");
    }

    #[test]
    fn named_theme_waits_until_the_gallery_is_ready() {
        assert!(matches!(
            set_gallery_theme("contrast".to_owned()),
            Ok(false)
        ));
        assert_eq!(gallery_theme(), None);
    }
}
