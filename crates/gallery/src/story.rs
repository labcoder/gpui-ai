//! Stable identifiers for component stories.

use std::{fmt, str::FromStr};

/// A stable route to one gallery story, or to the complete catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StoryId {
    /// The complete catalog.
    All,
    /// Pixel-grid loading state.
    Loading,
    /// Lifecycle status pill.
    Status,
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
    /// Single-choice groups and toggles.
    Form,
    /// A short sequence of questions asked before acting.
    QuestionFlow,
    /// The website's scripted hero.
    ///
    /// Addressable like any story, but deliberately absent from
    /// [`StoryId::ALL`]: it is one composition made for the home page, not a
    /// component the catalog documents.
    GuidedDemo,
    /// The themes page's comparison specimen.
    ///
    /// Same contract as [`StoryId::GuidedDemo`]: addressable, absent from the
    /// catalog. It stacks the loading, tool-chip, and context specimens in one
    /// view so the themes page boots one WebAssembly runtime instead of three
    /// — the page compares themes, and the comparison needs the same pixels
    /// re-skinned, not three separate instances.
    ThemesTrio,
    /// gpui-ai components inside upstream's `DockArea`.
    ///
    /// Same contract as [`StoryId::GuidedDemo`]: addressable, absent from the
    /// catalog. It documents no component — it is the interoperability proof
    /// that an embedded sidebar, a thread list, a chat, and an artifact
    /// compose inside someone else's docking, so there is nothing here for the
    /// website to name or size.
    DockComposition,
    /// Decorations an application paints into a component's frame.
    ///
    /// Same contract as [`StoryId::GuidedDemo`]: addressable, absent from the
    /// catalog. It documents no component — it demonstrates the slot every
    /// framed component offers, which is an extension point rather than a
    /// thing with an API of its own. The website shows it under Extensions
    /// for exactly that reason.
    Decorations,
    /// The motion lab: an instrument, not an exhibit.
    ///
    /// Same contract as [`StoryId::GuidedDemo`]: addressable, absent from the
    /// catalog. It drives the shared motion primitives through their failure
    /// cases — rapid interruption, mid-flight reversal, environment changes,
    /// arrival cascades — with live readouts and a scrub, so a primitive that
    /// jumps on reversal or schedules after settling is seen here rather
    /// than shipped. A tool for tuning the motion policy has no place on a
    /// site that documents components.
    MotionLab,
}

/// Variants the Chat story switches between.
///
/// Variant lists live here rather than beside the stories so the exported
/// catalog and the switcher toolbar cannot disagree.
pub const CHAT_STORY_VARIANTS: &[(&str, &str)] =
    &[("conversation", "Conversation"), ("welcome", "Welcome")];

/// Variants the prompt-bar story switches between.
pub const PROMPT_BAR_STORY_VARIANTS: &[(&str, &str)] = &[
    ("empty", "Empty"),
    ("ready", "Ready"),
    ("multiline", "Multiline"),
    ("running", "Running"),
    ("glyph", "Glyph submit"),
    ("gathered", "Gathered"),
];

/// Variants the command-search story switches between.
pub const COMMAND_SEARCH_STORY_VARIANTS: &[(&str, &str)] = &[
    ("populated", "Populated"),
    ("empty", "Empty catalog"),
    ("no-results", "No results"),
];

/// Variants the form story switches between.
pub const FORM_STORY_VARIANTS: &[(&str, &str)] = &[("choices", "Choices"), ("toggles", "Toggles")];

/// Variants the decorations story switches between.
pub const DECORATION_STORY_VARIANTS: &[(&str, &str)] = &[
    ("dither", "Dither"),
    ("pop-art", "Pop art"),
    ("engrave", "Cross-hatch"),
    ("halftone", "Halftone"),
    ("ripple", "Ripple"),
    ("veil", "Veil"),
];

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

/// The site hero's height in pixels at the website's demo width.
///
/// Separate from [`StoryMeta::height`] because the hero carries no component
/// metadata, and measured settled rather than idle: the frame has to hold the
/// finished answer, not the composer the demo opens on. The
/// `the_hero_height_matches_what_the_settled_demo_measures` test fails when
/// the script changes shape and this number does not.
pub const HERO_HEIGHT: u32 = 730;

