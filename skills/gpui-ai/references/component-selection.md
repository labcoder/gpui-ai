# Component selection

Choose by the user's job, not by visual resemblance. gpui-ai owns AI-specific
meaning; gpui-component owns general application controls.

Read the [generated component index](generated/components.md) after choosing a
family. It contains the exact API names, compiled starting expressions, typed
events, and source modules for this checkout.

## First decide the layer

Use gpui-component directly for ordinary buttons, inputs, select menus,
dialogs, sheets, notifications, tabs, docking, resizable panels, and generic
lists or tables. A screen does not become gpui-ai merely because an AI feature
opens it.

Use gpui-ai when the component needs domain semantics such as:

- content arriving progressively from a model or tool;
- a tool invocation, task, reasoning trace, or context budget;
- a person's approval, clarification, plan review, or recommendation;
- agent evidence, search results, generated artifacts, or code review;
- prompt composition, queued messages, conversation history, or agent-aware
  navigation;
- data tables whose states and actions belong to an agent workflow.

## Common distinctions

### Waiting and progress

- Use `LoadingState` for indeterminate work that deserves a labeled surface.
- Use `Orbs` for ambient activity when there is no useful progress detail yet.
- Use `StatusBadge` for compact lifecycle state.
- Use `TaskRow` or `TodoList` when stable work items progress independently.
- Use `ContextMeter` for capacity consumed, not task completion.

### Output and evidence

- Use `StreamingText` for readable model prose, citations, sources, and
  follow-ups.
- Use `CodeBlock` for one code body and `CodeDiff` when a person can review a
  proposed change.
- Use `SearchResults` for query/result/source structure.
- Use `ContextCard` for one retrieved source and `InsightCard` for a paged
  analytical claim with metrics.
- Use `ArtifactPanel` when a generated document or code artifact has views,
  versions, and actions separate from the conversation.

### Human decisions

- Use `QuestionFlow` to gather a short sequence of answers before the agent
  forms a plan.
- Use `ChoiceGroup` or `Toggle` when the form is part of another surface and
  does not need step progression.
- Use `PlanCard` to present proposed steps and their execution state.
- Use `ApprovalCard` for a concrete allow/deny gate. It is not a substitute for
  collecting missing requirements.
- Use `RecommendationCard` when the system proposes a next action without
  representing it as a permission boundary.

### Conversations

- Use `PromptBar` as the composer and retain it as an entity.
- Use `Chat` when the application needs a virtualized controlled transcript,
  unread reconciliation, editing, branch versions, and a composer together.
- Add `Suggestions` for starter or follow-up prompts, `AttachmentStrip` for
  upload snapshots, `MessageQueue` for prompts waiting behind active work, and
  `VoiceControls` only when those jobs exist.
- Do not build an entire `Chat` merely to display one answer; use
  `StreamingText` and the smaller surfaces around it.

### Agent work

- Use `ToolChip` for compact transcript status and `ToolCall` when input,
  output, errors, approval, or grouping must be inspectable.
- Use `Thinking` for a structured reasoning trace the product intentionally
  exposes. Do not fabricate reasoning content to make the UI look active.
- Use `ImageGeneration` for the generation lifecycle and preview together.

### Tables

- Use `RecordsTable` for a general virtualized record grid.
- Use `DiffTable` when rows express before/after proposed edits.
- Use `FilterTable` when filter state and stable-row reordering are the main
  interaction.
- Use `ComparisonTable` for a bounded feature-by-option comparison matrix.
- Use gpui-component's generic table when none of those agent-specific
  semantics are present.

## Useful compositions

Keep each component's job visible in application state rather than building a
new mega-component.

- **Conversational agent:** `Chat` + `PromptBar`, with `StreamingText`,
  `Thinking`, and `ToolCall` snapshots in messages; optionally `MessageQueue`.
- **Clarify, plan, act:** `QuestionFlow` → `PlanCard` → `ApprovalCard`.
- **Research and evidence:** `SearchResults` → `ContextCard`/`InsightCard` →
  `ArtifactPanel` or an appropriate table.
- **Workspace:** gpui-component `DockArea`/resizable panels containing
  `SidebarNav`, `ThreadList`, `Chat`, and `ArtifactPanel`.

Arrows describe application state transitions, not direct component ownership.
The application decides when one snapshot becomes the next.
