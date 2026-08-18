# site

The public showcase site (Phase 5 — not started).

Page chrome is plain web tech: HTML/Vite (React if useful) for navigation, component descriptions, copyable Rust source, and usage snippets. Only the component examples themselves run as WebAssembly, embedded from the single shared `gallery-web` binary with per-story deep links (for example `/gallery?story=prompt-bar&theme=dark`), lazy-loaded as they scroll into view, with a recorded fallback for browsers without WebGPU.

Planned host: Cloudflare. A theme picker beyond light/dark is a strong candidate for v1, since gpui-component's theme registry makes it cheap.

Nothing else lives here yet by design — the site follows the reusable component and gallery foundations.
