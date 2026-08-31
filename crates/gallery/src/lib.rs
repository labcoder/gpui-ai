//! Shared story catalog and gallery view used by native and WebAssembly hosts.

#[cfg(any(test, feature = "performance"))]
pub mod catalog_performance;
mod decorations;
mod dock_composition;
mod gallery;
mod metrics;
mod motion_lab;
pub mod performance;
mod sim;
mod story;
mod usage;

#[cfg(test)]
include!(concat!(env!("OUT_DIR"), "/readme_contracts.rs"));

pub use gallery::{
    Gallery, GalleryChrome, GalleryTheme, active_variant_index, apply_gallery_theme, init,
    measured_story_height, open_gallery, open_gallery_with_theme, set_active_variant,
};
pub use story::{
    CHAT_STORY_VARIANTS, HERO_HEIGHT, Overflow, StoryId, StoryLookupError, StoryMeta,
    TABLE_STORY_VARIANTS,
};

/// The decorations the site's Effects section documents.
///
/// Slug, label, and the line that says what each one is. The gallery already
/// keeps all three; this hands them out so the website is generated from the
/// list rather than from a copy of it that goes stale - which is what the four
/// hand-written entries on the Extensions page had already done.
pub fn decoration_catalog() -> Vec<(&'static str, &'static str, &'static str)> {
    decorations::catalog()
}

/// The height the decorations story measures at the demo width.
///
/// The story is deliberately absent from the component index, so nothing
/// measures it the way `StoryMeta::height` is measured. It is the staged
/// backdrop plus the switcher and the note above it.
pub const DECORATIONS_HEIGHT: u32 = 420;
