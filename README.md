# mighty-gpui

AI-native UI components for [GPUI](https://gpui.rs), the Rust UI framework from the makers of Zed.

> **Status: early development.** Nothing is published yet and the API is in flux. Thirteen components have typed native and live-WASM gallery stories; the reproducible native, web, progressive-state, interaction, and theme foundations are operational.

## What is this?

Building an AI application means building the same interface patterns over and over: streamed answers, visible reasoning, tool calls, approval gates, live task status, chat composers with @-mentions and /-commands. Web developers get these from libraries like [Beautiful UI](https://www.beautifului.dev) and [AIcss](https://www.aicss.dev). Rust and GPUI developers currently build them by hand.

mighty-gpui is that missing layer: a set of opinionated, composed components for AI applications, built on top of [gpui-component](https://github.com/longbridge/gpui-component) the way Beautiful UI builds on shadcn/ui. Every component styles itself exclusively through gpui-component's semantic theme tokens, so light/dark modes, custom themes, and live token editing flow through without component-specific overrides.

Available today: streaming text (markdown, sources, follow-ups), thinking traces (step and prose variants), code blocks with streaming reveal, tool chips, task rows, agent to-do lists, web-search results, image-generation frames, the pixel-grid loading state, the ambient orbs indicator, and approval, recommendation, and context cards. Still to come: chat, prompt bar, command search, sidebar navigation, insight cards with charts, records/filter/diff/comparison tables, fine-tune inspector, inline citations, and selection actions.

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
cd mighty-gpui
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

Expected output: a native window opens showing every component with live simulated agent activity — a streaming markdown answer, a code block revealing line by line, tool chips and task rows in each status, and a light/dark/contrast theme control. Other scripts include `npm run build`, `npm run check`, `npm run build:wasm`, `npm run build:web`, and `npm run update:upstream` (see [AGENTS.md](AGENTS.md)).

In your own app, components are fluent builders that render wherever an element fits:

```rust
use gpui_component::v_flex;
use mighty_gpui::prelude::*;

// inside a Render impl — `self.answer` is a StreamedContent you feed
v_flex()
    .gap_4()
    .child(ToolChip::new("edit", "edit main.rs").status(ToolStatus::Running))
    .child(
        StreamingText::new("answer", &self.answer)
            .sources(["pricing.md", "suppliers.csv"]),
    )
```

Using mighty-gpui as a dependency is early (pre-0.1, API in flux). The wiring looks like this:

```toml
[dependencies]
gpui = { git = "https://github.com/zed-industries/zed" }
gpui_platform = { git = "https://github.com/zed-industries/zed", features = ["font-kit"] }
gpui-component = { git = "https://github.com/longbridge/gpui-component" }
mighty-gpui = { git = "<this-repo>" }
```

## Support

Questions, bugs, and ideas: open a GitHub issue on this repository.

## Direction

The next focus is the first stateful composite set: chat, prompt bar, command search, and navigation. The shared progressive-content API, semantic-token styling, accessible typed interactions, reproducible native builds, one native/WASM story registry, and live multi-canvas web host are now established.

## Contributing

Issues and ideas are welcome now. The project is pre-0.1 and the component API is still settling, so if you want to contribute code, open an issue first so we can agree on direction before you invest time. Contributors should read [AGENTS.md](AGENTS.md) — it holds the working agreements (architecture boundaries, dependency policy, code standards) that keep the library coherent.

## Acknowledgments

- Design inspiration: [Beautiful UI](https://www.beautifului.dev) (MIT) and [AIcss](https://www.aicss.dev) — mighty-gpui reimplements these interface patterns for GPUI as original code with its own token-driven visual design.
- Foundation: [gpui-component](https://github.com/longbridge/gpui-component) by Longbridge and [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) by Zed Industries.

## License

[MIT](LICENSE). Upstream dependencies gpui and gpui-component are licensed Apache-2.0.
