# gpui-ai

[![CI](https://github.com/labcoder/gpui-ai/actions/workflows/ci.yml/badge.svg)](https://github.com/labcoder/gpui-ai/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue)](LICENSE)
[![Components](https://img.shields.io/badge/components-34-blue)](https://labcoder.github.io/gpui-ai/components/)
[![Themes](https://img.shields.io/badge/themes-45-blue)](https://labcoder.github.io/gpui-ai/themes/)

AI-native UI components for [GPUI](https://gpui.rs), the Rust UI framework from the makers of [Zed](https://zed.dev).

Streamed answers, thinking traces, tool calls, approval gates, chat with @-mentions, live task status, agent plans, context meters — the interface an AI application needs and every AI application rebuilds. **gpui-ai** is 34 of them, designed for GPUI and composed on top of [gpui-component](https://github.com/longbridge/gpui-component), so they inherit its semantic theming and sit beside its controls rather than replacing them.

Nothing here fetches, retries, or persists anything. A component renders the state you give it and reports what was done to it as a typed event keyed by your own IDs, which is what makes it possible to drop one into an application that already owns its data.

## Live demo

**[labcoder.github.io/gpui-ai](https://labcoder.github.io/gpui-ai/)** — every component, running.

Each demo on that site is the real component compiled to WebAssembly, not a screenshot or a video: the same gallery binary that runs natively, drawn on a WebGPU canvas, in any of the 45 themes. Each page carries the snippet it runs, cut from the gallery's own source, so the code on the page cannot drift from the thing above it.

To run the same gallery natively:

```sh
git clone https://github.com/labcoder/gpui-ai
cd gpui-ai
cargo run -p gallery
```

A window opens with every component, live simulated agent activity, and a theme switcher. `npm run prod` builds it optimized; `npm run dev:fast` iterates quickly with dependencies optimized and workspace crates not.

## Requirements

| | |
|---|---|
| **Rust** | 1.89 or newer. Edition 2024 needs 1.85; the pinned dependency graph raises the floor |
| **Platforms** | macOS, Linux, Windows — the same three GPUI supports. CI type-checks all three; the test suite runs on Linux |
| **Linux** | run [`script/install-linux.sh`](script/install-linux.sh) once for system dependencies |
| **Browser** (for the web gallery only) | WebGPU. There is no WebGL fallback; a browser without it is told so rather than made to download the binary |

## Installation

Not on crates.io. GPUI publishes there only rarely and its last release predates everything this is built on, and crates.io requires every dependency to have a crates.io version — so gpui-ai ships from Git until that changes.

```toml
[dependencies]
gpui-ai = { git = "https://github.com/labcoder/gpui-ai", tag = "v0.1.0" }
gpui = { git = "https://github.com/zed-industries/zed" }
gpui-component = { git = "https://github.com/longbridge/gpui-component" }
gpui_platform = { git = "https://github.com/zed-industries/zed", features = ["font-kit", "x11", "wayland", "runtime_shaders"] }
```

Four crates, because an application uses all four directly. `gpui` and `gpui-component` are what you write UI against; `gpui_platform` opens the window, and its features are what a Linux build needs. Declare each of them yourself, exactly as above, so Cargo unifies them with the copies gpui-ai uses.

- **Pin gpui-ai**, with `tag` or `rev`. The API moves between revisions; [CHANGELOG.md](CHANGELOG.md) records what each one changed.
- **Do not pin `gpui`.** gpui-component declares it without a `rev`, and two different git specs make Cargo build two incompatible copies.
- Each release names the `gpui-component` and `zed` revision pair it was built against. In a checkout that pair lives in `Cargo.toml` and `Cargo.lock`, and `npm run check:upstream` verifies the two still agree.

## Quick start

Every window calls `gpui_ai::init(cx)` before building UI — it runs `gpui_component`'s own init and registers the key bindings gpui-ai's components need — and its first-level view is wrapped in gpui-component's `Root`, which owns dialogs, notifications, and text selection:

```rust
use gpui_ai::prelude::*;
use gpui_component::Root;

fn main() {
    gpui_platform::application().run(move |cx| {
        gpui_ai::init(cx);
        cx.spawn(async move |cx| {
            cx.open_window(Default::default(), |window, cx| {
                let workspace = cx.new(|_| MyApp);
                cx.new(|cx| Root::new(workspace, window, cx))
            })
            .expect("failed to open window");
        })
        .detach();
    });
}
```

## Usage

### Stateless components

Presentational components are fluent builders that render wherever an element fits:

```rust
use gpui_ai::prelude::*;
use gpui_component::v_flex;

v_flex()
    .gap_4()
    .child(ToolChip::new("edit", "edit main.rs").status(ToolStatus::Running))
    .child(
        StreamingText::new("answer", &self.answer)
            .sources(["pricing.md", "suppliers.csv"]),
    )
```

### Stateful components

Composites like the prompt bar are retained entities. Your application owns durable data and async work; the component owns editors, focus, and overlays, reporting intent through typed events:

```rust
use gpui_ai::prelude::*;

let prompt = cx.new(|cx| PromptBar::new("agent-prompt", window, cx));
prompt.update(cx, |prompt, cx| {
    prompt.set_models([PromptModel::new("balanced", "Balanced")], cx);
    prompt.set_commands([
        PromptCommand::new("summarize", "summarize").description("Summarize current context"),
    ], cx);
});

// Retain this subscription wherever your app state lives.
let _subscription = cx.subscribe(&prompt, |_, _, event: &PromptBarEvent, _| {
    println!("prompt event: {event:?}");
});
```

`Chat` composes the prompt bar with a virtualized, selectable transcript. You own one immutable message snapshot plus every async producer; Chat owns transient scroll, tail-follow, anchor, and unread state. Message identity is stable across streaming replacements:

```rust
use std::sync::Arc;
use gpui_ai::prelude::*;

chat.update(cx, |chat, cx| {
    chat.set_messages(
        Arc::from([
            ChatMessage::new("q-42", ChatRole::User,
                StreamedContent::done("Which supplier is safest?")),
            ChatMessage::new("a-42", ChatRole::Assistant,
                StreamedContent::running("Comparing delivery risk…".to_owned())),
        ]),
        window, cx,
    );
});
```

Every type is documented: [the API reference](https://labcoder.github.io/gpui-ai/api/gpui_ai/) is published alongside the demos. `cargo doc --open` builds the same documentation from a checkout, with your dependencies' pages alongside it — the published tree is `--no-deps`, so its links into GPUI and gpui-component go to their own docs instead.

## Components

Thirty-four components across eight categories. Each one links to its live demo.

| Component | Kind | What it does |
|---|---|---|
| [`LoadingState`](https://labcoder.github.io/gpui-ai/components/loading/) | stateless | Pixel-grid loader with shimmer and elapsed time |
| [`Orbs`](https://labcoder.github.io/gpui-ai/components/orbs/) | stateless | Ambient dot-lattice thinking indicator, five choreographies |
| [`Thinking`](https://labcoder.github.io/gpui-ai/components/thinking/) | stateless | Reasoning disclosure that opens while streaming, shimmers, pins a live preview, then collapses to "Thought for Ns" |
| [`StreamingText`](https://labcoder.github.io/gpui-ai/components/streaming-text/) | stateless | Streaming markdown answer with hover-previewed citations, source chips, follow-ups |
| [`CodeBlock`](https://labcoder.github.io/gpui-ai/components/code-block/) | stateless | Syntax-highlighted code with header, copy button, stream reveal |
| [`CodeDiff`](https://labcoder.github.io/gpui-ai/components/code-diff/) | stateless | Unified patch viewer (`DiffFile::from_unified`) with rem-aligned gutters, change tints, per-hunk accept/reject by path and index, and a copyable source |
| [`ToolChip`](https://labcoder.github.io/gpui-ai/components/tool-chips/) | stateless | Compact tool-call / file-edit chips with status |
| [`ToolCall` / `ToolGroup`](https://labcoder.github.io/gpui-ai/components/tool-calls/) | stateless | Collapsible tool-call cards — input, output, failure, Allow/Deny — and a shimmering group that folds a burst of calls |
| [`TaskRow` / `TaskSnapshot`](https://labcoder.github.io/gpui-ai/components/tasks/) | stateless | Live agent task status rows |
| [`TodoList`](https://labcoder.github.io/gpui-ai/components/todos/) | stateless | Agent to-do list with progress |
| [`ImageGeneration`](https://labcoder.github.io/gpui-ai/components/image-generation/) | stateless | Image-generation frame with progress |
| [`SearchResults`](https://labcoder.github.io/gpui-ai/components/search/) | stateless | Web-search result cards |
| [`ApprovalCard`](https://labcoder.github.io/gpui-ai/components/approval/) | stateless | Human-in-the-loop gate with default and destructive tones, optional "Always allow", and resolved states |
| [`PlanCard`](https://labcoder.github.io/gpui-ai/components/plan/) | stateless | An agent's proposed steps with typed per-step status, lifecycle badge, Approve/Reject/Edit while proposed, and step activation by stable ID |
| [`RecommendationCard`](https://labcoder.github.io/gpui-ai/components/recommendation/) | stateless | Agent suggestion with confidence meter |
| [`ContextCard`](https://labcoder.github.io/gpui-ai/components/context/) | stateless | Retrieved knowledge chunks with sources |
| [`InsightCard`](https://labcoder.github.io/gpui-ai/components/insights/) | stateless | Paged insight cards with sparkline charts |
| [`PromptBar`](https://labcoder.github.io/gpui-ai/components/prompt-bar/) | entity | Composer: @ mentions, / commands, provider-grouped model picker with descriptions and context windows, attachments |
| [`VoiceControls`](https://labcoder.github.io/gpui-ai/components/voice/) | stateless | Dictate and speak controls for an application-owned voice state: live level meter, interim transcript status, typed `VoiceEvent`s |
| [`MessageQueue`](https://labcoder.github.io/gpui-ai/components/queue/) | stateless | Prompts waiting while the agent runs: named list with move, edit, send-now, remove, and clear by stable ID |
| [`Chat`](https://labcoder.github.io/gpui-ai/components/chat/) | entity | Virtualized transcript + composer with hover-revealed message actions, in-place edit, branch version switcher, message attachments, welcome state, unread & jump-to-latest |
| [`Suggestions`](https://labcoder.github.io/gpui-ai/components/suggestions/) | stateless | Starter and follow-up prompt chips with staggered reveal and stable IDs |
| [`AttachmentStrip` / `AttachmentPreview`](https://labcoder.github.io/gpui-ai/components/attachments/) | stateless | Composer and message attachments with thumbnails, kind glyphs, typed upload state, and open/remove events by stable ID |
| [`ArtifactPanel`](https://labcoder.github.io/gpui-ai/components/artifact/) | stateless | Side panel for generated documents and code: streamed source, kind-driven preview or source view, version switcher, typed actions, close |
| [`ContextMeter`](https://labcoder.github.io/gpui-ai/components/context-meter/) | stateless | Context-window usage ring / bar / text with severity tones and a hover breakdown |
| [`CommandSearch`](https://labcoder.github.io/gpui-ai/components/command-search/) | entity | Command palette with filtering and keyboard navigation |
| [`SidebarNav`](https://labcoder.github.io/gpui-ai/components/sidebar-nav/) | entity | Collapsible, filterable workspace navigation |
| [`ThreadList`](https://labcoder.github.io/gpui-ai/components/thread-list/) | entity | Grouped conversation list: new, switch, search, archived toggle, rename / archive / delete row actions |
| [`FineTuneCard`](https://labcoder.github.io/gpui-ai/components/fine-tune/) | entity | Design-property inspector (size, radius, opacity, typeface) |
| [`SelectionActions`](https://labcoder.github.io/gpui-ai/components/selection-actions/) | entity | Selection-anchored Ask / Explain / Rewrite actions |
| [`RecordsTable`](https://labcoder.github.io/gpui-ai/components/records-table/) | entity | CRM-style virtualized grid with sorting |
| [`DiffTable`](https://labcoder.github.io/gpui-ai/components/diff-table/) | entity | AI-proposed edits over tabular data |
| [`FilterTable`](https://labcoder.github.io/gpui-ai/components/filter-table/) | entity | Status-chip filtered, reorderable table |
| [`ComparisonTable`](https://labcoder.github.io/gpui-ai/components/comparison-table/) | entity | Feature-by-plan comparison matrix |

Shared building blocks are public too: `gpui_ai::motion` (reduced-motion-aware text shimmer, one-shot reveals, breathing), `gpui_ai::status` (the one tone scale and status pill every lifecycle uses), and `gpui_ai::cues` (typed interaction cues — message arrived, response settled, copied, submitted, decided — that an application can observe in one place to play sounds or haptics; gpui-ai never plays audio itself).

## Theming

Components inherit whatever gpui-component theme is active. There is no gpui-ai styling layer: every colour, radius, spacing value, shadow, and type style resolves through `cx.theme()`, which is what makes light/dark, bundled themes, custom JSON themes, and live token editing work with nothing to override per component.

A theme is a JSON file. To ship your own:

```rust
use gpui_component::theme::ThemeRegistry;

ThemeRegistry::global_mut(cx)
    .load_themes_from_str(MY_THEME_JSON)?;
```

Forty-five presets ship, in two groups. Nine are the gpui-ai set in [`themes/gpui-ai/`](themes/gpui-ai) — Light and Dark come from gpui-component, and [Contrast](themes/gpui-ai/contrast.json), [Midnight Violet](themes/gpui-ai/midnight-violet.json), [Nord Frost](themes/gpui-ai/nord-frost.json), [Ember Dusk](themes/gpui-ai/ember-dusk.json), [Paper Light](themes/gpui-ai/paper-light.json), [Graphite](themes/gpui-ai/graphite.json) and [Solstice](themes/gpui-ai/solstice.json) are original. The other 36 are gpui-component's own [vendored pack](themes/upstream), credited under Apache-2.0. Dropping another file into the directory adds it to the registry, the gallery, and the website, with no code to change.

Every one of them is downloadable from [the themes page](https://labcoder.github.io/gpui-ai/themes/) as the file the registry reads.

## What is verified, and where

**The native runtime is authoritative.** Every component is verified natively — light, dark and a third theme, AccessKit semantics for roles, names, values and actions, keyboard operation, and constrained-overflow behaviour — and the browser build is checked afterwards. A demo running in a tab is not evidence that the same path works for a screen reader or a keyboard-only user there; browser accessibility is a separate capability and is not claimed.

The website's demos demonstrate the components. They do not prove them.

One known limitation of the web gallery: pressing Tab, Shift+Tab, or Ctrl+C inside a live demo freezes that demo — the page around it keeps working, and reloading brings it back. The cause is upstream and is one import; [CHANGELOG.md](CHANGELOG.md) records the detail. Native builds are unaffected.

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md) for setup, the gates, and commit conventions, and [AGENTS.md](AGENTS.md) for architecture rules and the definition of done. `npm run check` must pass before review.

What changed in each version is in [CHANGELOG.md](CHANGELOG.md). Report vulnerabilities privately rather than in an issue — see [SECURITY.md](SECURITY.md).

## Built on

- **[GPUI](https://gpui.rs)** — the GPU-accelerated Rust UI framework from the Zed team, which does the drawing, layout, and platform work all of this stands on.
- **[gpui-component](https://github.com/longbridge/gpui-component)** by Longbridge — the component library and semantic theme system gpui-ai composes. Its controls, its theming, and its vendored theme pack are used as they are shipped, never forked or copied.

## License

MIT — see [LICENSE](LICENSE).
