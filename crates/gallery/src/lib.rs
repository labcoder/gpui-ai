//! Shared story catalog and gallery view used by native and WebAssembly hosts.

mod gallery;
mod sim;
mod story;

pub use gallery::{Gallery, GalleryTheme, init, open_gallery, open_gallery_with_theme};
pub use story::{StoryId, StoryLookupError};
