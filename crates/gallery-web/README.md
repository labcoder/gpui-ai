# gallery-web

WebAssembly build of the mighty-gpui gallery, for embedding live component demos in the showcase site.

The Rust cdylib reuses the same typed story registry and gallery view as the native `gallery` binary. The `www/` host is plain HTML, CSS, and JavaScript; only the component canvas runs in WebAssembly.

Requirements when wiring this up:

```sh
rustup toolchain install nightly
rustup target add wasm32-unknown-unknown --toolchain nightly
cargo install wasm-bindgen-cli --version 0.2.127
```

Build and preview:

```sh
npm run build:wasm
npm --prefix crates/gallery-web/www install
npm run build:web
npm run dev:web
```

Examples use `embed.html?story=<slug>&theme=light|dark|contrast|system`. Multiple examples are isolated in iframes but reuse the same compiled artifact through the browser cache. The host shows a static fallback when WebGPU or initialization is unavailable and exposes theme updates to running canvases.

The adapter opens a live GPUI canvas in WebGPU-capable browsers. Gallery time is derived from deterministic simulation ticks, so the shared native/WASM story code does not call the unsupported standard-library `Instant` clock on `wasm32-unknown-unknown`.
