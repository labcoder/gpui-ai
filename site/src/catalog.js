const component = (sequence, slug, title, category, summary, source, api, usage, viewport = "wide") => ({
  sequence,
  slug,
  title,
  compactLabel: title,
  category,
  summary,
  source: `crates/mighty-gpui/src/${source}.rs`,
  api,
  usage,
  viewport,
  limitation: "The live browser specimen requires WebGPU; the native component remains the authoritative runtime.",
});

/** Canonical public-site metadata, kept in the same order as StoryId::ALL. */
export const components = Object.freeze([
  component(1, "loading", "Loading state", "Progress", "A token-driven pixel field for work whose duration is not yet known.", "loading", "LoadingState", 'LoadingState::new().label("Thinking")', "tall"),
  component(2, "tool-chips", "Tool chips", "Agent work", "Compact, typed status for tool calls without hiding their lifecycle.", "chip", "ToolChip", 'ToolChip::new("edit", "edit main.rs")', "tall"),
  component(3, "tasks", "Task rows", "Agent work", "Progressive task rows with stable identity and readable state.", "task", "TaskRow", 'TaskRow::new(&task_progress)', "tall"),
  component(4, "thinking", "Thinking", "Progress", "Expandable reasoning traces in structured step and prose forms.", "thinking", "Thinking", 'Thinking::new("reasoning", &trace_progress)', "tall"),
  component(5, "orbs", "Orbs", "Progress", "A reduced-motion-aware ambient signal for background AI activity.", "orbs", "Orbs", 'Orbs::new()', "tall"),
  component(6, "search", "Web search", "Agent work", "Search results with readable citations, metadata, and progressive state.", "search_results", "SearchResults", 'SearchResults::new("research", "GPUI components")'),
  component(7, "todos", "To-do list", "Agent work", "A stable-ID checklist for plans that change while an agent works.", "todo_list", "TodoList", 'TodoList::new("release-plan")', "tall"),
  component(8, "image-generation", "Image generation", "Agent work", "Image generation progress, preview, and error states in one frame.", "image_generation", "ImageGeneration", 'ImageGeneration::new("hero-art").progress(0.64)'),
  component(9, "streaming-text", "Streaming text", "Readable output", "Selectable streaming Markdown with citations, sources, and follow-ups.", "streaming_text", "StreamingText", 'StreamingText::new("answer", &content)'),
  component(10, "chat", "Chat", "Composites", "A virtualized controlled conversation with tail-follow and unread behavior.", "chat", "Chat", 'Chat::new("conversation", prompt, window, cx)', "tall"),
  component(11, "command-search", "Command search", "Navigation", "Keyboard-first command discovery backed by stable application IDs.", "command_search", "CommandSearch", 'CommandSearch::new("commands", window, cx)', "tall"),
  component(12, "sidebar-nav", "Sidebar navigation", "Navigation", "Filterable, accessible navigation for growing AI workspaces.", "sidebar_nav", "SidebarNav", 'SidebarNav::new("workspace-nav", window, cx)', "tall"),
  component(13, "fine-tune", "Fine-tune card", "Composites", "A controlled property inspector for precise model and design settings.", "fine_tune", "FineTuneCard", 'FineTuneCard::new("controls", values, typefaces, window, cx)', "tall"),
  component(14, "records-table", "Records table", "Data tables", "A controlled virtualized records grid for large, changing datasets.", "records_table", "RecordsTable", 'RecordsTable::new("accounts", "Accounts", window, cx)'),
  component(15, "diff-table", "Diff table", "Data tables", "A virtualized before-and-after proposal grid with explicit change state.", "diff_table", "DiffTable", 'DiffTable::new("proposal", "Proposed changes", window, cx)'),
  component(16, "filter-table", "Filter table", "Data tables", "A controlled task grid with typed filters and stable-row reorder motion.", "filter_table", "FilterTable", 'FilterTable::new("tasks", "Tasks", window, cx)'),
  component(17, "comparison-table", "Comparison table", "Data tables", "A bounded feature matrix with semantic values and sticky context.", "comparison_table", "ComparisonTable", 'ComparisonTable::new("plans", "Plans", window, cx)'),
  component(18, "code-block", "Code block", "Readable output", "Selectable code with language context and progressive reveal.", "code_block", "CodeBlock", 'CodeBlock::new("patch", source).language("rust")'),
  component(19, "approval", "Approval card", "Decisions", "An explicit, keyboard-operable human gate for consequential agent actions.", "approval", "ApprovalCard", 'ApprovalCard::new("deploy", "Deploy production?")', "tall"),
  component(20, "recommendation", "Recommendation card", "Decisions", "A focused recommendation with rationale and typed actions.", "recommendation", "RecommendationCard", 'RecommendationCard::new("next-step", "Ship the fix")', "tall"),
  component(21, "context", "Context cards", "Evidence", "Compact source context that preserves provenance and readable detail.", "context_card", "ContextCard", 'ContextCard::new("design-doc", "Architecture")', "tall"),
  component(22, "insights", "Insight card", "Evidence", "Paged analytical findings with chart-ready, semantic values.", "insight", "InsightCard", 'InsightCard::new("retention", "Retention improved")'),
  component(23, "prompt-bar", "Prompt bar", "Composites", "A hybrid-controlled composer with mentions, commands, models, and attachments.", "prompt_bar", "PromptBar", 'PromptBar::new("agent-prompt", window, cx)'),
  component(24, "selection-actions", "Selection actions", "Readable output", "Ask, explain, and rewrite actions anchored to selected Markdown.", "selection_actions", "SelectionActions", 'SelectionActions::new("answer-actions", markdown, window, cx)'),
]);

export const categories = Object.freeze([
  ...new Set(components.map(({ category }) => category)),
]);

export function componentBySlug(slug) {
  return components.find((entry) => entry.slug === slug);
}
