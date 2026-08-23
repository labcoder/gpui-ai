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
```

Rust stable, edition 2024 (1.85+). On Linux, run `script/install-linux.sh` for
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

## Documentation

The crate stays unpublished while its dependency graph comes from git, so the
API documentation is local: run `cargo doc --open` from a checkout. Architecture
rules and the definition of done are in
[AGENTS.md](https://github.com/labcoder/gpui-ai/blob/v<!-- version -->/AGENTS.md);
contributor workflow is in
[CONTRIBUTING.md](https://github.com/labcoder/gpui-ai/blob/v<!-- version -->/CONTRIBUTING.md).
