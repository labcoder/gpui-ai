# Changelog

All notable changes to this project are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
this project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
The crate is pre-1.0: the public API can change in any release, so pin a
revision.

## [Unreleased]

## [0.1.0] - unreleased

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
