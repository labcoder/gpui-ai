# gpui-ai

AI-native UI components for [GPUI](https://gpui.rs), the Rust UI framework from the makers of Zed.

> **Status: early development.** Nothing is published yet and the API is in flux. Twenty-four components have typed native and live-WASM gallery stories; the reproducible native, web, progressive-state, interaction, and theme foundations are operational.

## What is this?

Building an AI application means building the same interface patterns over and over: streamed answers, visible reasoning, tool calls, approval gates, live task status, chat composers with @-mentions and /-commands. Web developers get these from libraries like [Beautiful UI](https://www.beautifului.dev) and [AIcss](https://www.aicss.dev). Rust and GPUI developers currently build them by hand.

gpui-ai is that missing layer: a set of opinionated, composed components for AI applications, built on top of [gpui-component](https://github.com/longbridge/gpui-component) the way Beautiful UI builds on shadcn/ui. Every component styles itself exclusively through gpui-component's semantic theme tokens, so light/dark modes, custom themes, and live token editing flow through without component-specific overrides.

Available today: streaming text (markdown, typed inline citations, sources, follow-ups), thinking traces (step and prose variants), code blocks with streaming reveal, tool chips, task rows, agent to-do lists, web-search results, image-generation frames, the pixel-grid loading state, the ambient orbs indicator, approval, recommendation, context, paged insight cards with charts, a hybrid-controlled prompt bar with native text editing, virtualized chat, stable-ID command search, filterable sidebar navigation, a controlled fine-tune inspector, virtualized records, diff, and filter tables, a bounded feature comparison table, and selection-anchored Ask/Explain/Rewrite actions over selectable Markdown.

## Requirements

- Rust stable (edition 2024 — Rust 1.85 or newer)
- macOS, Linux, or Windows. On Linux, install system dependencies first:

```sh
script/install-linux.sh
```

- For the WebAssembly gallery: Rust nightly with the `wasm32-unknown-unknown` target and `wasm-bindgen-cli` 0.2.127

## Installation

Clone and build the workspace:

```sh
git clone <this-repo>
cd gpui-ai
cargo check --workspace
```

GPUI is consumed as a git dependency from the Zed repository — the crates.io release is far behind where the actual work happens, so this repo pins a known-good revision through `Cargo.lock`. If dependency resolution ever surprises you, read the dependency policy in [AGENTS.md](AGENTS.md) before changing any `Cargo.toml`.

## Usage

Run the component gallery in the normal debug profile while developing:

```sh
cargo run -p gallery   # or: npm run dev
```

Use the optimized profile for visual and performance review:

```sh
npm run prod
```

Run the hardware-dependent native frame-budget check separately from the portable test suite:

```sh
npm run test:perf
```

It retains 100 steady draws from each of six representative catalog regions (Loading, Streaming Text, Approval, Prompt Bar, Chat, and a repeatedly filtered and reordered 1,000-row Filter Table) in an optimized build. The enforced draw-time gate targets a 120 Hz CPU budget; reported presentation intervals depend on the connected display and are informational.

Expected output: a native window opens showing every component with live simulated agent activity — a virtualized controlled chat, a streaming markdown answer, a code block revealing line by line, tool chips and task rows in each status, and a light/dark/contrast theme control. Other scripts include `npm run build`, `npm run check`, `npm run build:wasm`, `npm run build:web`, and `npm run update:upstream` (see [AGENTS.md](AGENTS.md)).

In your own app, components are fluent builders that render wherever an element fits:

```rust
use gpui_component::v_flex;
use gpui_ai::prelude::*;

// inside a Render impl — `self.answer` is a StreamedContent you feed
v_flex()
    .gap_4()
    .child(ToolChip::new("edit", "edit main.rs").status(ToolStatus::Running))
    .child(
        StreamingText::new("answer", &self.answer)
            .sources(["pricing.md", "suppliers.csv"]),
    )
```

Stateful composites are retained GPUI entities. Applications own durable data and asynchronous work; the component owns editor, focus, and overlay state and reports user intent through typed events:

```rust
use gpui_ai::prelude::*;

let prompt = cx.new(|cx| PromptBar::new("agent-prompt", window, cx));
prompt.update(cx, |prompt, cx| {
    prompt.set_models([PromptModel::new("balanced", "Balanced")], cx);
    prompt.set_mentions([PromptMention::new("workspace", "Workspace")], cx);
    prompt.set_commands([
        PromptCommand::new("summarize", "summarize").description("Summarize current context"),
    ], cx);
});

// Store this subscription on the entity that owns the prompt.
let _subscription = cx.subscribe(&prompt, |_, _, event: &PromptBarEvent, _| {
    // Start application-owned work for Submit, CancelRequested, and the other typed events.
    println!("prompt event: {event:?}");
});
```

`Chat` composes that prompt with selectable `StreamingText` rows. The application owns one immutable `Arc<[ChatMessage]>` snapshot and every async producer; Chat owns only transient virtual-list, tail-follow, anchor, and unread state. Message identity is stable across prepends and streaming replacements:

```rust
use std::sync::Arc;
use gpui_ai::prelude::*;

let prompt = cx.new(|cx| PromptBar::new("conversation-prompt", window, cx));
let chat = cx.new(|cx| Chat::new("conversation", prompt, window, cx));
chat.update(cx, |chat, cx| {
    chat.set_messages(
        Arc::from([
            ChatMessage::new(
                "question-42",
                ChatRole::User,
                StreamedContent::done("Which supplier is safest?"),
            ),
            ChatMessage::new(
                "answer-42",
                ChatRole::Assistant,
                StreamedContent::running("Comparing delivery risk…".to_owned()),
            ),
        ]),
        window,
        cx,
    );
});

// Retain this subscription with the application state that owns the snapshot.
let _subscription = cx.subscribe(&chat, |_, _, event: &ChatEvent, _| {
    // Route prompts, retry, follow-up, citation, and jump intents by stable ID.
    println!("chat event: {event:?}");
});
```

Chat uses GPUI's variable-height `ListState` so only visible rows are laid out and wrapped or streaming rows can be remeasured at the real viewport width. The pinned `gpui-base` virtual-list API requires an exact height table for the full history up front, which would defeat virtualization for width-dependent prose.

`CommandSearch` adapts gpui-component's native command palette without replacing its editor, filtering, focus, keyboard navigation, or virtual list. Applications replace stable-ID item snapshots and receive only typed application identity—never collection indices:

```rust
use gpui_ai::prelude::*;

let search = cx.new(|cx| CommandSearch::new("workspace-commands", window, cx));
search.update(cx, |search, cx| {
    search.set_items(
        [
            CommandSearchItem::new("supplier-report", "Open report")
                .subtitle("Supplier pricing and margin summary")
                .keywords(["cost", "margin"])
                .shortcut("Ctrl+R"),
            CommandSearchItem::new("offline-sync", "Sync offline catalog")
                .disabled(true),
        ],
        window,
        cx,
    );
});

let _subscription = cx.subscribe(&search, |_, _, event: &CommandSearchEvent, _| {
    println!("command-search event: {event:?}");
});
```

Title, subtitle, and keyword matching is case-insensitive. On native targets, arrow keys skip disabled rows, Enter and pointer activation emit the same stable item ID, and Escape clears a non-empty query before emitting dismissal on the next press. In browser WASM at the pinned GPUI revision, pointer activation remains usable, but general keyboard action dispatch currently panics before `Command` handles the action; the same failure reproduces with an upstream action-based control, so the keyboard claims are native-only until that upstream seam is fixed.

`SidebarNav` composes gpui-component's retained Sidebar, group, and menu presentation with one native filter input. Applications replace stable-ID recursive sections and remain authoritative for the active item; the entity owns collapse, query, expansion, focus, and scrolling interaction state:

```rust
use gpui_component::IconName;
use gpui_ai::prelude::*;

let nav = cx.new(|cx| SidebarNav::new("workspace-nav", window, cx));
nav.update(cx, |nav, cx| {
    nav.set_sections([
        SidebarSection::new("production", "Production").items([
            SidebarNavItem::new("orders", "Orders")
                .icon(IconName::LayoutDashboard)
                .badge("12")
                .children([
                    SidebarNavItem::new("wholesale", "Wholesale"),
                    SidebarNavItem::new("farm-shop", "Farm shop"),
                ]),
            SidebarNavItem::new("forecast", "Seasonal forecast").disabled(true),
        ]),
    ], cx);
    nav.set_active_item("wholesale", cx);
});

let _subscription = cx.subscribe(&nav, |_, _, event: &SidebarNavEvent, _| {
    println!("sidebar event: {event:?}");
});
```

Filtering is case-insensitive and recursively retains matching descendants with their ancestor path; duplicate labels are valid because selection, expansion, active state, and accessibility identity all use domain IDs. An externally controlled active descendant forces only its rendered ancestor path open without changing the consumer snapshot, and a globally collapsed ancestor conveys that descendant selection. Activating a parent intentionally toggles expansion and emits `Selected` together, allowing parent routes to navigate as well as disclose children. Native pointer, Enter, Space, and AccessKit activation converge on one stable control per row, while disabled rows expose no activation action. Collapsed navigation renders one fully visible expand control, keeps filter focus on visible controls, and gives icon rows accessible names plus a single hover label. The general action-dispatch limitation described above also applies to action-based keyboard paths in the browser WASM build at the pinned GPUI revision; pointer interaction remains available there.

`FineTuneCard` is a controlled property inspector composed from retained upstream number-input, slider, color-picker, and popup-menu state. Width, height, radius, opacity, typeface, and optional accent remain application-owned; every intent carries the stable card ID and a typed value:

```rust
use gpui_ai::prelude::*;

let values = FineTuneValues::new(320., 180., 24., 0.84, "inter-regular");
let inspector = cx.new(|cx| {
    FineTuneCard::new(
        "design-properties",
        values,
        [
            FineTuneTypeface::new("inter-regular", "Inter"),
            FineTuneTypeface::new("inter-display", "Inter"),
        ],
        window,
        cx,
    )
});

// Retain this subscription and answer change events with a new controlled snapshot.
let _subscription = cx.subscribe(&inspector, |_, _, event: &FineTuneEvent, _| {
    println!("fine-tune event: {event:?}");
});
```

Duplicate visible typeface labels are safe because selection and events use stable typeface IDs. Numeric values are clamped, opacity is normalized to `0..=1`, invalid intermediate editor text does not emit, color always has a textual name/value, and the final Apply action remains reachable when the inspector is height-constrained.

`RecordsTable` adapts gpui-component's virtualized table while keeping records, selection, and sorting application-owned. Rows and columns use stable domain IDs, readable cells remain selectable, and progressive loading, empty, and failure states share `Progressive<T>`:

```rust
use std::sync::Arc;
use gpui_ai::prelude::*;

let records = cx.new(|cx| RecordsTable::new("suppliers", "Supplier records", window, cx));
records.update(cx, |records, cx| {
    records.set_columns([
        RecordColumn::new("company", "Company").sortable(true).fixed(true),
        RecordColumn::new("status", "Status").sortable(true),
    ], window, cx);
    records.set_records(
        Progressive::complete(Arc::from([
            RecordRow::new("alpenrose", "Alpenrose Dairy").cells([
                RecordCell::new("company", "Alpenrose Dairy"),
                RecordCell::status("status", "Ready", RecordStatusTone::Positive),
            ]),
        ])),
        window,
        cx,
    );
});

// Answer requests by replacing the controlled selected-row or sort snapshot;
// use `clear_selected_row` when the application clears its selection.
let _subscription = cx.subscribe(&records, |_, _, event: &RecordsTableEvent, _| {
    println!("records-table event: {event:?}");
});
```

Pointer double-click, Enter/Space, and each row's named Open control converge on stable row IDs. The Open control is the direct AccessKit activation path; disabled controls are named as unavailable, expose no Click action, and disabled rows are skipped by keyboard navigation. Both large row sets and wide column sets construct only their visible ranges, while snapshot replacement preserves the current stable row and column anchors. Applications can also move explicitly with `scroll_to_row` and `scroll_to_column`.

`DiffTable` composes the same virtualized focus and overflow behavior without adding diff flags to Records Table. Each cell is constructed as an added, removed, changed, or unchanged before/after value, and proposal decisions remain application-owned:

```rust
use std::sync::Arc;
use gpui_ai::prelude::*;

let proposals = cx.new(|cx| {
    DiffTable::new("menu-cleanup", "Proposed menu cleanup", window, cx)
});
proposals.update(cx, |table, cx| {
    table.set_columns([
        DiffColumn::new("flavor", "Flavor").fixed(true),
        DiffColumn::new("supplier", "Supplier").sortable(true),
    ], window, cx);
    table.set_rows(
        Progressive::complete(Arc::from([
            DiffRow::new("mint-chip", "Mint Chip", DiffChangeKind::Changed).cells([
                DiffCell::unchanged("flavor", "Mint Chip"),
                DiffCell::changed("supplier", "kumo-creamery", "maple-orbit"),
            ]),
        ])),
        window,
        cx,
    );
});

// Answer selection, sorting, review, accept, and reject requests with a new
// controlled snapshot.
let _subscription = cx.subscribe(&proposals, |_, _, event: &DiffTableEvent, _| {
    println!("diff-table event: {event:?}");
});
```

Every rendered value includes a readable change label and selectable prior/proposed content; color only reinforces that state. The selected proposal exposes named Accept and Reject controls with direct AccessKit actions, while Review, Enter, and Space converge on the same stable proposal ID. Loading, empty, failure, disabled, selected, wide, and growing snapshots use the shared progressive and virtualized contracts.

`FilterTable` adds stable, counted status controls above the same virtualized table behavior. The application owns the active filter and supplies the final filtered row order; retained visible rows move through a finite GPUI transition, while reduced-motion mode snaps immediately:

```rust
use std::sync::Arc;
use gpui_ai::prelude::*;

let tasks = cx.new(|cx| FilterTable::new("tasks", "Ice cream tasks", window, cx));
tasks.update(cx, |table, cx| {
    table.set_filters([
        FilterDefinition::new("all", "All", 5).active(true),
        FilterDefinition::new("todo", "To do", 2),
        FilterDefinition::new("progress", "In Progress", 2),
        FilterDefinition::new("completed", "Completed", 1),
    ], cx);
    table.set_columns([
        FilterColumn::new("task", "Task name").fixed(true),
        FilterColumn::new("status", "Status").sortable(true),
    ], window, cx);
    table.set_rows(Progressive::complete(Arc::from([
        FilterRow::new("menu", "Update menu").cells([
            FilterCell::new("task", "Update menu"),
            FilterCell::new("status", "To do"),
        ]),
    ])), cx);
});

// Answer filter, selection, activation, and sort requests by replacing the
// corresponding controlled snapshots.
let _subscription = cx.subscribe(&tasks, |_, _, event: &FilterTableEvent, _| {
    println!("filter-table event: {event:?}");
});
```

Filter controls expose active, inactive, count, and unavailable state semantically as well as visually. A constrained 1,000-row projection constructs only the visible range, retains stable row anchors, and keeps the final row reachable.

`ComparisonTable` uses an ordinary composed grid because its validated snapshot is intentionally bounded to 12 side-by-side items and 128 feature rows. It rejects duplicate IDs, dangling values, and larger shapes instead of truncating consumer data:

```rust
use gpui_ai::prelude::*;

let snapshot = ComparisonSnapshot::try_new(
    [
        ComparisonItem::new("starter", "Starter"),
        ComparisonItem::new("business", "Business")
            .state(ComparisonItemState::Highlighted),
    ],
    [ComparisonFeature::new("support", "Priority support").values([
        ComparisonValue::included("starter", false),
        ComparisonValue::included("business", true),
    ])],
)?;

let comparison = cx.new(|cx| {
    let mut table = ComparisonTable::new("plans", "Plan comparison", window, cx);
    table.set_snapshot(Progressive::complete(snapshot), window, cx);
    table
});
```

Item highlighting, disabled state, progressive state, and selection stay application-owned. Pointer, keyboard, and AccessKit activation emit `ComparisonTableEvent::SelectionRequested` with stable table and item IDs. Long labels wrap inside fixed-width columns, readable values remain selectable, and the bounded surface scrolls in both axes with focused columns revealed horizontally.

Selection actions retain gpui-component's native Markdown selection and copy behavior. Stable action IDs and the selected-text snapshot are emitted for application-owned work:

```rust
use gpui_ai::prelude::*;

let selection = cx.new(|cx| {
    SelectionActions::new("answer-selection", "Select any part of this answer.", window, cx)
});
selection.update(cx, |selection, cx| {
    selection.set_actions([
        SelectionAction::new("ask", "Ask"),
        SelectionAction::new("explain", "Explain"),
        SelectionAction::new("rewrite", "Rewrite"),
    ], cx);
});
```

Streaming answers resolve complete `[[cite:<stable-id>]]` markers against application-owned citation metadata. The inline Markdown stays selectable; activation emits the stable ID and opaque destination instead of opening it inside the library. At the current upstream pin, the inline Markdown glyph is pointer-only, so the adjacent named companion Link is the keyboard and AccessKit authority and emits the same typed event:

```rust
use gpui_ai::prelude::*;

let answer = StreamedContent::complete(
    "Pistachio margins improved [[cite:margin-report]].".to_owned(),
);

let cited = StreamingText::new("answer", &answer)
    .citations([CitationRef::new(
        "margin-report",
        "Margin report",
        "Open the margin report",
        "app://reports/margins",
    )])
    .on_event(cx.listener(|_, event: &StreamingTextEvent, _, _| {
        if let StreamingTextEvent::CitationActivated { id, destination } = event {
            println!("route citation {id} to {destination}");
        }
    }));
```

Using gpui-ai as a dependency is early (pre-0.1, API in flux). The wiring looks like this:

```toml
[dependencies]
gpui = { git = "https://github.com/zed-industries/zed" }
gpui_platform = { git = "https://github.com/zed-industries/zed", features = ["font-kit"] }
gpui-component = { git = "https://github.com/longbridge/gpui-component" }
gpui-ai = { git = "<this-repo>" }
```

## Support

Questions, bugs, and ideas: open a GitHub issue on this repository.

## Direction

The next focus is the remaining data-rich composites. The shared progressive-content API, semantic-token styling, accessible typed interactions, hybrid-controlled prompt composition, virtualized controlled chat, stable-ID command search, filterable sidebar navigation, controlled fine-tune inspection, selection actions, typed inline citations, reproducible native builds, one native/WASM story registry, and live multi-canvas web host are now established.

## Contributing

Issues and ideas are welcome now. The project is pre-0.1 and the component API is still settling, so if you want to contribute code, open an issue first so we can agree on direction before you invest time. Contributors should read [AGENTS.md](AGENTS.md) — it holds the working agreements (architecture boundaries, dependency policy, code standards) that keep the library coherent.

## Acknowledgments

- Design inspiration: [Beautiful UI](https://www.beautifului.dev) (MIT) and [AIcss](https://www.aicss.dev) — gpui-ai reimplements these interface patterns for GPUI as original code with its own token-driven visual design.
- Foundation: [gpui-component](https://github.com/longbridge/gpui-component) by Longbridge and [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) by Zed Industries.

## License

[MIT](LICENSE). Upstream dependencies gpui and gpui-component are licensed Apache-2.0.
