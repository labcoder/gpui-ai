# Changelog

All notable changes to this project are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
this project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
The crate is pre-1.0: the public API can change in any release, so pin a
revision.

## [Unreleased]

### Changed

- The website opens on the Nord Frost theme rather than following the machine's
  light or dark. Following the system is still offered, is now recorded like any
  other choice, and remains the fallback if the registry ever stops shipping the
  default. Every choice — the default included — is written to the URL, so the
  address always reproduces what the sender was looking at.
- Secondary text on the website is painted from a derived `--ai-muted-text`
  rather than from the registry's `muted.foreground`. Measured against their own
  backgrounds, 26 of the 45 themes put that text below 4.5:1 and nine below
  3:1. Themes that already cleared AA are unchanged.
- The home page shows how to install the library above the hero demo.
- The gallery stories no longer carry "Reference comparison" notes. They were
  published as part of the site and its code snippets, and read as though the
  components were ports of the libraries they named. The comparisons are
  maintainer notes and now live in `docs/internal/reference-comparisons.md`.

### Fixed

- The home page's dependency lines are syntax-highlighted like every other code
  block on the site, and are generated from the workspace manifests rather than
  written out in a component.
- A story now declares which way its content overflows instead of the exporter
  guessing from the story's measured height, which meant editing prose beneath a
  component could silently rewrite the published claim about it.

### Known issues

- Pressing Tab, Shift+Tab, or Ctrl+C inside a live demo on the website freezes
  that demo. The page around it keeps working and reloading brings the demo
  back. Native builds are unaffected.

  gpui-component enables gpui's `profiler` feature for everything that depends
  on it, and `gpui/src/profiler/actions.rs` reads the clock through
  `std::time::Instant` where the rest of that module uses the wasm-safe
  `scheduler::Instant`. `std::time` is unimplemented on
  `wasm32-unknown-unknown`, so dispatching any action panics with "time not
  implemented on this platform". The panic happens while the app's `RefCell` is
  mutably borrowed, and WebAssembly aborts rather than unwinding, so the borrow
  is never released and every later update fails with "RefCell already
  borrowed" — which is why the demo never recovers.

  The fix belongs upstream, in one import. Until it lands, live demos are
  pointer-driven only.

## [0.1.0] - 2026-08-23

### Added

#### Components

Thirty-four components. Each one ships with a gallery story exercising its real
states, typed events keyed by stable application IDs rather than collection
indices, and presentation resolved entirely through gpui-component's semantic
theme tokens.

- **Streaming and progress** — `LoadingState`, `Orbs`, `Thinking`,
  `StreamingText`, `CodeBlock`, `TaskRow` / `TaskSnapshot`, `TodoList`,
  `ImageGeneration`
- **Tools, plans, and approvals** — `ToolChip`, `ToolCall` / `ToolGroup`,
  `ApprovalCard`, `PlanCard`, `CodeDiff`
- **Conversation** — `Chat`, `PromptBar`, `Suggestions`, `AttachmentStrip` /
  `AttachmentPreview`, `MessageQueue`, `VoiceControls`, `ThreadList`,
  `ContextMeter`, `ArtifactPanel`
- **Knowledge and insight** — `SearchResults`, `ContextCard`, `InsightCard`,
  `RecommendationCard`
- **Tables** — `RecordsTable`, `DiffTable`, `FilterTable`, `ComparisonTable`
- **Navigation and utility** — `CommandSearch`, `SidebarNav`, `FineTuneCard`,
  `SelectionActions`

#### Foundations

- `stream` — `Progressive<T>` and `ProgressState`, the one lifecycle model every
  component that displays progressive work consumes. Applications own the state
  and the clock; components render snapshots.
- `motion` — reduced-motion-aware text shimmer, keyed one-shot reveals, and
  breathing, so a reduced-motion run still produces a useful static frame.
- `status` — the single tone scale and `StatusBadge` every lifecycle uses.
- `cues` — typed interaction cues (message arrived, response settled, copied,
  submitted, cancelled, decided) that an application observes in one place to
  play sounds or haptics. The library never plays audio itself.
- `scrolling` — wheel acceleration and middle-click autoscroll as pure
  behaviors: they own no rendering and no theme.
- `prelude` — one import for the whole component set.

#### Theming

- Every color, radius, spacing value, shadow, and type style resolves through
  `cx.theme()`. There is no gpui-ai styling layer and no hardcoded colors, so
  light/dark, bundled themes, custom JSON themes, and live token editing work
  without per-component overrides.
- Layout resolves through semantic spacing tokens and the rem scale rather than
  raw pixels, so window zoom works; a `pixel_discipline` test enforces it.
- Four original showcase themes ship with the gallery — Midnight Violet, Nord
  Frost, Ember Dusk, and Paper Light — alongside Light, Dark, and a
  high-contrast review theme.

#### Gallery

- One shared gallery binary serves both a native window and the browser
  (`wasm32-unknown-unknown`): a story per component plus a whole-catalog view,
  with simulated agent activity driven by deterministic ticks rather than a
  wall clock, so native and web hosts run identical story code.

### Known limitations

- **Not published to crates.io.** Publishing requires every dependency to carry
  a crates.io version, and the released `gpui` predates everything this library
  builds on. Install from git — see the README — and expect a registry release
  only once upstream publishes regularly.
- **Keyboard action dispatch is native-only in the browser gallery.** The
  pinned upstream GPUI revision does not route action-based keyboard paths
  under WASM, so Command Search navigation and similar paths work natively
  while pointer activation works in both. The native runtime stays
  authoritative.
- **Focused-input tests are ignored on macOS.** The pinned GPUI test window does
  not deliver focus the way the real window does; those tests run elsewhere
  rather than being deleted or weakened.
- **The streaming cursor does not animate inside Markdown.** Upstream's markdown
  view owns that text, and the library does not fork it.
- **One measured host misses the frame budget on the Filter Table transition.**
  The hardware performance gate (`npm run test:perf`) reports the same result on
  an unmodified checkout, so it is an environment result rather than a
  regression introduced here.
- **Browser demos require WebGPU.** Without it the gallery falls back quietly
  instead of rendering.

[Unreleased]: https://github.com/labcoder/gpui-ai/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/labcoder/gpui-ai/releases/tag/v0.1.0
