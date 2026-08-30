//! Canonical catalog expressions: displayed verbatim and type-checked by Rust.
//!
//! Context arguments are supplied only for compilation, never by starting a GUI.

#[cfg(test)]
use gpui::{Context, Entity, SharedString, Window};
#[cfg(test)]
use gpui_ai::prelude::*;

macro_rules! example {
    ($name:ident($($parameter:ident: $ty:ty),*) -> $result:ty = $expression:expr) => {
        pub(super) const $name: &str = stringify!($expression);
        #[cfg(test)]
        const _: fn($($ty),*) -> $result = |$($parameter),*| $expression;
    };
}

example!(LOADING_STATE() -> LoadingState = LoadingState::new().label("Thinking"));
example!(STATUS_BADGE() -> StatusBadge = StatusBadge::new("review", "Needs review"));
example!(TOOL_CHIP() -> ToolChip = ToolChip::new("edit", "edit main.rs"));
example!(TOOL_CALL(call_progress: Progressive<ToolInvocation>) -> ToolCall = ToolCall::new(&call_progress));
example!(TASK_ROW(task_progress: Progressive<TaskSnapshot>) -> TaskRow = TaskRow::new(&task_progress));
example!(THINKING(trace_progress: Progressive<ThinkingTrace>) -> Thinking = Thinking::new("reasoning", &trace_progress));
example!(ORBS() -> Orbs = Orbs::new());
example!(SEARCH_RESULTS() -> SearchResults = SearchResults::new("research", "GPUI components"));
example!(TODO_LIST() -> TodoList = TodoList::new("release-plan"));
example!(IMAGE_GENERATION() -> ImageGeneration = ImageGeneration::new("hero-art").progress(0.64));
example!(STREAMING_TEXT(content: StreamedContent) -> StreamingText = StreamingText::new("answer", &content));
example!(CHAT(prompt: Entity<PromptBar>, window: &mut Window, cx: &mut Context<Chat>) -> Chat = Chat::new("conversation", prompt, window, cx));
example!(SUGGESTIONS() -> Suggestions = Suggestions::new("starters"));
example!(ATTACHMENT_STRIP(attachments: Vec<Attachment>) -> AttachmentStrip = AttachmentStrip::new("files").items(attachments));
example!(ARTIFACT_PANEL(artifact: Artifact) -> ArtifactPanel = ArtifactPanel::new("doc", &artifact));
example!(CONTEXT_METER(usage: ContextUsage) -> ContextMeter = ContextMeter::new("context", &usage));
example!(COMMAND_SEARCH(window: &mut Window, cx: &mut Context<CommandSearch>) -> CommandSearch = CommandSearch::new("commands", window, cx));
example!(SIDEBAR_NAV(window: &mut Window, cx: &mut Context<SidebarNav>) -> SidebarNav = SidebarNav::new("workspace-nav", window, cx));
example!(THREAD_LIST(window: &mut Window, cx: &mut Context<ThreadList>) -> ThreadList = ThreadList::new("threads", window, cx));
example!(FINE_TUNE_CARD(values: FineTuneValues, typefaces: Vec<FineTuneTypeface>, window: &mut Window, cx: &mut Context<FineTuneCard>) -> FineTuneCard = FineTuneCard::new("controls", values, typefaces, window, cx));
example!(RECORDS_TABLE(window: &mut Window, cx: &mut Context<RecordsTable>) -> RecordsTable = RecordsTable::new("accounts", "Accounts", window, cx));
example!(DIFF_TABLE(window: &mut Window, cx: &mut Context<DiffTable>) -> DiffTable = DiffTable::new("proposal", "Proposed changes", window, cx));
example!(FILTER_TABLE(window: &mut Window, cx: &mut Context<FilterTable>) -> FilterTable = FilterTable::new("tasks", "Tasks", window, cx));
example!(COMPARISON_TABLE(window: &mut Window, cx: &mut Context<ComparisonTable>) -> ComparisonTable = ComparisonTable::new("plans", "Plans", window, cx));
example!(CODE_BLOCK(source: SharedString) -> CodeBlock = CodeBlock::new("patch", source).language("rust"));
example!(CODE_DIFF(file: DiffFile) -> CodeDiff = CodeDiff::new("patch", &file).reviewable(true));
example!(APPROVAL_CARD() -> ApprovalCard = ApprovalCard::new("deploy", "Deploy production?"));
example!(PLAN_CARD() -> PlanCard = PlanCard::new("rollout", "Switch bulk orders"));
example!(RECOMMENDATION_CARD() -> RecommendationCard = RecommendationCard::new("next-step", "Ship the fix"));
example!(CONTEXT_CARD() -> ContextCard = ContextCard::new("design-doc", "Architecture"));
example!(INSIGHT_CARD() -> InsightCard = InsightCard::new("retention", "Retention improved"));
example!(PROMPT_BAR(window: &mut Window, cx: &mut Context<PromptBar>) -> PromptBar = PromptBar::new("agent-prompt", window, cx));
example!(VOICE_CONTROLS() -> VoiceControls = VoiceControls::new("voice", VoiceState::Idle));
example!(MESSAGE_QUEUE(queued: Vec<QueuedMessage>) -> MessageQueue = MessageQueue::new("queue").items(queued));
example!(CHOICE_GROUP() -> ChoiceGroup = ChoiceGroup::new("flavours", "How many flavours?").options([ChoiceOption::new("three", "Three"), ChoiceOption::new("five", "Five")]));
example!(QUESTION_FLOW() -> QuestionFlow = QuestionFlow::new("launch", "Before I draft the plan").questions([Question::new("flavours", "How many flavours?").options([ChoiceOption::new("three", "Three")])]));
example!(SELECTION_ACTIONS(markdown: SharedString, window: &mut Window, cx: &mut Context<SelectionActions>) -> SelectionActions = SelectionActions::new("answer-actions", markdown, window, cx));
