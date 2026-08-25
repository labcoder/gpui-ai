//! Shared story catalog and gallery view used by native and WebAssembly hosts.

#[cfg(any(test, feature = "performance"))]
pub mod catalog_performance;
mod gallery;
pub mod performance;
mod sim;
mod story;

pub use gallery::{
    Gallery, GalleryChrome, GalleryTheme, active_variant_index, apply_gallery_theme, init,
    measured_story_height, set_active_variant,
    open_gallery,
    open_gallery_with_theme,
};
pub use story::{
    CHAT_STORY_VARIANTS, HERO_HEIGHT, Overflow, StoryId, StoryLookupError, StoryMeta,
    TABLE_STORY_VARIANTS,
};
