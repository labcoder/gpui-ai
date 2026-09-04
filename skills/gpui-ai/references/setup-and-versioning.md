# Setup and versioning

Resolve compatibility before writing UI. gpui-ai, gpui-component, gpui-base,
and GPUI exchange concrete Rust types; two source identities that compile as
separate crates produce types that look the same and cannot be passed to one
another.

## Resolve the consumer's version

Inspect both files:

```sh
rg 'gpui-ai|gpui-component|gpui_platform|gpui =' Cargo.toml
rg -n -A1 'name = "(gpui-ai|gpui-pre|gpui-pre-platform|gpui-component|gpui-kit-assets|gpui-base)"' Cargo.lock
```

`Cargo.lock` records the version Cargo actually selected. A version printed in
a README or in this skill is not a substitute for it.

GPUI is published under another name: GPUI Kit ships a snapshot of Zed's crate
as `gpui-pre`, so a manifest declares `gpui = { package = "gpui-pre", ... }`
and the lockfile names `gpui-pre`. `use gpui::` paths are unaffected.

When modifying an existing application, preserve its locked versions unless
the user also asked for a dependency upgrade. Read the public API from the
registry source for that exact version under `~/.cargo/registry/src/`.

## Dependency identity

Use the consuming release's own manifest and lockfile as the compatibility
matrix. The shape is:

```toml
[dependencies]
gpui-ai = "<chosen-gpui-ai-version>"

# The versions that gpui-ai release was built against, not the newest.
gpui-component = "<matching-component-version>"
gpui-component-assets = { package = "gpui-kit-assets", version = "<matching-component-version>" }

# GPUI, under the name GPUI Kit publishes it as.
gpui = { package = "gpui-pre", version = "<matching-gpui-pre-version>" }
gpui_platform = { package = "gpui-pre-platform", version = "<matching-gpui-pre-version>", features = ["font-kit", "x11", "wayland", "runtime_shaders"] }
```

Do not copy the placeholders. A gpui-ai release names the versions it was built
against in its GitHub release notes and in `CHANGELOG.md`; `cargo tree` on the
application confirms what actually resolved. If the application does not use
gpui-component APIs or its asset provider directly, it may not need both direct
dependencies; add only what its own source imports.

Keep `Cargo.lock`. It is the record of what the application actually builds.

Symptoms of two GPUI versions in one graph include trait bounds that should
hold, mismatched `App`/`Window`/`Entity` types, or an element from one crate
refusing an apparently identical type from another. GPUI is pre-1.0, so
`0.3.x` and `0.4.x` are separate crates to Cargo and both can resolve at once.
Inspect `cargo tree -d` before rewriting working component code.

## Application initialization

Call `gpui_ai::init(cx)` once before opening a window. It initializes
gpui-component as part of its contract, so do not also call
`gpui_component::init(cx)`.

Every window's first-level view remains a `gpui_component::Root`:

```rust
use gpui::{App, WindowOptions};
use gpui_component::Root;

gpui_platform::application()
    .with_assets(gpui_component_assets::Assets)
    .run(|cx: &mut App| {
        gpui_ai::init(cx);

        let view = cx.new(|cx| AppView::new(cx));
        cx.open_window(WindowOptions::default(), move |window, cx| {
            cx.new(|cx| Root::new(view, window, cx))
        })
        .expect("open the application window");
    });
```

Use `gpui_ai::prelude::*` for normal component work. Import specialized policy
or tuning types from their modules when the prelude intentionally omits them.

If the application chooses global motion, sizing, scrollbar, or popup policy,
set it after `gpui_ai::init` and before opening windows. The policies also
preserve an application choice installed before initialization, but one
consistent setup order is easier to audit.

## Canonical examples

Prefer a compiled example at the locked revision. In the gpui-ai repository,
`crates/gallery/examples/minimal.rs` is the smallest complete wiring example;
the gallery snippets are compiled Rust and supply component-specific usage.
Website prose is secondary because it can outlive a signature or setup change.
