# Contributing

Thanks for looking. This file is the front door: how to set up, what the gates
are, and what gets a change merged. [AGENTS.md](AGENTS.md) is the full rulebook
for architecture, dependencies, and the definition of done — read it before your
first change, and treat it as authoritative wherever the two disagree.

## Setup

```sh
git clone https://github.com/labcoder/gpui-ai
cd gpui-ai
script/install-linux.sh   # Linux only, for system dependencies
npm run dev               # opens the native gallery
```

Rust 1.89 or newer, edition 2024. `rust-toolchain.toml` pins the stable channel. The
root `package.json` is a task runner with no JavaScript dependencies — the
scripts shell out to cargo. Add new workflows there so they stay discoverable.

Node 22 or newer, and CI runs 24. The site's browser test drives Chrome through
the global `WebSocket`, which Node did not provide before 22, so an older
runtime fails that test with a `ReferenceError` while everything else passes.

For the browser gallery you also need a nightly toolchain with the
`wasm32-unknown-unknown` target and `wasm-bindgen-cli` at the version recorded
in `.github/workflows/ci.yml`.

Install both web workspaces before running their checks:

```sh
npm ci --prefix crates/gallery-web/www
npm ci --prefix site
npm run setup:web-browser
```

The browser installer downloads the Chrome for Testing version pinned in
`script/web-test-config.json` into ignored `target/web-browser/`. Release tests
verify the running version; they never skip because a browser is missing.
On Windows the installer grants Chrome's sandbox read/execute access only to
that downloaded browser directory. It does not disable the sandbox.

