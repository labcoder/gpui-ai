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
    /// Dictate and speak controls.
    Voice,
    /// Prompts waiting while the agent runs.
    Queue,
    /// Actions anchored to a readable text selection.
    SelectionActions,
}

/// Variants the Chat story switches between.
///
/// Variant lists live here rather than beside the stories so the exported
/// catalog and the switcher toolbar cannot disagree.
pub const CHAT_STORY_VARIANTS: &[(&str, &str)] =
    &[("conversation", "Conversation"), ("welcome", "Welcome")];

/// Variants every table story switches between.
pub const TABLE_STORY_VARIANTS: &[(&str, &str)] = &[
    ("populated", "Populated"),
    ("loading", "Loading"),
    ("error", "Error"),
    ("empty", "Empty"),
    ("disabled", "Disabled"),
    ("selected", "Selected"),
    ("constrained", "Constrained"),
];

/// How much vertical room a story's demo frame needs on the website.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Viewport {
    /// Fits a standard demo frame.
    Wide,
    /// Needs a taller frame to show its real states.
    Tall,
}

impl Viewport {
    /// The value written to the exported catalog.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Wide => "wide",
            Self::Tall => "tall",
        }
    }
}

/// Catalog metadata for one component story, exported to the website.
///
/// This is the single source for the component index: the site is generated
/// from it rather than from a parallel hand-maintained list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoryMeta {
    /// The group the component index files this story under.
    pub category: &'static str,
    /// One sentence describing what the component is for.
    pub summary: &'static str,
    /// The module stem under `crates/gpui-ai/src/` that implements it.
    pub module: &'static str,
    /// The primary public type the story demonstrates.
    pub api: &'static str,
    /// A constructor call short enough to read in the index.
    pub usage: &'static str,
    /// How much vertical room the demo frame needs.
    pub viewport: Viewport,
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
        Self::Voice,
        Self::Queue,
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
            Self::Voice => "voice",
            Self::Queue => "queue",
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
            Self::Voice => "Voice controls",
            Self::Queue => "Message queue",
            Self::SelectionActions => "Selection actions",
        }
    }

    /// Catalog metadata for the website.
    ///
    /// [`StoryId::All`] is the whole-catalog view rather than a component, so
    /// it has no entry.
    pub const fn meta(self) -> Option<StoryMeta> {
        Some(match self {
            Self::All => return None,
            Self::Loading => StoryMeta {
                category: "Progress",
                summary: "A token-driven pixel field for work whose duration is not yet known.",
                module: "loading",
                api: "LoadingState",
                usage: "LoadingState::new().label(\"Thinking\")",
                viewport: Viewport::Tall,
            },
            Self::ToolChips => StoryMeta {
                category: "Agent work",
                summary: "Compact, typed status for tool calls without hiding their lifecycle.",
                module: "chip",
                api: "ToolChip",
                usage: "ToolChip::new(\"edit\", \"edit main.rs\")",
                viewport: Viewport::Tall,
            },
            Self::ToolCalls => StoryMeta {
                category: "Agent work",
                summary: "Collapsible tool-call cards with input, output, approval, and a shimmering group.",
                module: "tool_call",
                api: "ToolCall",
                usage: "ToolCall::new(&call_progress)",
                viewport: Viewport::Tall,
            },
            Self::Tasks => StoryMeta {
                category: "Agent work",
                summary: "Progressive task rows with stable identity and readable state.",
                module: "task",
                api: "TaskRow",
                usage: "TaskRow::new(&task_progress)",
                viewport: Viewport::Tall,
            },
            Self::Thinking => StoryMeta {
                category: "Progress",
                summary: "Expandable reasoning traces in structured step and prose forms.",
                module: "thinking",
                api: "Thinking",
                usage: "Thinking::new(\"reasoning\", &trace_progress)",
                viewport: Viewport::Tall,
            },
            Self::Orbs => StoryMeta {
                category: "Progress",
                summary: "A reduced-motion-aware ambient signal for background AI activity.",
                module: "orbs",
                api: "Orbs",
                usage: "Orbs::new()",
                viewport: Viewport::Tall,
            },
            Self::Search => StoryMeta {
                category: "Agent work",
                summary: "Search results with readable citations, metadata, and progressive state.",
                module: "search_results",
                api: "SearchResults",
                usage: "SearchResults::new(\"research\", \"GPUI components\")",
                viewport: Viewport::Wide,
            },
            Self::Todos => StoryMeta {
                category: "Agent work",
                summary: "A stable-ID checklist for plans that change while an agent works.",
                module: "todo_list",
                api: "TodoList",
                usage: "TodoList::new(\"release-plan\")",
                viewport: Viewport::Tall,
            },
            Self::ImageGeneration => StoryMeta {
                category: "Agent work",
                summary: "Image generation progress, preview, and error states in one frame.",
                module: "image_generation",
                api: "ImageGeneration",
                usage: "ImageGeneration::new(\"hero-art\").progress(0.64)",
                viewport: Viewport::Wide,
            },
            Self::StreamingText => StoryMeta {
                category: "Readable output",
                summary: "Selectable streaming Markdown with citations, sources, and follow-ups.",
                module: "streaming_text",
                api: "StreamingText",
                usage: "StreamingText::new(\"answer\", &content)",
                viewport: Viewport::Wide,
            },
            Self::Chat => StoryMeta {
                category: "Composites",
                summary: "A virtualized controlled conversation with tail-follow, unread behavior, in-place edit, and branch versions.",
                module: "chat",
                api: "Chat",
                usage: "Chat::new(\"conversation\", prompt, window, cx)",
                viewport: Viewport::Tall,
            },
            Self::Suggestions => StoryMeta {
                category: "Composites",
                summary: "Starter and follow-up prompt chips that ripple in and report stable IDs.",
                module: "suggestions",
                api: "Suggestions",
                usage: "Suggestions::new(\"starters\")",
                viewport: Viewport::Wide,
            },
            Self::Attachments => StoryMeta {
                category: "Composites",
                summary: "Composer and message attachments with thumbnails, kinds, upload state, and typed open or remove events.",
                module: "attachment",
                api: "AttachmentStrip",
                usage: "AttachmentStrip::new(\"files\").items(attachments)",
                viewport: Viewport::Wide,
            },
            Self::Artifact => StoryMeta {
                category: "Composites",
                summary: "A side panel for generated documents and code with preview and source views, versions, actions, and streaming state.",
                module: "artifact",
                api: "ArtifactPanel",
                usage: "ArtifactPanel::new(\"doc\", &artifact)",
                viewport: Viewport::Tall,
            },
            Self::ContextMeter => StoryMeta {
                category: "Progress",
                summary: "Context-window usage as a ring, bar, or text with severity tones and a breakdown.",
                module: "context_meter",
                api: "ContextMeter",
                usage: "ContextMeter::new(\"context\", &usage)",
                viewport: Viewport::Wide,
            },
            Self::CommandSearch => StoryMeta {
                category: "Navigation",
                summary: "Keyboard-first command discovery backed by stable application IDs.",
                module: "command_search",
                api: "CommandSearch",
                usage: "CommandSearch::new(\"commands\", window, cx)",
                viewport: Viewport::Tall,
            },
            Self::SidebarNav => StoryMeta {
                category: "Navigation",
                summary: "Filterable, accessible navigation for growing AI workspaces.",
                module: "sidebar_nav",
                api: "SidebarNav",
                usage: "SidebarNav::new(\"workspace-nav\", window, cx)",
                viewport: Viewport::Tall,
            },
            Self::ThreadList => StoryMeta {
                category: "Navigation",
                summary: "A grouped conversation list with search, archived threads, and typed row actions.",
                module: "thread_list",
                api: "ThreadList",
                usage: "ThreadList::new(\"threads\", window, cx)",
                viewport: Viewport::Tall,
            },
            Self::FineTune => StoryMeta {
                category: "Composites",
                summary: "A controlled property inspector for precise model and design settings.",
                module: "fine_tune",
                api: "FineTuneCard",
                usage: "FineTuneCard::new(\"controls\", values, typefaces, window, cx)",
                viewport: Viewport::Tall,
            },
            Self::RecordsTable => StoryMeta {
                category: "Data tables",
                summary: "A controlled virtualized records grid for large, changing datasets.",
                module: "records_table",
                api: "RecordsTable",
                usage: "RecordsTable::new(\"accounts\", \"Accounts\", window, cx)",
                viewport: Viewport::Wide,
            },
            Self::DiffTable => StoryMeta {
                category: "Data tables",
                summary: "A virtualized before-and-after proposal grid with explicit change state.",
                module: "diff_table",
                api: "DiffTable",
                usage: "DiffTable::new(\"proposal\", \"Proposed changes\", window, cx)",
                viewport: Viewport::Wide,
            },
            Self::FilterTable => StoryMeta {
                category: "Data tables",
                summary: "A controlled task grid with typed filters and stable-row reorder motion.",
                module: "filter_table",
                api: "FilterTable",
                usage: "FilterTable::new(\"tasks\", \"Tasks\", window, cx)",
                viewport: Viewport::Wide,
            },
            Self::ComparisonTable => StoryMeta {
                category: "Data tables",
                summary: "A bounded feature matrix with semantic values and sticky context.",
                module: "comparison_table",
                api: "ComparisonTable",
                usage: "ComparisonTable::new(\"plans\", \"Plans\", window, cx)",
                viewport: Viewport::Wide,
            },
            Self::CodeBlock => StoryMeta {
                category: "Readable output",
                summary: "Selectable code with language context and progressive reveal.",
                module: "code_block",
                api: "CodeBlock",
                usage: "CodeBlock::new(\"patch\", source).language(\"rust\")",
                viewport: Viewport::Wide,
            },
            Self::CodeDiff => StoryMeta {
                category: "Readable output",
                summary: "A unified patch with line gutters, change tints, per-hunk accept or reject, and a copyable source.",
                module: "code_diff",
                api: "CodeDiff",
                usage: "CodeDiff::new(\"patch\", &file).reviewable(true)",
                viewport: Viewport::Tall,
            },
            Self::Approval => StoryMeta {
                category: "Decisions",
                summary: "An explicit, keyboard-operable human gate with destructive and always-allow variants and resolved states.",
                module: "approval",
                api: "ApprovalCard",
                usage: "ApprovalCard::new(\"deploy\", \"Deploy production?\")",
                viewport: Viewport::Tall,
            },
            Self::Plan => StoryMeta {
                category: "Decisions",
                summary: "An agent's proposed steps with typed per-step status, approve or reject while proposed, and resolved states.",
                module: "plan",
                api: "PlanCard",
                usage: "PlanCard::new(\"rollout\", \"Switch bulk orders\")",
                viewport: Viewport::Tall,
            },
            Self::Recommendation => StoryMeta {
                category: "Decisions",
                summary: "A focused recommendation with rationale and typed actions.",
                module: "recommendation",
                api: "RecommendationCard",
                usage: "RecommendationCard::new(\"next-step\", \"Ship the fix\")",
                viewport: Viewport::Tall,
            },
            Self::Context => StoryMeta {
                category: "Evidence",
                summary: "Compact source context that preserves provenance and readable detail.",
                module: "context_card",
                api: "ContextCard",
                usage: "ContextCard::new(\"design-doc\", \"Architecture\")",
                viewport: Viewport::Tall,
            },
            Self::Insights => StoryMeta {
                category: "Evidence",
                summary: "Paged analytical findings with chart-ready, semantic values.",
                module: "insight",
                api: "InsightCard",
                usage: "InsightCard::new(\"retention\", \"Retention improved\")",
                viewport: Viewport::Wide,
            },
            Self::PromptBar => StoryMeta {
                category: "Composites",
                summary: "A hybrid-controlled composer with mentions, commands, models, and attachments.",
                module: "prompt_bar",
                api: "PromptBar",
                usage: "PromptBar::new(\"agent-prompt\", window, cx)",
                viewport: Viewport::Wide,
            },
            Self::Voice => StoryMeta {
                category: "Composites",
                summary: "Dictate and speak controls with a live level meter and interim transcript, as typed intent.",
                module: "voice",
                api: "VoiceControls",
                usage: "VoiceControls::new(\"voice\", VoiceState::Idle)",
                viewport: Viewport::Wide,
            },
            Self::Queue => StoryMeta {
                category: "Composites",
                summary: "Prompts waiting while the agent runs, with reorder, edit, send-now, remove, and clear by stable ID.",
                module: "queue",
                api: "MessageQueue",
                usage: "MessageQueue::new(\"queue\").items(queued)",
                viewport: Viewport::Wide,
            },
            Self::SelectionActions => StoryMeta {
                category: "Readable output",
                summary: "Ask, explain, and rewrite actions anchored to selected Markdown.",
                module: "selection_actions",
                api: "SelectionActions",
                usage: "SelectionActions::new(\"answer-actions\", markdown, window, cx)",
                viewport: Viewport::Wide,
            },
        })
    }

    /// The states this story's switcher offers, as `(id, label)` pairs.
    ///
    /// Empty for stories that show one composition.
    pub const fn variants(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Chat => CHAT_STORY_VARIANTS,
            Self::RecordsTable | Self::DiffTable | Self::FilterTable | Self::ComparisonTable => {
                TABLE_STORY_VARIANTS
            }
            _ => &[],
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

    /// Reads a library source file relative to the workspace root.
    fn library_source(module: &str) -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(std::path::Path::parent)
            .expect("the gallery crate lives two directories below the workspace root")
            .join("crates")
            .join("gpui-ai")
            .join("src")
            .join(format!("{module}.rs"));
        std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
    }

    #[test]
    fn every_component_story_carries_catalog_metadata() {
        for story in StoryId::ALL {
            let meta = story
                .meta()
                .unwrap_or_else(|| panic!("{} has no catalog metadata", story.slug()));
            assert!(!meta.category.is_empty(), "{} has no category", story.slug());
            assert!(
                meta.summary.len() > 20,
                "{} needs a real summary, not a label",
                story.slug()
            );
            assert!(!story.title().is_empty());
            assert!(
                meta.module.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "{} points at a module path that is not snake_case",
                story.slug()
            );
        }
        assert!(
            StoryId::All.meta().is_none(),
            "the whole-catalog view is not a component"
        );
    }

    #[test]
    fn every_usage_snippet_constructs_its_own_type_from_the_prelude() {
        for story in StoryId::ALL {
            let meta = story.meta().expect("component stories carry metadata");
            assert!(
                meta.usage.contains(&format!("{}::new", meta.api)),
                "{} must show {}::new, not {}",
                story.slug(),
                meta.api,
                meta.usage
            );
            assert!(
                !meta.usage.contains("px("),
                "{} must not need raw pixels to construct",
                story.slug()
            );
        }
    }

    #[test]
    fn every_story_names_a_public_type_that_exists() {
        for story in StoryId::ALL {
            let meta = story.meta().expect("component stories carry metadata");
            let source = library_source(meta.module);
            assert!(
                source.contains(&format!("pub struct {}", meta.api)),
                "{} declares no pub struct {}",
                meta.module,
                meta.api
            );
            assert!(
                source.contains(&format!("impl {} {{", meta.api)),
                "{} has no inherent impl for {}",
                meta.module,
                meta.api
            );
            assert!(
                source.contains("pub fn new"),
                "{} exposes no constructor",
                meta.module
            );
        }
    }

    #[test]
    fn story_variants_are_unique_and_labelled() {
        for story in StoryId::ALL {
            let variants = story.variants();
            let mut ids: Vec<&str> = variants.iter().map(|(id, _)| *id).collect();
            let total = ids.len();
            ids.sort_unstable();
            ids.dedup();
            assert_eq!(ids.len(), total, "{} repeats a variant id", story.slug());

            for (id, label) in variants {
                assert!(
                    id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                    "{} has variant id {id}, which is not a slug",
                    story.slug()
                );
                assert!(!label.is_empty(), "{} has an unlabelled variant", story.slug());
            }
        }

        // The stories with a switcher must actually report their states.
        assert_eq!(StoryId::Chat.variants().len(), 2);
        assert_eq!(
            StoryId::FilterTable.variants(),
            StoryId::RecordsTable.variants(),
            "the table stories share one switcher"
        );
        assert!(StoryId::Loading.variants().is_empty());
    }

    #[test]
    fn catalog_slugs_and_sequence_are_stable_and_unique() {
        let mut slugs: Vec<&str> = StoryId::ALL.iter().map(|story| story.slug()).collect();
        let total = slugs.len();
        slugs.sort_unstable();
        slugs.dedup();
        assert_eq!(slugs.len(), total, "story slugs must be unique");

        for story in StoryId::ALL {
            assert_eq!(
                story.slug().parse::<StoryId>(),
                Ok(*story),
                "{} must round-trip through its slug",
                story.slug()
            );
        }
    }
}
