use gallery::{StoryId, init, open_gallery};

fn main() {
    // `GALLERY_STORY=motion-lab cargo run -p gallery` opens one story by its
    // slug — the only native door to the addressable stories the catalog
    // deliberately omits (the motion lab above all, which exists to be driven
    // by hand). An unknown slug is a typo, not a preference: say so and show
    // the catalog rather than silently ignoring it.
    let story = std::env::var("GALLERY_STORY")
        .ok()
        .map(|slug| match slug.parse::<StoryId>() {
            Ok(story) => story,
            Err(error) => {
                eprintln!("GALLERY_STORY: {error:?}; opening the catalog");
                StoryId::All
            }
        })
        .unwrap_or(StoryId::All);

    gpui_platform::application()
        .with_assets(gpui_component_assets::Assets)
        .run(move |cx| {
            init(cx);
            open_gallery(story, cx);
        });
}
