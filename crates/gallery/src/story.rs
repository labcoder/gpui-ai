//! Stable identifiers for component stories.

use std::{fmt, str::FromStr};

/// A stable route to one gallery story, or to the complete catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StoryId {
    /// The complete catalog.
    All,
    /// Pixel-grid loading state.
    Loading,
    /// Tool-call chips.
    ToolChips,
    /// Progressive task rows.
    Tasks,
    /// Expandable thinking traces.
    Thinking,
    /// Ambient orbs.
    Orbs,
    /// Web-search results.
    Search,
    /// Agent to-do list.
    Todos,
    /// Image-generation progress.
    ImageGeneration,
    /// Streaming Markdown answer.
    StreamingText,
    /// Controlled virtualized conversation.
    Chat,
    /// Stable-ID command palette.
    CommandSearch,
    /// Streaming code block.
    CodeBlock,
    /// Human approval gate.
    Approval,
    /// Recommendation card.
    Recommendation,
    /// Context/source cards.
    Context,
    /// Paged analytical insight card.
    Insights,
    /// Hybrid-controlled prompt composer.
    PromptBar,
    /// Actions anchored to a readable text selection.
    SelectionActions,
}

impl StoryId {
    /// Every individually addressable component story, in catalog order.
    pub const ALL: &'static [Self] = &[
        Self::Loading,
        Self::ToolChips,
        Self::Tasks,
        Self::Thinking,
        Self::Orbs,
        Self::Search,
        Self::Todos,
        Self::ImageGeneration,
        Self::StreamingText,
        Self::Chat,
        Self::CommandSearch,
        Self::CodeBlock,
        Self::Approval,
        Self::Recommendation,
        Self::Context,
        Self::Insights,
        Self::PromptBar,
        Self::SelectionActions,
    ];

    /// Stable URL slug for this selection.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Loading => "loading",
            Self::ToolChips => "tool-chips",
            Self::Tasks => "tasks",
            Self::Thinking => "thinking",
            Self::Orbs => "orbs",
            Self::Search => "search",
            Self::Todos => "todos",
            Self::ImageGeneration => "image-generation",
            Self::StreamingText => "streaming-text",
            Self::Chat => "chat",
            Self::CommandSearch => "command-search",
            Self::CodeBlock => "code-block",
            Self::Approval => "approval",
            Self::Recommendation => "recommendation",
            Self::Context => "context",
            Self::Insights => "insights",
            Self::PromptBar => "prompt-bar",
            Self::SelectionActions => "selection-actions",
        }
    }

    /// Human-readable story title.
    pub const fn title(self) -> &'static str {
        match self {
            Self::All => "All components",
            Self::Loading => "Loading state",
            Self::ToolChips => "Tool chips",
            Self::Tasks => "Task rows",
            Self::Thinking => "Thinking",
            Self::Orbs => "Orbs",
            Self::Search => "Web search",
            Self::Todos => "To-do list",
            Self::ImageGeneration => "Image generation",
            Self::StreamingText => "Streaming text",
            Self::Chat => "Chat",
            Self::CommandSearch => "Command search",
            Self::CodeBlock => "Code block",
            Self::Approval => "Approval card",
            Self::Recommendation => "Recommendation card",
            Self::Context => "Context cards",
            Self::Insights => "Insight card",
            Self::PromptBar => "Prompt bar",
            Self::SelectionActions => "Selection actions",
        }
    }
}

impl FromStr for StoryId {
    type Err = StoryLookupError;

    fn from_str(slug: &str) -> Result<Self, Self::Err> {
        if slug == Self::All.slug() {
            return Ok(Self::All);
        }

        Self::ALL
            .iter()
            .copied()
            .find(|story| story.slug() == slug)
            .ok_or_else(|| StoryLookupError {
                slug: slug.to_owned(),
            })
    }
}

/// Error returned when a deep link names no registered story.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryLookupError {
    slug: String,
}

impl StoryLookupError {
    /// The unrecognized slug supplied by the host.
    pub fn slug(&self) -> &str {
        &self.slug
    }
}

impl fmt::Display for StoryLookupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "unknown story: {}", self.slug)
    }
}

impl std::error::Error for StoryLookupError {}

#[cfg(test)]
mod tests {
    use super::StoryId;

    #[test]
    fn command_search_has_a_stable_gallery_route() {
        assert_eq!(StoryId::CommandSearch.slug(), "command-search");
        assert_eq!(StoryId::CommandSearch.title(), "Command search");
        assert_eq!(
            "command-search".parse::<StoryId>(),
            Ok(StoryId::CommandSearch)
        );
        assert!(StoryId::ALL.contains(&StoryId::CommandSearch));
    }
}
