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
    /// Collapsible tool-call cards and groups.
    ToolCalls,
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
    /// Starter and follow-up suggestion chips.
    Suggestions,
    /// Composer and message attachment previews.
    Attachments,
    /// Generated document or code beside the conversation.
    Artifact,
    /// Context-window usage meter.
    ContextMeter,
    /// Stable-ID command palette.
    CommandSearch,
    /// Stable-ID filterable sidebar navigation.
    SidebarNav,
    /// Controlled conversation list with search and row actions.
    ThreadList,
    /// Controlled design-property inspector.
    FineTune,
    /// Controlled virtualized CRM-style records grid.
    RecordsTable,
    /// Controlled virtualized before/after proposal grid.
    DiffTable,
    /// Controlled filterable task grid with stable-row reorder motion.
    FilterTable,
    /// Controlled bounded feature comparison grid.
    ComparisonTable,
    /// Streaming code block.
    CodeBlock,
    /// Reviewable unified code diff.
    CodeDiff,
    /// Human approval gate.
    Approval,
    /// Proposed plan with steps and decisions.
    Plan,
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
        Self::ToolCalls,
        Self::Tasks,
        Self::Thinking,
        Self::Orbs,
        Self::Search,
        Self::Todos,
        Self::ImageGeneration,
        Self::StreamingText,
        Self::Chat,
        Self::Suggestions,
        Self::Attachments,
        Self::Artifact,
        Self::ContextMeter,
        Self::CommandSearch,
        Self::SidebarNav,
        Self::ThreadList,
        Self::FineTune,
        Self::RecordsTable,
        Self::DiffTable,
        Self::FilterTable,
        Self::ComparisonTable,
        Self::CodeBlock,
        Self::CodeDiff,
        Self::Approval,
        Self::Plan,
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
            Self::ToolCalls => "tool-calls",
            Self::Tasks => "tasks",
            Self::Thinking => "thinking",
            Self::Orbs => "orbs",
            Self::Search => "search",
            Self::Todos => "todos",
            Self::ImageGeneration => "image-generation",
            Self::StreamingText => "streaming-text",
            Self::Chat => "chat",
            Self::Suggestions => "suggestions",
            Self::Attachments => "attachments",
            Self::Artifact => "artifact",
            Self::ContextMeter => "context-meter",
            Self::CommandSearch => "command-search",
            Self::SidebarNav => "sidebar-nav",
            Self::ThreadList => "thread-list",
            Self::FineTune => "fine-tune",
            Self::RecordsTable => "records-table",
            Self::DiffTable => "diff-table",
            Self::FilterTable => "filter-table",
            Self::ComparisonTable => "comparison-table",
            Self::CodeBlock => "code-block",
            Self::CodeDiff => "code-diff",
            Self::Approval => "approval",
            Self::Plan => "plan",
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
            Self::ToolCalls => "Tool calls",
            Self::Tasks => "Task rows",
            Self::Thinking => "Thinking",
            Self::Orbs => "Orbs",
            Self::Search => "Web search",
            Self::Todos => "To-do list",
            Self::ImageGeneration => "Image generation",
            Self::StreamingText => "Streaming text",
            Self::Chat => "Chat",
            Self::Suggestions => "Suggestions",
            Self::Attachments => "Attachment previews",
            Self::Artifact => "Artifact panel",
            Self::ContextMeter => "Context meter",
            Self::CommandSearch => "Command search",
            Self::SidebarNav => "Sidebar navigation",
            Self::ThreadList => "Thread list",
            Self::FineTune => "Fine-tune card",
            Self::RecordsTable => "Records table",
            Self::DiffTable => "Diff table",
            Self::FilterTable => "Filter table",
            Self::ComparisonTable => "Comparison table",
            Self::CodeBlock => "Code block",
            Self::CodeDiff => "Code diff",
            Self::Approval => "Approval card",
            Self::Plan => "Plan card",
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

    #[test]
    fn sidebar_navigation_has_a_stable_gallery_route() {
        assert_eq!(StoryId::SidebarNav.slug(), "sidebar-nav");
        assert_eq!(StoryId::SidebarNav.title(), "Sidebar navigation");
        assert_eq!("sidebar-nav".parse::<StoryId>(), Ok(StoryId::SidebarNav));
        assert!(StoryId::ALL.contains(&StoryId::SidebarNav));
    }

    #[test]
    fn fine_tune_has_a_stable_gallery_route() {
        assert_eq!(StoryId::FineTune.slug(), "fine-tune");
        assert_eq!(StoryId::FineTune.title(), "Fine-tune card");
        assert_eq!("fine-tune".parse::<StoryId>(), Ok(StoryId::FineTune));
        assert!(StoryId::ALL.contains(&StoryId::FineTune));
    }

    #[test]
    fn records_table_has_a_stable_gallery_route() {
        assert_eq!(StoryId::RecordsTable.slug(), "records-table");
        assert_eq!(StoryId::RecordsTable.title(), "Records table");
        assert_eq!(
            "records-table".parse::<StoryId>(),
            Ok(StoryId::RecordsTable)
        );
        assert!(StoryId::ALL.contains(&StoryId::RecordsTable));
    }

    #[test]
    fn diff_table_has_a_stable_gallery_route() {
        assert_eq!(StoryId::DiffTable.slug(), "diff-table");
        assert_eq!(StoryId::DiffTable.title(), "Diff table");
        assert_eq!("diff-table".parse::<StoryId>(), Ok(StoryId::DiffTable));
        assert!(StoryId::ALL.contains(&StoryId::DiffTable));
    }

    #[test]
    fn filter_table_has_a_stable_gallery_route() {
        assert_eq!(StoryId::FilterTable.slug(), "filter-table");
        assert_eq!(StoryId::FilterTable.title(), "Filter table");
        assert_eq!("filter-table".parse::<StoryId>(), Ok(StoryId::FilterTable));
        assert!(StoryId::ALL.contains(&StoryId::FilterTable));
    }

    #[test]
    fn comparison_table_has_a_stable_gallery_route() {
        assert_eq!(StoryId::ComparisonTable.slug(), "comparison-table");
        assert_eq!(StoryId::ComparisonTable.title(), "Comparison table");
        assert_eq!(
            "comparison-table".parse::<StoryId>(),
            Ok(StoryId::ComparisonTable)
        );
        assert!(StoryId::ALL.contains(&StoryId::ComparisonTable));
    }
}