/// Which way a story's content outgrows the frame it is shown in.
///
/// Declared rather than inferred. The exporter used to decide this from
/// [`StoryMeta::height`], on the reasoning that a tall story is the one with
/// something to say about staying reachable — but height measures the story as
/// it is composed, including any prose beneath the component, so editing that
/// prose silently rewrote the published claim for four components. The two
/// facts are independent, so they are recorded independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overflow {
    /// Content grows downward, and the claim is that it stays reachable.
    Vertical,
    /// Content is wider than the frame, and the claim is that context survives.
    Wide,
}

/// How much horizontal room a story needs to be judged fairly.
///
/// Most components are specimens: a chip, a badge, a card, read beside
/// prose at the width prose is read at. Some are working surfaces — a data
/// grid, a transcript, a navigation pane — and at a prose column's width
/// they are squeezed rather than shown, which invites the reader to judge
/// the squeeze instead of the component.
///
/// Both widths sit inside the website's own demo column, so a story is
/// measured at the width it will be drawn at and the reserved height stays
/// honest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoryWidth {
    /// The prose column a specimen is read in.
    Column,
    /// The full demo column, for surfaces that carry a layout.
    Wide,
}

impl StoryWidth {
    /// The frame's maximum width in pixels.
    pub const fn max_width(self) -> f32 {
        match self {
            Self::Column => 640.,
            Self::Wide => 900.,
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
    /// The story's natural height in pixels at the website's demo width.
    ///
    /// **A website concern, and only that.** The site reserves this much
    /// space before a demo's WASM has booted, and sizes the poster it shows
    /// where WebGPU is unavailable. Nothing lays out from it: not the
    /// gallery, which draws every story at whatever size it actually is,
    /// and certainly not an application using this library — a consumer of
    /// `gpui-ai` never sees these numbers. Read by the catalog export and
    /// by the test that keeps it honest, and by nothing else.
    ///
    /// Measured rather than guessed: a story is centred in its frame, so a
    /// frame sized from anything else leaves dead space or clips. The
    /// `story_heights_match_what_the_stories_measure` test fails when a story
    /// changes shape and this number does not.
    pub height: u32,
    /// Which way this story's content outgrows its frame.
    pub overflow: Overflow,
    /// How much horizontal room the story needs to be judged fairly.
    pub width: StoryWidth,
}

impl StoryId {
    /// Every individually addressable component story, in catalog order.
    pub const ALL: &'static [Self] = &[
        Self::Loading,
        Self::Status,
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
        Self::Form,
        Self::QuestionFlow,
    ];

    /// Stable URL slug for this selection.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Loading => "loading",
            Self::Status => "status",
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
            Self::Form => "form-controls",
            Self::QuestionFlow => "question-flow",
            Self::GuidedDemo => "guided-demo",
            Self::Decorations => "decorations",
            Self::MotionLab => "motion-lab",
            Self::ThemesTrio => "themes-trio",
            Self::DockComposition => "dock-composition",
        }
    }

    /// Human-readable story title.
    pub const fn title(self) -> &'static str {
        match self {
            Self::All => "All components",
            Self::Loading => "Loading state",
            Self::Status => "Status badge",
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
            Self::Form => "Form controls",
            Self::QuestionFlow => "Question flow",
            Self::GuidedDemo => "Guided demo",
            Self::Decorations => "Decorations",
            Self::ThemesTrio => "Themes trio",
            Self::DockComposition => "Dock composition",
            Self::MotionLab => "Motion lab",
        }
    }

    /// Catalog metadata for the website.
    ///
    /// [`StoryId::All`] is the whole-catalog view rather than a component, so
    /// it has no entry.
    pub const fn meta(self) -> Option<StoryMeta> {
        Some(match self {
            // Neither the catalog view, the site hero, the themes-page
            // specimen, the dock interoperability proof, nor the motion
            // instrument is a component.
            Self::All
            | Self::GuidedDemo
            | Self::Decorations
            | Self::ThemesTrio
            | Self::DockComposition
            | Self::MotionLab => {
                return None;
            }
            Self::Loading => StoryMeta {
                category: "Progress",
                summary: "A token-driven pixel field for work whose duration is not yet known.",
                module: "loading",
                api: "LoadingState",
                usage: crate::usage::LOADING_STATE,
                height: 52,
                width: StoryWidth::Column,
                overflow: Overflow::Wide,
            },
            Self::Status => StoryMeta {
                category: "Progress",
                summary: "One status pill for every lifecycle, swapping states in a fixed slot.",
                module: "status",
                api: "StatusBadge",
                usage: crate::usage::STATUS_BADGE,
                height: 84,
                width: StoryWidth::Column,
                overflow: Overflow::Wide,
            },
            Self::ToolChips => StoryMeta {
                category: "Agent work",
                summary: "Compact, typed status for tool calls without hiding their lifecycle.",
                module: "chip",
                api: "ToolChip",
                usage: crate::usage::TOOL_CHIP,
                height: 84,
                width: StoryWidth::Column,
                overflow: Overflow::Wide,
            },
            Self::ToolCalls => StoryMeta {
                category: "Agent work",
                summary: "Collapsible tool-call cards with input, output, approval, and a shimmering group.",
                module: "tool_call",
                api: "ToolCall",
                usage: crate::usage::TOOL_CALL,
                height: 650,
                width: StoryWidth::Column,
                overflow: Overflow::Vertical,
            },
            Self::Tasks => StoryMeta {
                category: "Agent work",
                summary: "Progressive task rows with stable identity and readable state.",
                module: "task",
                api: "TaskRow",
                usage: crate::usage::TASK_ROW,
                height: 144,
                width: StoryWidth::Column,
                overflow: Overflow::Wide,
            },
            Self::Thinking => StoryMeta {
                category: "Progress",
                summary: "Expandable reasoning traces in structured step and prose forms.",
                module: "thinking",
                api: "Thinking",
                usage: crate::usage::THINKING,
                height: 306,
                width: StoryWidth::Column,
                overflow: Overflow::Wide,
            },
            Self::Orbs => StoryMeta {
                category: "Progress",
                summary: "A reduced-motion-aware ambient signal for background AI activity.",
                module: "orbs",
                api: "Orbs",
                usage: crate::usage::ORBS,
                height: 195,
                width: StoryWidth::Column,
                overflow: Overflow::Wide,
            },
            Self::Search => StoryMeta {
                category: "Agent work",
                summary: "Search results with readable citations, metadata, and progressive state.",
                module: "search_results",
                api: "SearchResults",
                usage: crate::usage::SEARCH_RESULTS,
                height: 161,
                width: StoryWidth::Column,
                overflow: Overflow::Wide,
            },
            Self::Todos => StoryMeta {
                category: "Agent work",
                summary: "A stable-ID checklist for plans that change while an agent works.",
                module: "todo_list",
                api: "TodoList",
                usage: crate::usage::TODO_LIST,
                height: 234,
                width: StoryWidth::Column,
                overflow: Overflow::Wide,
            },
            Self::ImageGeneration => StoryMeta {
                category: "Agent work",
                summary: "Image generation progress, preview, and error states in one frame.",
                module: "image_generation",
                api: "ImageGeneration",
                usage: crate::usage::IMAGE_GENERATION,
                height: 212,
                width: StoryWidth::Column,
                overflow: Overflow::Wide,
            },
            Self::StreamingText => StoryMeta {
                category: "Readable output",
                summary: "Selectable streaming Markdown with citations, sources, and follow-ups.",
                module: "streaming_text",
                api: "StreamingText",
                usage: crate::usage::STREAMING_TEXT,
                height: 426,
                width: StoryWidth::Column,
                overflow: Overflow::Vertical,
            },
            Self::Chat => StoryMeta {
                category: "Composites",
                summary: "A virtualized controlled conversation with tail-follow, unread behavior, in-place edit, and branch versions.",
                module: "chat",
                api: "Chat",
                usage: crate::usage::CHAT,
                height: 595,
                width: StoryWidth::Wide,
                overflow: Overflow::Vertical,
            },
            Self::Suggestions => StoryMeta {
                category: "Composites",
                summary: "Starter and follow-up prompt chips that ripple in and report stable IDs.",
                module: "suggestions",
                api: "Suggestions",
                usage: crate::usage::SUGGESTIONS,
                height: 132,
                width: StoryWidth::Column,
                overflow: Overflow::Wide,
            },
            Self::Attachments => StoryMeta {
                category: "Composites",
                summary: "Composer and message attachments with thumbnails, kinds, upload state, and typed open or remove events.",
                module: "attachment",
                api: "AttachmentStrip",
                usage: crate::usage::ATTACHMENT_STRIP,
                height: 486,
                width: StoryWidth::Column,
                overflow: Overflow::Vertical,
            },
            Self::Artifact => StoryMeta {
                category: "Composites",
                summary: "A side panel for generated documents and code with preview and source views, versions, actions, and streaming state.",
                module: "artifact",
                api: "ArtifactPanel",
                usage: crate::usage::ARTIFACT_PANEL,
                height: 504,
                width: StoryWidth::Wide,
                overflow: Overflow::Vertical,
            },
            Self::ContextMeter => StoryMeta {
                category: "Progress",
                summary: "Context-window usage as a ring, bar, or text with severity tones and a breakdown.",
                module: "context_meter",
                api: "ContextMeter",
                usage: crate::usage::CONTEXT_METER,
                height: 120,
                width: StoryWidth::Column,
                overflow: Overflow::Wide,
            },
            Self::CommandSearch => StoryMeta {
                category: "Navigation",
                summary: "Keyboard-first command discovery backed by stable application IDs.",
                module: "command_search",
                api: "CommandSearch",
                usage: crate::usage::COMMAND_SEARCH,
                height: 387,
                width: StoryWidth::Wide,
                overflow: Overflow::Vertical,
            },
            Self::SidebarNav => StoryMeta {
                category: "Navigation",
                summary: "Filterable, accessible navigation for growing AI workspaces.",
                module: "sidebar_nav",
                api: "SidebarNav",
                usage: crate::usage::SIDEBAR_NAV,
                height: 284,
                width: StoryWidth::Wide,
                overflow: Overflow::Wide,
            },
            Self::ThreadList => StoryMeta {
                category: "Navigation",
                summary: "A grouped conversation list with search, archived threads, and typed row actions.",
                module: "thread_list",
                api: "ThreadList",
                usage: crate::usage::THREAD_LIST,
                height: 436,
                width: StoryWidth::Wide,
                overflow: Overflow::Vertical,
            },
            Self::FineTune => StoryMeta {
                category: "Composites",
                summary: "A controlled property inspector for precise model and design settings.",
                module: "fine_tune",
                api: "FineTuneCard",
                usage: crate::usage::FINE_TUNE_CARD,
                height: 738,
                width: StoryWidth::Wide,
                overflow: Overflow::Vertical,
            },
            Self::RecordsTable => StoryMeta {
                category: "Data tables",
                summary: "A controlled virtualized records grid for large, changing datasets.",
                module: "records_table",
                api: "RecordsTable",
                usage: crate::usage::RECORDS_TABLE,
                height: 288,
                width: StoryWidth::Wide,
                overflow: Overflow::Wide,
            },
            Self::DiffTable => StoryMeta {
                category: "Data tables",
                summary: "A virtualized before-and-after proposal grid with explicit change state.",
                module: "diff_table",
                api: "DiffTable",
                usage: crate::usage::DIFF_TABLE,
                height: 288,
                width: StoryWidth::Wide,
                overflow: Overflow::Wide,
            },
            Self::FilterTable => StoryMeta {
                category: "Data tables",
                summary: "A controlled task grid with typed filters and stable-row reorder motion.",
                module: "filter_table",
                api: "FilterTable",
                usage: crate::usage::FILTER_TABLE,
                height: 288,
                width: StoryWidth::Wide,
                overflow: Overflow::Wide,
            },
            Self::ComparisonTable => StoryMeta {
                category: "Data tables",
                summary: "A bounded feature matrix with semantic values and sticky context.",
                module: "comparison_table",
                api: "ComparisonTable",
                usage: crate::usage::COMPARISON_TABLE,
                height: 432,
                width: StoryWidth::Wide,
                overflow: Overflow::Wide,
            },
            Self::CodeBlock => StoryMeta {
                category: "Readable output",
                summary: "Selectable code with language context and progressive reveal.",
                module: "code_block",
                api: "CodeBlock",
                usage: crate::usage::CODE_BLOCK,
                height: 223,
                width: StoryWidth::Column,
                overflow: Overflow::Wide,
            },
            Self::CodeDiff => StoryMeta {
                category: "Readable output",
                summary: "A unified patch with line gutters, change tints, per-hunk accept or reject, and a copyable source.",
                module: "code_diff",
                api: "CodeDiff",
                usage: crate::usage::CODE_DIFF,
                height: 469,
                width: StoryWidth::Wide,
                overflow: Overflow::Vertical,
            },
            Self::Approval => StoryMeta {
                category: "Decisions",
                summary: "An explicit, keyboard-operable human gate with destructive and always-allow variants and resolved states.",
                module: "approval",
                api: "ApprovalCard",
                usage: crate::usage::APPROVAL_CARD,
                height: 449,
                width: StoryWidth::Column,
                overflow: Overflow::Wide,
            },
            Self::Plan => StoryMeta {
                category: "Decisions",
                summary: "An agent's proposed steps with typed per-step status, approve or reject while proposed, and resolved states.",
                module: "plan",
                api: "PlanCard",
                usage: crate::usage::PLAN_CARD,
                height: 402,
                width: StoryWidth::Column,
                overflow: Overflow::Vertical,
            },
            Self::Recommendation => StoryMeta {
                category: "Decisions",
                summary: "A focused recommendation with rationale and typed actions.",
                module: "recommendation",
                api: "RecommendationCard",
                usage: crate::usage::RECOMMENDATION_CARD,
                height: 262,
                width: StoryWidth::Column,
                overflow: Overflow::Wide,
            },
            Self::Context => StoryMeta {
                category: "Evidence",
                summary: "Compact source context that preserves provenance and readable detail.",
                module: "context_card",
                api: "ContextCard",
                usage: crate::usage::CONTEXT_CARD,
                height: 216,
                width: StoryWidth::Column,
                overflow: Overflow::Wide,
            },
            Self::Insights => StoryMeta {
                category: "Evidence",
                summary: "Paged analytical findings with chart-ready, semantic values.",
                module: "insight",
                api: "InsightCard",
                usage: crate::usage::INSIGHT_CARD,
                height: 452,
                width: StoryWidth::Column,
                overflow: Overflow::Wide,
            },
            Self::PromptBar => StoryMeta {
                category: "Composites",
                summary: "A hybrid-controlled composer with mentions, commands, models, and attachments.",
                module: "prompt_bar",
                api: "PromptBar",
                usage: crate::usage::PROMPT_BAR,
                height: 432,
                width: StoryWidth::Column,
                overflow: Overflow::Vertical,
            },
            Self::Voice => StoryMeta {
                category: "Composites",
                summary: "Dictate and speak controls with a live level meter and interim transcript, as typed intent.",
                module: "voice",
                api: "VoiceControls",
                usage: crate::usage::VOICE_CONTROLS,
                height: 90,
                width: StoryWidth::Column,
                overflow: Overflow::Wide,
            },
            Self::Queue => StoryMeta {
                category: "Composites",
                summary: "Prompts waiting while the agent runs, with reorder, edit, send-now, remove, and clear by stable ID.",
                module: "queue",
                api: "MessageQueue",
                usage: crate::usage::MESSAGE_QUEUE,
                height: 238,
                width: StoryWidth::Column,
                overflow: Overflow::Wide,
            },
            Self::SelectionActions => StoryMeta {
                category: "Readable output",
                summary: "Ask, explain, and rewrite actions anchored to selected Markdown.",
                module: "selection_actions",
                api: "SelectionActions",
                usage: crate::usage::SELECTION_ACTIONS,
                height: 288,
                width: StoryWidth::Column,
                overflow: Overflow::Wide,
            },
            Self::Form => StoryMeta {
                category: "Human in the loop",
                summary: "Single-choice groups and toggles in the library's own grammar.",
                module: "form",
                api: "ChoiceGroup",
                usage: crate::usage::CHOICE_GROUP,
                height: 300,
                width: StoryWidth::Column,
                overflow: Overflow::Vertical,
            },
            Self::QuestionFlow => StoryMeta {
                category: "Human in the loop",
                summary: "A short sequence of questions an agent asks before it acts.",
                module: "question_flow",
                api: "QuestionFlow",
                usage: crate::usage::QUESTION_FLOW,
                height: 336,
                width: StoryWidth::Column,
                overflow: Overflow::Vertical,
            },
        })
    }

    /// The states this story's switcher offers, as `(id, label)` pairs.
    ///
    /// Empty for stories that show one composition.
    pub const fn variants(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Chat => CHAT_STORY_VARIANTS,
            Self::PromptBar => PROMPT_BAR_STORY_VARIANTS,
            Self::CommandSearch => COMMAND_SEARCH_STORY_VARIANTS,
            Self::Decorations => DECORATION_STORY_VARIANTS,
            Self::Form => FORM_STORY_VARIANTS,
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
        // Addressable, but not catalog components, so not in ALL.
        if slug == Self::GuidedDemo.slug() {
            return Ok(Self::GuidedDemo);
        }
        if slug == Self::Decorations.slug() {
            return Ok(Self::Decorations);
        }
        if slug == Self::ThemesTrio.slug() {
            return Ok(Self::ThemesTrio);
        }
        if slug == Self::DockComposition.slug() {
            return Ok(Self::DockComposition);
        }
        if slug == Self::MotionLab.slug() {
            return Ok(Self::MotionLab);
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
    fn dock_composition_is_addressable_but_not_a_catalog_component() {
        assert_eq!(StoryId::DockComposition.slug(), "dock-composition");
        assert_eq!(StoryId::DockComposition.title(), "Dock composition");
        assert_eq!(
            "dock-composition".parse::<StoryId>(),
            Ok(StoryId::DockComposition)
        );
        // It documents interoperability, not a component, so the catalog and
        // the website must not list or size it.
        assert!(!StoryId::ALL.contains(&StoryId::DockComposition));
        assert!(StoryId::DockComposition.meta().is_none());
        assert!(StoryId::DockComposition.variants().is_empty());
    }

    #[test]
    fn the_motion_lab_is_addressable_but_not_a_catalog_component() {
        assert_eq!(StoryId::MotionLab.slug(), "motion-lab");
        assert_eq!(StoryId::MotionLab.title(), "Motion lab");
        assert_eq!("motion-lab".parse::<StoryId>(), Ok(StoryId::MotionLab));
        // It is an instrument for tuning the motion policy, not a component,
        // so the catalog and the website must not list or size it.
        assert!(!StoryId::ALL.contains(&StoryId::MotionLab));
        assert!(StoryId::MotionLab.meta().is_none());
        assert!(StoryId::MotionLab.variants().is_empty());
    }

    #[test]
    fn every_component_story_carries_catalog_metadata() {
        for story in StoryId::ALL {
            let meta = story
                .meta()
                .unwrap_or_else(|| panic!("{} has no catalog metadata", story.slug()));
            assert!(
                !meta.category.is_empty(),
                "{} has no category",
                story.slug()
            );
            assert!(
                meta.summary.len() > 20,
                "{} needs a real summary, not a label",
                story.slug()
            );
            assert!(!story.title().is_empty());
            assert!(
                meta.module
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '_'),
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
                    id.chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                    "{} has variant id {id}, which is not a slug",
                    story.slug()
                );
                assert!(
                    !label.is_empty(),
                    "{} has an unlabelled variant",
                    story.slug()
                );
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
}
