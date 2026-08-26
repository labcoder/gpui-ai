# gpui-ai <!-- version -->

AI-native UI components for [GPUI](https://gpui.rs) — streamed answers, thinking
traces, tool calls, approval gates, chat, and live task status — built on
[gpui-component](https://github.com/longbridge/gpui-component).

## Install

gpui-ai is not on crates.io. Publishing requires every dependency to carry a
crates.io version, and the released `gpui` predates everything this library
builds on. Take it from git, and declare `gpui` exactly the same way so Cargo
resolves one shared copy:

```toml
[dependencies]
gpui-ai = { git = "https://github.com/labcoder/gpui-ai", tag = "v<!-- version -->" }
gpui = { git = "https://github.com/zed-industries/zed" }
gpui-component = { git = "https://github.com/longbridge/gpui-component" }
gpui_platform = { git = "https://github.com/zed-industries/zed", features = ["font-kit", "x11", "wayland", "runtime_shaders"] }
```

Rust 1.89 or newer, edition 2024. On Linux, run `script/install-linux.sh` for
system dependencies first.

## Upstream revisions this release supports

| Crate | Revision |
| --- | --- |
| `gpui-component`, `gpui-component-assets`, `gpui-base` | `<!-- gpui-component-rev -->` |
| `gpui` (zed-industries/zed) | `<!-- zed-rev -->` |

Do not add a `rev` field to your own `gpui` dependency: gpui-component declares
it without one, and differing git specs make Cargo build two incompatible copies
of gpui. The revision above is the one this release's `Cargo.lock` selects, and
it is the pair every gate ran against.

## What is in this release

<!-- release-notes -->

## Tested platforms

The release candidate was validated on Windows 11. Release CI builds macOS,
Linux, and Windows and runs the full test suite on Linux before the tag is cut.

## Known limitations

- gpui-ai is installed from Git rather than crates.io while its GPUI dependency
  graph remains Git-based.
- Live browser demos require WebGPU. Browsers without it receive the captured
  still frame rather than an interactive component.
- `wasm-opt` is not applied to the gallery artifact because it currently makes
  this GPUI build fail at startup.
- Focused-input tests are ignored on macOS because the pinned GPUI test window
  has no native handle; native focused-input behavior is exercised elsewhere.
- The streaming cursor does not animate inside upstream's Markdown view.

## Documentation

The crate stays unpublished while its dependency graph comes from git, so the
API documentation is local: run `cargo doc --open` from a checkout. Architecture
rules and the definition of done are in
[AGENTS.md](https://github.com/labcoder/gpui-ai/blob/v<!-- version -->/AGENTS.md);
contributor workflow is in
[CONTRIBUTING.md](https://github.com/labcoder/gpui-ai/blob/v<!-- version -->/CONTRIBUTING.md).
