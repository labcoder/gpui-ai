# State, events, and progress

gpui-ai components are controlled views. The application owns truth and work;
components own only transient presentation behavior that cannot outlive them,
such as hover, focus, scroll position, or a short copied confirmation.

## Stateless and entity-backed components

A stateless component implements `RenderOnce`. Build it from the current
snapshot while rendering:

```rust
ToolChip::new(tool.id.clone(), tool.label.clone())
    .status(tool.status)
    .on_event(cx.listener(|this, event: &ToolChipEvent, _, cx| {
        this.handle_tool_intent(event, cx);
    }))
```

An entity-backed component accepts `window` and a typed context during
construction. Create it once, retain its `Entity<T>` and subscriptions, and
send changed snapshots through its public `set_*` methods:

```rust
let prompt = cx.new(|cx| PromptBar::new("agent-prompt", window, cx));
let subscription = cx.subscribe(
    &prompt,
    |this, _, event: &PromptBarEvent, cx| this.handle_prompt(event, cx),
);
```

Store the subscription for as long as the event should be observed. Do not
recreate an entity during every render merely to make it reflect new data.
Inspect the pinned source for its setters and event type.

## Progressive work

Use one shared lifecycle:

- `ProgressState::Pending`
- `ProgressState::Running`
- `ProgressState::Complete`
- `ProgressState::Failed(reason)`

`Progressive<T>` pairs content with that state and a monotonic revision.
`StreamedContent` is `Progressive<String>` with text-oriented helpers.

The application advances it when real work changes:

```rust
let mut answer = StreamedContent::new();
answer.append("First tokens");
answer.append(" and more");
answer.finish();
```

Use the exact helpers available at the locked revision. The important contract
is that revisions change for meaningful content or lifecycle transitions, not
for every redraw. Components use the revision to avoid measuring or rebuilding
unchanged progressive content.

Do not add a timer that guesses when work finishes. A model request, tool, or
job owner supplies actual transitions. Repeating asynchronous work belongs in
the application entity that owns its lifecycle and should capture that entity
weakly across waits.

## Typed intent

Events report what the person asked for: approved, rejected, submitted,
selected, opened, removed, reordered, or similar intent. Handle the real enum
exhaustively enough that a future variant is noticed.

The normal loop is:

```text
component event
  → application validates and performs the transition
  → application updates its domain snapshot
  → stateless view is rebuilt or entity receives set_*
  → narrowest owning entity is notified
```

Avoid treating a callback as an imperative command into durable component
state. For example, an `ApprovalEvent` does not itself authorize external work;
the application records the decision, starts the authorized work if
appropriate, and renders the resulting decision state.

## Stable identity

IDs come from the domain, not display positions:

- message UUID, not message index;
- tool invocation ID, not the fifth visible card;
- row key, not its current sorted position;
- attachment ID, not its thumbnail slot;
- question and option IDs, not page and option indices.

Stable IDs let virtualization, filtering, branching, reordering, focus, and
animation reconcile the same object across snapshots. If the domain has no
stable ID, introduce one at ingestion rather than deriving it from layout.

## Cues

Use the cue system for optional sound or haptic reinforcement of meaningful
moments. A cue never carries state: success, failure, approval, and attention
must remain visible and semantic when sound and haptics are unavailable.
