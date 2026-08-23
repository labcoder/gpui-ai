const componentEventTypes = Object.freeze({
  "tool-chips": "ToolChipEvent",
  thinking: "ThinkingEvent",
  search: "SearchResultsEvent",
  todos: "TodoListEvent",
  "streaming-text": "StreamingTextEvent",
  chat: "ChatEvent",
  "command-search": "CommandSearchEvent",
  "sidebar-nav": "SidebarNavEvent",
  "thread-list": "ThreadListEvent",
  "fine-tune": "FineTuneEvent",
  "records-table": "RecordsTableEvent",
  "diff-table": "DiffTableEvent",
  "code-diff": "CodeDiffEvent",
  "filter-table": "FilterTableEvent",
  "comparison-table": "ComparisonTableEvent",
  approval: "ApprovalEvent",
  plan: "PlanEvent",
  recommendation: "RecommendationEvent",
  context: "ContextCardEvent",
  insights: "InsightEvent",
  "prompt-bar": "PromptBarEvent",
  suggestions: "SuggestionsEvent",
  attachments: "AttachmentEvent",
  artifact: "ArtifactPanelEvent",
  "selection-actions": "SelectionActionsEvent",
  "tool-calls": ["ToolCallEvent", "ToolGroupEvent"],
});

const joinEvents = (events) =>
  events.length > 1 ? `${events.slice(0, -1).join(", ")} and ${events.at(-1)}` : events[0];

const component = (sequence, slug, title, category, summary, source, api, usage, viewport = "wide") => {
  const events = [componentEventTypes[slug] ?? []].flat();
  return {
    sequence,
    slug,
    title,
    compactLabel: title,
    category,
    summary,
    source: `crates/gpui-ai/src/${source}.rs`,
    api,
    usage,
    viewport,
    events,
    event: events[0] ?? null,
    limitation: "The live browser specimen requires WebGPU; the native component remains the authoritative runtime.",
    behavior: Object.freeze({
      ownership: `${api} renders state supplied by the application; it does not own durable work.`,
      interaction: events.length
        ? `Interactive intent is reported through the typed ${joinEvents(events)} contract${events.length > 1 ? "s" : ""} and stable application IDs.`
        : "This presentation surface adds no component-specific interaction event.",
      semantics: summary,
      overflow: viewport === "tall"
        ? "Growing content remains reachable in a bounded vertical surface; reduced motion preserves a useful state."
        : "Wide content retains context in a bounded surface; reduced motion preserves a useful state.",
    }),
  };
};