Linux additionally needs `xvfb`, `xauth`, `libvulkan1`, and
`mesa-vulkan-drivers` (install with your distribution's package manager).
The Linux gate uses SwiftShader WebGPU inside a private virtual display:
headless Chrome can acknowledge WebGPU work while capturing black frames.
Windows/macOS use their normal graphics adapter. All profiles must produce
nonblank canvas pixels; a silent WebGL fallback fails the WebGPU checks.
On Ubuntu 23.10+ with restricted user namespaces, follow Chromium's
[sandbox setup guidance](https://chromium.googlesource.com/chromium/src/+/main/docs/security/apparmor-userns-restrictions.md).
CI installs the helper shipped with the pinned Chrome archive into a
root-owned versioned directory under `/usr/local/lib/gpui-ai-chrome/`, verifies
its ownership and mode, and starts a real renderer before compiling WASM.
It does not assume `/opt/google/chrome/chrome-sandbox` exists, disable the
browser sandbox, or change global AppArmor settings. On a Linux development
machine with sudo access, `npm run setup:web-browser -- --linux-sandbox` runs
the same setup and probe. CI exports the resulting helper path automatically;
locally, set `CHROME_DEVEL_SANDBOX` to the path printed by setup for subsequent
browser checks.

`build:wasm` uses the locked dependency graph and **never invokes `wasm-opt`**,
even if it is on `PATH`. Binaryen 108 and 132 produced non-instantiable gallery
modules. Local and CI builds use the same bindgen pipeline; re-enabling an
optimizer requires an explicit, pinned, browser-verified change.

## The rules that decide whether a change lands

Four of them account for most review comments:

1. **Compose upstream; never fork it.** Build on gpui-component's styled
   components. When their styling is too opinionated, drop to `gpui-base`
   behaviors and own the presentation. Copying upstream source into this
   repository is not an option.
2. **All presentation comes from theme tokens.** Every color, radius, spacing
   value, shadow, and type style resolves through `cx.theme()`. Zero hardcoded
   colors in `crates/gpui-ai`. This is what makes light/dark, bundled themes,
   custom JSON themes, and live token editing work for free.
3. **No raw `px()` in library layout.** Use semantic spacing tokens or the
   rem-based helpers (`p_2`, `gap_3`, `text_sm`), so window zoom works. Raw
   pixels are reserved for physical boundaries, and the `pixel_discipline` test
   enforces the documented allowlist.
4. **Every component change carries its evidence.** A story in
   `crates/gallery`, an entry in the site catalog, a row in the README table,
   and tests — AccessKit semantics, keyboard operation, and constrained
   overflow — in the same change that adds the component.

Also expected: typed event enums and stable application IDs rather than
collection indices, `#![deny(missing_docs)]` on the library crate, and no
`unwrap` in library code (`expect` with a real message is fine in the gallery
binary). The complete definition of done is in
[AGENTS.md](AGENTS.md#definition-of-done-for-a-component).

## Gates

Run these before opening a pull request:

```sh
npm run check:prepush # native quality gates + web tests + fresh release WASM/browser gate
```

`check:prepush` runs `check` followed by `check:web`. Compilation alone is not
browser evidence. CI uses the same checks, naming compile and browser steps
separately so failures identify the layer. Regenerate catalog/theme data with
`npm run generate` when changing those inputs and review the generated diff;
CI checks freshness.

### Choosing the cheapest useful check

Tests should protect behavior, not repeat the implementation. Prefer real input
and typed events for interaction, AccessKit nodes for semantics, compiled examples
for API usage, and built artifacts for links or generated content. A harmless
private-module move should not break a test; a wrong stable ID or missing label
should. When replacing a test, identify its surviving owner and check that a
plausible broken implementation actually fails.

```sh
npm run test:models          # library model/unit tests; no browser build
npm run test:gallery:layout  # measured gallery layout, including transient heights
npm run test:web:host        # host protocols and browser-independent behavior
npm run test:site:artifacts  # one fresh immutable SSR artifact, plus isolated rollback faults
npm run check:generated     # compare every generated byte without repairing the checkout
```

These focused commands do not replace `check:prepush`. `check` also compiles
the catalog's displayed examples and the README's component-kind contracts,
executes doctests, and verifies links against freshly built rustdoc. Its Rust
tests enable `gallery/performance` to include deterministic measurement-plan
tests; they do not run hardware performance budgets. `check:generated` writes
only to a temporary directory. Run `npm run generate` explicitly to update stale
outputs, then review their diff.

CI's native OS matrix checks compilation. The Quality job executes deterministic
Rust tests on Linux; it is not evidence of native glyph rendering on every OS.
The platform typography and hardware performance checks below remain separate.

### Publication pipeline

On a main push, Quality, the native matrix, and WASM/browser checks must all
pass before CI assembles the publishable site. That assembly downloads the
already-tested gallery: it does not compile WASM, rebuild its host, or rerun
the browser suite. It generates the complete posters, social cards, API docs,
and static site once, then uploads `pages-site` with source/run provenance and
per-file hashes. Tag CI also retains a complete artifact without deploying it.

Pages runs automatically only after successful main-push CI. It downloads
`pages-site` from that exact run, verifies its identity and bytes, packages
those files for Pages, and deploys without rebuilding. Failed/cancelled CI,
forks, PRs, and superseded commits cannot publish. Publication is checked
against current main again after any deployment-environment approval wait.
Packaging retries use distinct artifact names; retrying deployment alone
uses the artifact name saved by its successful preparation job.

For recovery, use **Actions → Pages → Run workflow**, selecting **main**.
This is an explicit standalone path: it uses the shared site builder but first
compiles and tests a fresh gallery, without requiring a successful CI run.
It still verifies generated data and the complete publication before deploying.
Automatic runs never silently fall back to rebuilding a missing/expired artifact;
rerun CI or choose the manual path.

`npm run check:pipeline` exercises parsed workflow conditions, artifact reuse,
manual/failure paths, and publication provenance/hash checks. These tests also
run under `check:site`. They use test-only YAML and GitHub expression parsers;
the shipped site has no new runtime dependency. Run `actionlint` on all workflow
files too. A local test cannot exercise GitHub's hosted event delivery or OIDC
deployment; the first pushed run remains the hosted integration check.

For a shorter web-only run or a focused reproduction:

```sh
npm run check:web                            # host/site tests, rebuild, all release suites
npm run test:web:browser -- --suite mobile --repeat 3
```

`test:web:browser` deliberately reuses the existing release artifact. Use it
only after building the current code; it is not proof that edited Rust/host
code was tested. Available suites are `artifacts`, `catalog`, `lifecycle`, and `mobile`.
New `site/test/release/*.test.mjs` files join the full gate automatically.
`--repeat` requires every run to pass; there are no automatic retries.

The built scheme matrix runs in `artifacts` after the build and fails if the
artifact is missing. Lifecycle workflows share one completed site build but
each owns a new browser/profile. The catalog also retains fresh processes so
one story cannot hide another's cold-start or GPU-lifecycle failure.

The gate writes a manifest (commit, dirty state, browser/profile, artifact
hashes), JUnit results, screenshots, browser errors, and CDP command timings
under `target/web-evidence/`. CI and standalone Pages builds upload their own
run's evidence even when tests fail; it is not restored from the Cargo cache.
To compare a system browser, use `--system-browser` and optionally `CHROME_PATH`.
That diagnostic run is not the pinned release profile. `GPUI_AI_WEB_GPU=default`
selects the platform's normal adapter; Linux defaults to `software` in the
release runner.

Mobile coverage uses real touch events at 2x/3x density, verifies the backing
store, checks an actual approval decision, and exercises edit/blur/re-entry.
Emulating Safari's missing device-pixel resize API is **not** testing Safari.
Before releases affecting mobile input, also check a real iPhone/Safari and
Android/Chrome: keyboard visibility, tap targets, scrolling, and crisp text.
The software CI profile is a correctness check, not a 120 fps benchmark.
Theme contrast is enforced for gpui-ai's own presets. Upstream presets are
shown as published and their contrast findings are reported, not enforced.
For workflow edits, also run `actionlint` against the changed YAML files.
In WSL, prefer a checkout on the Linux filesystem: cross-user metadata and
copy operations on a Windows-mounted checkout can fail independently of the
tests. Run Chrome as a regular user, not root.

`npm run check` must pass before review. Visual changes need a look in the real
window across at least three themes including light and dark — reviewing motion
by eye in a throttled preview pane does not count; judge it natively or through
tests.

Two hardware-dependent gates stay outside `npm run check` and are not required
for every change, but any performance claim must come from them rather than
from a debug build:

```sh
npm run test:perf      # optimized frame-budget gate
npm run test:catalog   # whole-catalog scroll, stalls, latency, memory, idle demand
```

For label/font changes, `npm run test:typography` checks actual glyph pixels
with Windows/DirectWrite and a GPU across all bundled themes and rem scales.
It stays separate from the mock-renderer tests in `npm run check`.

## Commits and pull requests

- One logical change per commit.
- Semantic, imperative subject lines: `feat: add grouped conversation thread
  list`, `fix(attachment): keep ready file metadata muted`, `docs: …`,
  `chore: …`, `refactor: …`. Single-line messages are the house style; add a
  body only when the reason is not obvious from the diff.
- No trailers. Commits carry no co-author, generated-by, or tool-attribution
  lines.
- Say what you verified in the pull request description: which gates you ran,
  which themes you looked at, and what you could not check on your platform.
  Separate automated evidence from manual evidence.

## Dependency changes

The upstream stack is on crates.io — GPUI Kit publishes GPUI itself as
`gpui-pre` — so a bump is an ordinary version bump:

```sh
cargo update -p gpui-component --precise <version>   # and its siblings
cargo check --workspace
npm run vendor:themes                                # after a component bump
```

`gpui-pre`, `gpui-pre-platform`, `gpui-component`, `gpui-kit-assets`, and
`gpui-base` are one compatible set: move them together, in one commit, with
`Cargo.toml` and `Cargo.lock` alongside each other. The theme pack is not in
the published crate, so `vendor:themes` fetches it from the tag matching the
locked version — a palette change should show up in the diff.

`gpui-ai` is published, so everything it depends on must come from a registry;
a git or path dependency in the library crate makes it unpublishable. Prefer
what gpui, gpui-component, and the standard library already provide over a new
dependency.

## Releases

[CHANGELOG.md](CHANGELOG.md) follows Keep a Changelog: land user-visible changes
under `## [Unreleased]` as you go. At release time, rename that heading to the
version and give it a date — `node script/release-notes.mjs <version>` warns
while a section is still marked `unreleased`.

```sh
node script/release-notes.mjs <version>                  # the changelog section
node script/release-notes.mjs <version> --release-body   # the full release body
```

The `--release-body` form fills `.github/release-template.md`, including the
`gpui-component` and `gpui-pre` versions read from `Cargo.lock`, so each release
states exactly which upstream it supports.

Then publish the library:

```sh
cargo publish -p gpui-ai --dry-run   # packages and compiles from registry sources alone
cargo publish -p gpui-ai
```

A published version is permanent — it can be yanked but never replaced — so the
dry run is not optional. `cargo publish` needs a crates.io token with the
`publish-update` scope (`publish-new` for a crate's first release), stored once
with `cargo login`. Only `gpui-ai` is published; the gallery crates are
`publish = false` because they are demonstrations of it, not part of it.

## Reporting problems

Bugs and feature ideas go in GitHub issues. Vulnerabilities do not — see
[SECURITY.md](SECURITY.md).

## License

By contributing you agree that your contribution is licensed under the
project's [MIT license](LICENSE).
