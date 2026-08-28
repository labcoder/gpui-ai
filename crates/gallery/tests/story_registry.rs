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

#[test]
fn published_consumer_routes_keep_their_slugs_and_titles() {
    // Literal expectations protect published URLs; deriving them from slug()
    // would only test round-tripping, which is covered separately above.
    for (story, slug, title) in [
        (StoryId::PromptBar, "prompt-bar", "Prompt bar"),
        (StoryId::CommandSearch, "command-search", "Command search"),
        (StoryId::SidebarNav, "sidebar-nav", "Sidebar navigation"),
        (StoryId::FineTune, "fine-tune", "Fine-tune card"),
        (StoryId::RecordsTable, "records-table", "Records table"),
        (StoryId::DiffTable, "diff-table", "Diff table"),
        (StoryId::FilterTable, "filter-table", "Filter table"),
        (
            StoryId::ComparisonTable,
            "comparison-table",
            "Comparison table",
        ),
    ] {
        assert_eq!(story.slug(), slug, "{story:?}");
        assert_eq!(story.title(), title, "{story:?}");
        assert_eq!(StoryId::from_str(slug), Ok(story));
        assert!(
            StoryId::ALL.contains(&story),
            "{slug} remains in the catalog"
        );
    }
}