/** Canonical public-site metadata, kept in the same order as StoryId::ALL. */
export const components = Object.freeze([
  component(1, "loading", "Loading state", "Progress", "A token-driven pixel field for work whose duration is not yet known.", "loading", "LoadingState", 'LoadingState::new().label("Thinking")', "tall"),
  component(2, "tool-chips", "Tool chips", "Agent work", "Compact, typed status for tool calls without hiding their lifecycle.", "chip", "ToolChip", 'ToolChip::new("edit", "edit main.rs")', "tall"),
  component(3, "tool-calls", "Tool calls", "Agent work", "Collapsible tool-call cards with input, output, approval, and a shimmering group.", "tool_call", "ToolCall", 'ToolCall::new(&call_progress)', "tall"),
  component(4, "tasks", "Task rows", "Agent work", "Progressive task rows with stable identity and readable state.", "task", "TaskRow", 'TaskRow::new(&task_progress)', "tall"),
  component(5, "thinking", "Thinking", "Progress", "Expandable reasoning traces in structured step and prose forms.", "thinking", "Thinking", 'Thinking::new("reasoning", &trace_progress)', "tall"),
  component(6, "orbs", "Orbs", "Progress", "A reduced-motion-aware ambient signal for background AI activity.", "orbs", "Orbs", 'Orbs::new()', "tall"),
  component(7, "search", "Web search", "Agent work", "Search results with readable citations, metadata, and progressive state.", "search_results", "SearchResults", 'SearchResults::new("research", "GPUI components")'),
  component(8, "todos", "To-do list", "Agent work", "A stable-ID checklist for plans that change while an agent works.", "todo_list", "TodoList", 'TodoList::new("release-plan")', "tall"),
  component(9, "image-generation", "Image generation", "Agent work", "Image generation progress, preview, and error states in one frame.", "image_generation", "ImageGeneration", 'ImageGeneration::new("hero-art").progress(0.64)'),
  component(10, "streaming-text", "Streaming text", "Readable output", "Selectable streaming Markdown with citations, sources, and follow-ups.", "streaming_text", "StreamingText", 'StreamingText::new("answer", &content)'),
  component(11, "chat", "Chat", "Composites", "A virtualized controlled conversation with tail-follow, unread behavior, in-place edit, and branch versions.", "chat", "Chat", 'Chat::new("conversation", prompt, window, cx)', "tall"),
  component(12, "suggestions", "Suggestions", "Composites", "Starter and follow-up prompt chips that ripple in and report stable IDs.", "suggestions", "Suggestions", 'Suggestions::new("starters")'),
  component(13, "attachments", "Attachment previews", "Composites", "Composer and message attachments with thumbnails, kinds, upload state, and typed open or remove events.", "attachment", "AttachmentStrip", 'AttachmentStrip::new("files").items(attachments)'),
  component(14, "artifact", "Artifact panel", "Composites", "A side panel for generated documents and code with preview and source views, versions, actions, and streaming state.", "artifact", "ArtifactPanel", 'ArtifactPanel::new("doc", &artifact)', "tall"),
  component(15, "context-meter", "Context meter", "Progress", "Context-window usage as a ring, bar, or text with severity tones and a breakdown.", "context_meter", "ContextMeter", 'ContextMeter::new("context", &usage)'),
  component(16, "command-search", "Command search", "Navigation", "Keyboard-first command discovery backed by stable application IDs.", "command_search", "CommandSearch", 'CommandSearch::new("commands", window, cx)', "tall"),
  component(17, "sidebar-nav", "Sidebar navigation", "Navigation", "Filterable, accessible navigation for growing AI workspaces.", "sidebar_nav", "SidebarNav", 'SidebarNav::new("workspace-nav", window, cx)', "tall"),
  component(18, "thread-list", "Thread list", "Navigation", "A grouped conversation list with search, archived threads, and typed row actions.", "thread_list", "ThreadList", 'ThreadList::new("threads", window, cx)', "tall"),
  component(19, "fine-tune", "Fine-tune card", "Composites", "A controlled property inspector for precise model and design settings.", "fine_tune", "FineTuneCard", 'FineTuneCard::new("controls", values, typefaces, window, cx)', "tall"),
  component(20, "records-table", "Records table", "Data tables", "A controlled virtualized records grid for large, changing datasets.", "records_table", "RecordsTable", 'RecordsTable::new("accounts", "Accounts", window, cx)'),
  component(21, "diff-table", "Diff table", "Data tables", "A virtualized before-and-after proposal grid with explicit change state.", "diff_table", "DiffTable", 'DiffTable::new("proposal", "Proposed changes", window, cx)'),
  component(22, "filter-table", "Filter table", "Data tables", "A controlled task grid with typed filters and stable-row reorder motion.", "filter_table", "FilterTable", 'FilterTable::new("tasks", "Tasks", window, cx)'),
  component(23, "comparison-table", "Comparison table", "Data tables", "A bounded feature matrix with semantic values and sticky context.", "comparison_table", "ComparisonTable", 'ComparisonTable::new("plans", "Plans", window, cx)'),
  component(24, "code-block", "Code block", "Readable output", "Selectable code with language context and progressive reveal.", "code_block", "CodeBlock", 'CodeBlock::new("patch", source).language("rust")'),
  component(25, "code-diff", "Code diff", "Readable output", "A unified patch with line gutters, change tints, per-hunk accept or reject, and a copyable source.", "code_diff", "CodeDiff", 'CodeDiff::new("patch", &file).reviewable(true)', "tall"),
  component(26, "approval", "Approval card", "Decisions", "An explicit, keyboard-operable human gate with destructive and always-allow variants and resolved states.", "approval", "ApprovalCard", 'ApprovalCard::new("deploy", "Deploy production?")', "tall"),
  component(27, "plan", "Plan card", "Decisions", "An agent's proposed steps with typed per-step status, approve or reject while proposed, and resolved states.", "plan", "PlanCard", 'PlanCard::new("rollout", "Switch bulk orders")', "tall"),
  component(28, "recommendation", "Recommendation card", "Decisions", "A focused recommendation with rationale and typed actions.", "recommendation", "RecommendationCard", 'RecommendationCard::new("next-step", "Ship the fix")', "tall"),
  component(29, "context", "Context cards", "Evidence", "Compact source context that preserves provenance and readable detail.", "context_card", "ContextCard", 'ContextCard::new("design-doc", "Architecture")', "tall"),
  component(30, "insights", "Insight card", "Evidence", "Paged analytical findings with chart-ready, semantic values.", "insight", "InsightCard", 'InsightCard::new("retention", "Retention improved")'),
  component(31, "prompt-bar", "Prompt bar", "Composites", "A hybrid-controlled composer with mentions, commands, models, and attachments.", "prompt_bar", "PromptBar", 'PromptBar::new("agent-prompt", window, cx)'),
  component(32, "selection-actions", "Selection actions", "Readable output", "Ask, explain, and rewrite actions anchored to selected Markdown.", "selection_actions", "SelectionActions", 'SelectionActions::new("answer-actions", markdown, window, cx)'),
]);

export const categories = Object.freeze([
  ...new Set(components.map(({ category }) => category)),
]);

export function componentBySlug(slug) {
  return components.find((entry) => entry.slug === slug);
}
