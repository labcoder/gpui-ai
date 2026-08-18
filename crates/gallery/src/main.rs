use gallery::{StoryId, init, open_gallery};

fn main() {
    gpui_platform::application()
        .with_assets(gpui_component_assets::Assets)
        .run(move |cx| {
            init(cx);
            open_gallery(StoryId::All, cx);
        });
}
