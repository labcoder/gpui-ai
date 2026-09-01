# Setup and versioning

Resolve compatibility before writing UI. gpui-ai, gpui-component, gpui-base,
and GPUI exchange concrete Rust types; two source identities that compile as
separate crates produce types that look the same and cannot be passed to one
another.

## Resolve the consumer's version

Inspect both files:

```sh
rg 'gpui-ai|gpui-component|gpui_platform|gpui =' Cargo.toml
rg -n 'name = "(gpui-ai|gpui|gpui-component|gpui-component-assets)"|source = "git\+' Cargo.lock
```

`Cargo.lock` records the commit Cargo actually selected. A version printed in
a README or in this skill is not a substitute for it.

When modifying an existing application, preserve its locked revision unless
the user also asked for a dependency upgrade. Find the corresponding dependency
source in Cargo's Git checkout or at that repository revision and inspect its
public API there.

## Dependency identity

Use the consuming release's own manifest and lockfile as the compatibility
matrix. The shape is:

```toml
[dependencies]
gpui-ai = { git = "https://github.com/labcoder/gpui-ai", rev = "<chosen-gpui-ai-commit>" }

# Match the exact gpui-component revision selected by that gpui-ai commit.
gpui-component = { git = "https://github.com/longbridge/gpui-component", rev = "<matching-component-commit>" }
gpui-component-assets = { git = "https://github.com/longbridge/gpui-component", rev = "<matching-component-commit>" }

# Leave GPUI without `rev`: gpui-component declares the same source this way.
# Cargo.lock pins the Zed commit shared by the graph.
gpui = { git = "https://github.com/zed-industries/zed" }
gpui_platform = { git = "https://github.com/zed-industries/zed", features = ["font-kit", "x11", "wayland", "runtime_shaders"] }
```

Do not copy the placeholders. Read the selected gpui-ai manifest and lockfile.
If the application does not use gpui-component APIs or its asset provider
directly, it may not need both direct dependencies; add only what its own source
imports.

Keep `Cargo.lock`. It is the pin for GPUI's unrevisioned Git declaration.

Symptoms of duplicate GPUI or gpui-component sources include trait bounds that
should hold, mismatched `App`/`Window`/`Entity` types, or an element from one
crate refusing an apparently identical type from another. Inspect
`cargo tree -d` and the Git source URLs before rewriting working component
code.

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
