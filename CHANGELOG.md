# Changelog

All notable changes to this project are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
this project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
The crate is pre-1.0: the public API can change in any release, so pin a
revision.

## [Unreleased]

### Added

- Every story is rendered once at build time and kept as a still, so a reader
  who cannot run the live one still sees the component. It is the only picture
  a browser without WebGPU will ever get, and it now fills that window instead
  of a paragraph explaining the absence. An idle frame under Light or Dark
  shows one too — under the other 43 themes the window fills with the colour
  the canvas is about to paint, which is closer to the truth than a still in
  the wrong palette. The stills are captured by driving the real gallery
  through the same browser harness the release gate uses, so one cannot drift
  from what the component draws; they are rebuilt rather than checked in,
  because a GPU-rendered frame is not byte-reproducible.

- Nine component pages showed one line of code. Chat, Prompt bar, Command
  search, Sidebar nav and the four tables are entities, and the published
  region stopped at the constructor — everything that makes them worth using
  happened on the lines after it. The regions now cover the configuration, the
  columns a table is given, and, for Chat, the events an application answers.

- A link to a demo can name the state the story opens in, and Copy link gives
  back the one the reader was actually looking at. Five stories offer states to
  switch between and each draws its switcher inside its own canvas, so the page
  around it could not see which was showing: a shared link always opened where
  the story opens, which is not where the sender was.

- Demos follow the reader's motion preference. GPUI takes reduced motion from
  the platform and the web platform has none, so every demo on the site
  shimmered and breathed at someone who had asked their machine for stillness,
  while the same components honoured it on a desktop. Adding `motion=reduced`
  or `motion=full` to a demo's address pins it either way, so what the setting
  does to a component can be seen without changing a system setting to find
  out.

- Five documentation pages, and an index over them: Getting started, Theming,
  Ownership and events, Accessibility and motion, and Browser demo limits. The
  prose is written; the numbers in it are read from the generated data, and
  every code sample is a real file highlighted by the same step the component
  snippets go through, so a page cannot claim there are thirty components when
  there are thirty-four or show code that was never anything but a string.

- Reset on a demo restarts the story instead of replacing the frame. It was
  tearing down a seventeen-megabyte WebAssembly instance and building another
  one to reach a state the story gets back to in a frame; the theme override
  and the reader's place on the page survive it now.

- The catalog and the rail search a real index instead of matching substrings.
  Typing "approv" now finds the Approval card, which never contained the word;
  an event name finds the component that emits it; and a second word narrows
  the answer rather than widening it. Results are ranked by where the words
  landed — a type name means far more than a passing mention in a behaviour
  note — and while there is a query the category headings give way to one
  ordered list, so the best answer is at the top rather than wherever its
  category happened to fall. Pressing `/` puts the cursor in whichever search
  box is on screen, and opens the drawer when neither is.

- Every page carries a canonical link and a social card, so a link to this site
  expands into a title, a description and a picture of the component instead of
  a line of grey text. The cards are rendered from the site's own stylesheet
  around the still captured for each story, so they cannot drift from the pages
  they describe.
- A `sitemap.xml` listing every page, and a `robots.txt` that names it and keeps
  crawlers out of the demo embeds — one page per story, each a canvas with
  nothing to read.
- A 404 page in the site's own chrome, with three ways out. GitHub Pages served
  its own until now. The client also used to fall back to the home route for an
  address it did not recognise, which would have rebuilt every 404 as the front
  page the moment React took over.

- A demo more than a viewport away stops running, and at most three run at
  once. Every live demo is an instance of the shared gallery binary with its
  own WASM heap and WebGPU surface, and starting was previously a one-way door:
  a reader going down a long page collected one of each for every demo they
  passed. A demo that has been stopped restarts when it comes back into range,
  and one evicted by a nearer demo takes its seat back on its own.

### Changed

- The wheel scrolls the page over a demo. GPUI's web platform calls
  `preventDefault()` on every wheel event before looking at it, so a demo
  swallowed the wheel whether or not its story had anything to scroll: with the
  pointer over one, the page would not move in either direction, and a story
  already scrolled to its end simply ate the gesture. A demo now takes the
  wheel when the reader clicks into it, says so in its title bar, and gives it
  back when the pointer leaves. The same applies to a finger on a touch screen,
  where the canvas ships with `touch-action: none`.
- A demo now appears in its window instead of flashing black first. The embed
  paints no background of its own and its canvas is held hidden until GPUI has
  drawn into it — an unpresented WebGPU surface composites as solid black,
  because gpui_web configures it opaque — and the "Loading GPUI example" card
  in the middle of the window is replaced by the window's own title bar saying
  "Starting".
- Code samples read like an editor: the file each snippet was cut from on a
  strip above it, and line numbers in a gutter. The numbers are drawn with a
  CSS counter rather than written into the document, so they cannot reach the
  clipboard by any route.
- Nothing that ships claims kinship with another component library any more. A published code
  sample, the crate's own front page, and a constant in `orbs.rs` each credited
  one, which reads as a claim that this is a port of it. It is not: every
  component here is an original design for GPUI. The comparisons stay in
  `docs/internal/reference-comparisons.md`, which is not checked in. The
  vendored theme pack keeps its shadcn attribution: that is credit for data
  this project ships, and removing it would be the opposite of the point.
- The README is rewritten around what the library offers: a live demo, explicit
  requirements, installation with pinning guidance, and a component table where
  every entry links to its own running demo.
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

- A demo is no longer clipped when the column is narrower than the width its
  height was measured at. A story's height is a function of the width it is
  given and not a step function — prose rewraps a line at a time — so on a
  phone the reserved frame was hundreds of pixels too short and the story
  scrolled inside its own canvas. The story now reports what it laid out at and
  the frame grows to the tallest it reports. The measured numbers still reserve
  the space before anything runs, and are still what a reader without WebGPU
  keeps.
- Choosing a theme that sets no type size, corner radius, or shadow no longer
  leaves the last theme's. Three of the forty-five bundled themes declare those
  metrics and the other forty-two do not, and only a declared value was ever
  written — so picking Graphite, which asks for 14 px type and square corners,
  and then picking anything else left every demo at 14 px with square corners
  until the page was reloaded. No theme chosen after it could look like itself.
- The page no longer moves under the reader as the self-hosted faces arrive.
  They come in through an `@import` inside the stylesheet, and Vite does not
  preload what an `@import` pulled in, so the chrome painted in the system
  fallback and shifted about a second into a cold visit. The build now emits a
  preload for each face it wrote: measured cold and throttled, cumulative layout
  shift on the home page goes from 0.0029 to zero.
- The home page's dependency lines are syntax-highlighted like every other code
  block on the site, and are generated from the workspace manifests rather than
  written out in a component.
- A story now declares which way its content overflows instead of the exporter
  guessing from the story's measured height, which meant editing prose beneath a
  component could silently rewrite the published claim about it.
- The API documentation no longer carries 82,920 "Read more" links that did
  nothing. `cargo doc --no-deps` leaves rustdoc no page to point at for a trait
  implemented from a dependency, so it emitted anchors with no `href`; the
  publish step removes them and leaves the 1,533 that resolve.

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
