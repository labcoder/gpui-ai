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

Rust stable, edition 2024 (1.85+). `rust-toolchain.toml` pins the channel. The
root `package.json` is a task runner with no JavaScript dependencies — the
scripts shell out to cargo. Add new workflows there so they stay discoverable.

Node 22 or newer, and CI runs 24. The site's browser test drives Chrome through
the global `WebSocket`, which Node did not provide before 22, so an older
runtime fails that test with a `ReferenceError` while everything else passes.

For the browser gallery you also need a nightly toolchain with the
`wasm32-unknown-unknown` target and `wasm-bindgen-cli` at the version recorded
in `.github/workflows/ci.yml`.

[Binaryen](https://github.com/WebAssembly/binaryen) is optional locally and
installed in CI. When `wasm-opt` is on your `PATH`, `npm run build:wasm` runs it
over the release artifact and prints the saving; without it the build says so
and produces a working but larger binary. Install it if you are making a size
claim — `npm run report:wasm` numbers from a build that skipped `wasm-opt` are
not comparable to CI's.

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
npm run check        # fmt, clippy --deny warnings, script tests, Rust tests, rustdoc
npm run check:site   # site tests
npm run build:wasm   # anything the web build touches
```

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

GPUI comes from git, and the revision is pinned in `Cargo.lock`. To move it:

```sh
npm run update:upstream          # or: npm run update:upstream -- <full-rev>
```

That resolves gpui-component and its assets crate as one pair, reads the gpui
revision from *their* lockfile, and updates ours. Commit `Cargo.toml` and
`Cargo.lock` together, and never add a `rev` field to the `gpui` dependency —
differing git specs make Cargo build two incompatible copies. `npm run
check:upstream` verifies the manifest, the lockfile, and upstream agree.

Prefer what gpui, gpui-component, and the standard library already provide over
a new dependency.

## Releases

[CHANGELOG.md](CHANGELOG.md) follows Keep a Changelog: land user-visible changes
under `## [Unreleased]` as you go. At release time, rename that heading to the
version and give it a date — `node script/release-notes.mjs <version>` warns
while a section is still marked `unreleased`.

```sh
node script/release-notes.mjs 0.1.0                  # the changelog section
node script/release-notes.mjs 0.1.0 --release-body   # the full release body
```

The `--release-body` form fills `.github/release-template.md`, including the
`gpui-component` and `zed` revision pair read from the pinned graph, so each
release states exactly which upstream it supports.

## Reporting problems

Bugs and feature ideas go in GitHub issues. Vulnerabilities do not — see
[SECURITY.md](SECURITY.md).

## License

By contributing you agree that your contribution is licensed under the
project's [MIT license](LICENSE).
