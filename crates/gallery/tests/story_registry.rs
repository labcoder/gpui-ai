use gallery::StoryId;
use std::collections::HashSet;
use std::str::FromStr as _;

#[test]
fn every_catalog_slug_is_unique_and_round_trips() {
    let mut slugs = HashSet::new();

    for story in StoryId::ALL {
        assert!(
            slugs.insert(story.slug()),
            "duplicate slug: {}",
            story.slug()
        );
        assert_eq!(StoryId::from_str(story.slug()), Ok(*story));
    }
}

#[test]
fn unknown_story_preserves_the_requested_slug() {
    let error = StoryId::from_str("not-a-story").expect_err("unknown slug must fail");

    assert_eq!(error.slug(), "not-a-story");
    assert_eq!(error.to_string(), "unknown story: not-a-story");
}
