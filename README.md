# gpui-ai

AI-native UI components for [GPUI](https://gpui.rs), the Rust UI framework from the makers of Zed.

Streamed answers, thinking traces, tool calls, approval gates, chat with @-mentions, live task status — the interface patterns every AI application rebuilds. Web developers get them from [Beautiful UI](https://www.beautifului.dev) and [AIcss](https://www.aicss.dev); **gpui-ai** brings them to Rust, built on top of [gpui-component](https://github.com/longbridge/gpui-component) the way Beautiful UI builds on shadcn/ui.

**Status:** early development, pre-1.0. The API moves; pin a revision.

## Live demo

Browse every component with working demos at **https://labcoder.github.io/gpui-ai/** (coming soon).

To run the gallery locally instead:

```sh
git clone https://github.com/labcoder/gpui-ai
cd gpui-ai
cargo run -p gallery        # or: npm run dev
```

A native window opens showing all components with live simulated agent activity and a theme switcher (Light · Dark · Contrast · Midnight Violet · Nord Frost · Ember Dusk · Paper Light). Use `npm run prod` for the optimized build.

## Installation

`gpui` itself is under heavy development upstream and its crates.io release is far behind, so gpui-ai currently ships from GitHub only:

```toml
[dependencies]
gpui-ai = { git = "https://github.com/labcoder/gpui-ai" }
```

Your app will also need GPUI itself, declared exactly the same way so Cargo unifies them into one copy:

```toml
[dependencies]
gpui = { git = "https://github.com/zed-industries/zed" }
```

> **Why no version numbers?** crates.io publishing requires every dependency to have a crates.io version, and the published `gpui` release predates everything gpui-ai builds on. Once upstream starts releasing regularly, gpui-ai will publish to crates.io too.

Requirements: Rust stable (edition 2024 — 1.85+). On Linux, run `script/install-linux.sh` for system dependencies first.

## Usage

Every window must call `gpui_component::init(cx)` before building UI, and its first-level view must be wrapped in gpui-component's `Root`:

```rust
use gpui_ai::prelude::*;
use gpui_component::Root;

fn main() {
    gpui_platform::application().run(move |cx| {
        gpui_component::init(cx);
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

The full API is documented on each type. Run `cargo doc --open` from a checkout to browse it while the Git-sourced dependency graph keeps the crate unpublished.

## Components

| Component | Kind | What it does |
|---|---|---|
| `LoadingState` | stateless | Pixel-grid loader with shimmer and elapsed time |
| `Orbs` | stateless | Ambient dot-lattice thinking indicator, five choreographies |
| `Thinking` | stateless | Reasoning disclosure that opens while streaming, shimmers, pins a live preview, then collapses to "Thought for Ns" |
| `StreamingText` | stateless | Streaming markdown answer with hover-previewed citations, favicon-style source chips, follow-ups |
| `CodeBlock` | stateless | Syntax-highlighted code with header, copy button, stream reveal |
| `CodeDiff` | stateless | Unified patch viewer (`DiffFile::from_unified`) with rem-aligned gutters, change tints, per-hunk accept/reject by path and index, and a copyable source |
| `ToolChip` | stateless | Compact tool-call / file-edit chips with status |
| `ToolCall` / `ToolGroup` | stateless | Collapsible tool-call cards — input, output, failure, Allow/Deny — and a shimmering group that folds a burst of calls |
| `TaskRow` / `TaskSnapshot` | stateless | Live agent task status rows |
| `TodoList` | entity | Agent to-do list with progress |
| `ImageGeneration` | stateless | Image-generation frame with progress |
| `SearchResults` | stateless | Web-search result cards |
| `ApprovalCard` | stateless | Human-in-the-loop gate with default and destructive tones, optional "Always allow", and resolved approved/rejected states |
| `PlanCard` | stateless | An agent's proposed steps with typed per-step status, lifecycle badge, Approve/Reject/Edit while proposed, and step activation by stable ID |
| `RecommendationCard` | entity | Agent suggestion with confidence meter |
| `ContextCard` | stateless | Retrieved knowledge chunks with sources |
| `InsightCard` | stateless | Paged insight cards with sparkline charts |
| `PromptBar` | entity | Composer: @ mentions, / commands, provider-grouped model picker with descriptions and context windows, attachments |
| `Chat` | entity | Virtualized transcript + composer with hover-revealed message actions (copy, regenerate, feedback), in-place edit, branch version switcher, message attachments, welcome state, unread & jump-to-latest |
| `Suggestions` | stateless | Starter and follow-up prompt chips with staggered reveal and stable IDs |
| `AttachmentStrip` / `AttachmentPreview` | stateless | Composer and message attachments with thumbnails, kind glyphs, typed upload state, and open/remove events by stable ID |
| `ContextMeter` | stateless | Context-window usage ring / bar / text with severity tones and a hover breakdown |
| `CommandSearch` | entity | Command palette with filtering and keyboard navigation |
| `SidebarNav` | entity | Collapsible, filterable workspace navigation |
| `ThreadList` | entity | Grouped conversation list: new, switch, search, archived toggle, rename / archive / delete row actions |
| `FineTuneCard` | entity | Design-property inspector (size, radius, opacity, typeface) |
| `SelectionActions` | entity | Selection-anchored Ask / Explain / Rewrite actions |
| `RecordsTable` | entity | CRM-style virtualized grid with sorting |
| `DiffTable` | entity | AI-proposed edits over tabular data |
| `FilterTable` | entity | Status-chip filtered, reorderable table |
| `ComparisonTable` | entity | Feature-by-plan comparison matrix |

Keyboard action dispatch in the browser WASM gallery is limited by the pinned upstream GPUI revision: pointer activation works, while action-based keyboard paths such as Command Search navigation remain native-only until that upstream seam is fixed.

All components style themselves through gpui-component's semantic theme tokens, so light/dark, bundled themes, custom JSON themes, and live token editing work without per-component overrides.

Shared building blocks are public too: `gpui_ai::motion` (reduced-motion-aware text shimmer, one-shot reveals, breathing), `gpui_ai::status` (the one tone scale and status pill every lifecycle uses), and `gpui_ai::cues` (typed interaction cues — message arrived, response settled, copied, submitted, decided — that an application can observe in one place to play sounds or haptics; gpui-ai never plays audio itself).

## Theming

Components inherit whatever gpui-component theme is active — there is no gpui-ai-specific styling layer. To ship your own look, define a theme in gpui-component's JSON format:

```rust
use gpui_component::theme::ThemeRegistry;

ThemeRegistry::global_mut(cx)
    .load_themes_from_str(MY_THEME_JSON)?;
```

The gallery ships seven presets you can use as starting points — including four original showcase themes ([Midnight Violet](crates/gallery/themes/showcase-themes.json), Nord Frost, Ember Dusk, Paper Light). Their JSON lives in [`crates/gallery/themes/`](crates/gallery/themes/showcase-themes.json) and is downloadable from the website.

## Contributing

Read [AGENTS.md](AGENTS.md) for architecture rules, verification commands, and the definition of done, then open a PR. `npm run check` must pass before review.

## License

MIT — see [LICENSE](LICENSE).
