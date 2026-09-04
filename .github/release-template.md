# gpui-ai <!-- version -->

AI-native UI components for [GPUI](https://gpui.rs) — streamed answers, thinking
traces, tool calls, approval gates, chat, and live task status — built on
[gpui-component](https://github.com/longbridge/gpui-component).

## Install

```sh
cargo add gpui-ai@<!-- version -->
```

GPUI itself is published under another name — GPUI Kit ships a snapshot of
Zed's crate as `gpui-pre` — so an application declares it with a rename, which
keeps every `use gpui::` path working:

```toml
[dependencies]
gpui-ai = "<!-- version -->"
gpui = { package = "gpui-pre", version = "<!-- gpui-pre-version -->" }
gpui-component = "<!-- gpui-component-version -->"
gpui_platform = { package = "gpui-pre-platform", version = "<!-- gpui-pre-version -->", features = ["font-kit", "x11", "wayland", "runtime_shaders"] }
```

Rust 1.89 or newer, edition 2024. On Linux, run `script/install-linux.sh` for
system dependencies first.

## Upstream versions this release supports

| Crate | Version |
| --- | --- |
| `gpui-component`, `gpui-kit-assets`, `gpui-base` | `<!-- gpui-component-version -->` |
| `gpui-pre`, `gpui-pre-platform` | `<!-- gpui-pre-version -->` |

These are the versions this release's `Cargo.lock` selects, and the set every
gate ran against. They move together: a `gpui-pre` that does not match the one
under your `gpui-component` gives you two incompatible copies of GPUI's types.

## What is in this release

<!-- release-notes -->

## Tested platforms

The release candidate was validated on Windows 11. Release CI builds macOS,
Linux, and Windows and runs the full test suite on Linux before the tag is cut.

## Known limitations

- **On a touch device, tapping out of a text input and back in does not bring
  the keyboard back.** A defect in GPUI's own web backend, not in this library:
  `gpui_web` skips its virtual-keyboard sync when the same input was focused
  before and after a tap, but the tap that left the input has already torn down
  the hidden IME element without moving GPUI's focus — so the tap that should
  revive it is the one that gets skipped. Reloading the demo restores typing,
  and the draft text survives. Present in every GPUI web application on
  `gpui-pre` 0.3.3; `site/test/release/mobile.test.mjs` pins the behaviour and
  fails once upstream fixes it.
- Live browser demos require WebGPU. Browsers without it receive the captured
  still frame rather than an interactive component.
- `wasm-opt` is not applied to the gallery artifact because it currently makes
  this GPUI build fail at startup.
- Focused-input tests are ignored on macOS because the pinned GPUI test window
  has no native handle; native focused-input behavior is exercised elsewhere.
- The streaming cursor does not animate inside upstream's Markdown view.

## Documentation

API documentation is on [docs.rs](https://docs.rs/gpui-ai/<!-- version -->).
Architecture rules and the definition of done are in
[AGENTS.md](https://github.com/labcoder/gpui-ai/blob/v<!-- version -->/AGENTS.md);
contributor workflow is in
[CONTRIBUTING.md](https://github.com/labcoder/gpui-ai/blob/v<!-- version -->/CONTRIBUTING.md).
