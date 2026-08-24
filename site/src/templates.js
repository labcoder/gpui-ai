import catalog from "../generated/catalog.json" with { type: "json" };

const { categories, components } = catalog;

const escapeHtml = (value) =>
  String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");

const componentLinks = (root, currentSlug = "") => categories.map((category) =>
  `<section><h3>${escapeHtml(category)}</h3>${components
    .filter((item) => item.category === category)
    .map((item) => `<a class="nav-component-link" href="${root}components/${item.slug}/"${item.slug === currentSlug ? ' aria-current="page"' : ""}><span>${String(item.sequence).padStart(2, "0")}</span>${escapeHtml(item.compactLabel)}</a>`)
    .join("")}</section>`).join("");

const catalogGroups = (hrefPrefix = "") => categories.map((category) =>
  `<section class="catalog-group" aria-labelledby="${category.toLowerCase().replaceAll(" ", "-")}"><div class="group-heading"><h2 id="${category.toLowerCase().replaceAll(" ", "-")}">${escapeHtml(category)}</h2><span>${components.filter((item) => item.category === category).length}</span></div><div class="catalog-grid">${components.filter((item) => item.category === category).map((item) => `<a class="catalog-card" data-catalog-item data-title="${escapeHtml(item.title)}" data-category="${escapeHtml(item.category)}" data-summary="${escapeHtml(item.summary)}" href="${hrefPrefix}${item.slug}/"><span class="catalog-number">${String(components.indexOf(item) + 1).padStart(2, "0")}</span><h3>${escapeHtml(item.title)}</h3><p>${escapeHtml(item.summary)}</p><span class="inspect">Inspect component →</span></a>`).join("")}</div></section>`).join("");

function documentShell({ title, description, root, body, bodyClass = "", rail = "", currentSlug = "" }) {
  return `<!doctype html>
<html lang="en" data-theme="light">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta name="description" content="${escapeHtml(description)}">
  <title>${escapeHtml(title)} · gpui-ai</title>
  <link rel="stylesheet" href="${root}assets/styles.css">
  <script type="module" src="${root}assets/shell.js"></script>
</head>
<body${bodyClass ? ` class="${bodyClass}"` : ""}>
  <a class="skip-link" href="#content">Skip to content</a>
  <header class="masthead">
    <a class="wordmark" href="${root}" aria-label="gpui-ai home"><span>gpui</span>-ai</a>
    <div class="header-tools"><nav aria-label="Primary"><a href="${root}components/">Component index</a><a href="https://github.com/labcoder/gpui-ai">Source</a></nav><div class="theme-switcher" role="group" aria-label="Theme"><button type="button" data-theme-choice="light" aria-pressed="true">Light</button><button type="button" data-theme-choice="dark" aria-pressed="false">Dark</button><button type="button" data-theme-choice="contrast" aria-pressed="false">Contrast</button></div><button class="nav-toggle" type="button" data-nav-toggle aria-expanded="false" aria-controls="site-nav-panel">Index</button></div>
  </header>
  <div id="site-nav-panel" role="dialog" aria-modal="true" aria-labelledby="site-nav-title" hidden><div class="nav-backdrop" data-nav-close aria-hidden="true"></div><aside class="nav-drawer"><header><p class="eyebrow">Field index / ${components.length}</p><h2 id="site-nav-title">All components</h2><button type="button" data-nav-close>Close</button></header><nav aria-label="All components">${componentLinks(root, currentSlug)}</nav></aside></div>
  ${rail}
  ${body}
  <footer><span>Field notes for AI interfaces in Rust.</span><nav aria-label="Repository files"><a href="https://github.com/labcoder/gpui-ai/blob/main/README.md">README</a><a href="https://github.com/labcoder/gpui-ai/blob/main/Cargo.toml">Cargo.toml</a><a href="https://github.com/labcoder/gpui-ai/blob/main/site/README.md">Site guide</a></nav><span>${components.length} components · 3 themes · one WASM gallery</span></footer>
</body>
</html>`;
}

export function homePage() {
  const featured = ["loading", "records-table", "prompt-bar"].map((slug) =>
    components.find((item) => item.slug === slug));
  return documentShell({
    title: "AI interface field manual",
    description: "AI-native components for Rust and GPUI.",
    root: "",
    body: `<main id="content" class="home" tabindex="-1">
      <section class="hero" aria-labelledby="hero-title">
        <p class="eyebrow">Field manual / issue 01</p>
        <h1 id="hero-title">The missing interface layer for <em>AI-native</em> Rust apps.</h1>
        <p class="lede">${components.length} composed GPUI components for streaming, tools, decisions, navigation, and dense data. Token-driven. Typed. Built to be read and operated.</p>
        <div class="hero-actions"><a class="button" href="components/">Inspect all ${components.length}</a><a class="text-link" href="https://github.com/labcoder/gpui-ai">Read the source →</a></div>
      </section>
      <dl class="build-metadata" aria-label="Build metadata"><div><dt>Surface</dt><dd>${components.length} stable stories</dd></div><div><dt>Artifact</dt><dd>One shared release WASM</dd></div><div><dt>Distribution</dt><dd>Not published · source checkout</dd></div><div><dt>Build</dt><dd>Static local production output</dd></div></dl>
      <section class="manifesto" aria-label="Library principles">
        <p><strong>01</strong><span>Application-independent components above gpui-component.</span></p>
        <p><strong>02</strong><span>Progress and interaction modeled once with typed state.</span></p>
        <p><strong>03</strong><span>Every visual value resolves through the active theme.</span></p>
      </section>
      <section class="featured" aria-labelledby="featured-title"><div class="featured-heading"><p class="eyebrow">Live field samples</p><h2 id="featured-title">Inspect the working surface.</h2><p>Three real stories from the single shared Rust/WASM gallery build.</p></div>${featured.map((item) => `<article class="featured-specimen" data-featured-specimen data-story="${item.slug}"><div class="section-label"><h3>${escapeHtml(item.title)}</h3><div class="specimen-actions"><span>${escapeHtml(item.category)}</span><button type="button" data-specimen-reload>Reload</button><a data-specimen-open href="gallery/embed.html?story=${item.slug}&amp;theme=light" target="_blank" rel="noopener">Open</a></div></div><div class="specimen-stage specimen-${item.viewport}"><iframe title="Interactive ${escapeHtml(item.title)} featured example" data-specimen-frame data-src="gallery/embed.html?story=${item.slug}&amp;theme=light"></iframe><noscript><p>JavaScript is required to load this live GPUI specimen.</p></noscript></div><a class="featured-link" href="components/${item.slug}/">Read field note →</a></article>`).join("")}</section>
      <section class="architecture-strip" aria-labelledby="architecture-title"><div><p class="eyebrow">Install / current status</p><h2 id="architecture-title">Build from source.</h2><p>gpui-ai is not published yet. Clone the repository and keep the paired GPUI revisions pinned by <code>Cargo.lock</code>.</p><pre><code>git clone https://github.com/labcoder/gpui-ai\ncd gpui-ai\ncargo check --workspace</code></pre></div><dl><div><dt>Page chrome</dt><dd>Page chrome: semantic HTML, CSS, and browser JavaScript.</dd></div><div><dt>Live examples</dt><dd>One shared Rust/WASM gallery; never one binary per page.</dd></div><div><dt>Ownership</dt><dd>Applications own durable data and async work; components report typed intent.</dd></div></dl></section>
      <section class="index-preview" aria-labelledby="preview-title"><p class="eyebrow">Component index</p><h2 id="preview-title">A working vocabulary, not a screenshot pack.</h2><div class="catalog-search"><label for="catalog-search">Find a pattern</label><input id="catalog-search" type="search" data-catalog-search autocomplete="off" placeholder="Search by name, category, or purpose"><p data-catalog-status role="status" aria-live="polite">${components.length} components</p></div>${catalogGroups("components/")}</section>
    </main>`,
  });
}

export function catalogPage() {
  return documentShell({
    title: "Component index",
    description: `Browse all ${components.length} gpui-ai components.`,
    root: "../",
    body: `<main id="content" class="catalog-page" tabindex="-1">
      <header class="page-intro"><p class="eyebrow">Index / ${components.length} components</p><h1>Component field notes</h1><p>Production-oriented primitives for the states AI applications actually inhabit.</p></header>
      <div class="catalog-search"><label for="catalog-search">Find a pattern</label><input id="catalog-search" type="search" data-catalog-search autocomplete="off" placeholder="Search by name, category, or purpose"><p data-catalog-status role="status" aria-live="polite">${components.length} components</p></div>
      ${catalogGroups()}
    </main>`,
  });
}

export function componentPage(item) {
  const index = components.indexOf(item);
  const number = String(index + 1).padStart(2, "0");
  const previous = components[index - 1];
  const next = components[index + 1];
  return documentShell({
    title: item.title,
    description: item.summary,
    root: "../../",
    bodyClass: "has-desktop-rail",
    currentSlug: item.slug,
    rail: `<aside class="desktop-rail" aria-label="Component catalog"><p class="eyebrow">Field index / ${components.length}</p><nav aria-label="Component catalog">${componentLinks("../../", item.slug)}</nav></aside>`,
    body: `<main id="content" class="component-page" tabindex="-1">
      <nav class="breadcrumb" aria-label="Breadcrumb"><a href="../">Components</a><span aria-hidden="true">/</span><span>${escapeHtml(item.title)}</span></nav>
      <header class="component-intro"><p class="eyebrow">Plate ${number} / ${escapeHtml(item.category)}</p><h1>${escapeHtml(item.title)}</h1><p>${escapeHtml(item.summary)}</p><dl class="component-metadata"><div><dt>Story</dt><dd><code>story=${item.slug}</code></dd></div><div><dt>Source</dt><dd><code>${escapeHtml(item.source)}</code></dd></div><div><dt>Public API</dt><dd><code>${escapeHtml(item.api)}</code></dd></div></dl></header>
      <section class="specimen" aria-labelledby="specimen-title" data-story="${item.slug}"><div class="section-label"><h2 id="specimen-title">Live specimen</h2><div class="specimen-actions"><span>GPUI · WASM</span><button type="button" data-specimen-reload>Reload</button><a data-specimen-open href="../../gallery/embed.html?story=${item.slug}&amp;theme=light" target="_blank" rel="noopener">Open</a></div></div><div class="specimen-stage specimen-${item.viewport}"><iframe title="Interactive ${escapeHtml(item.title)} example" data-specimen-frame data-src="../../gallery/embed.html?story=${item.slug}&amp;theme=light"></iframe><noscript><p>JavaScript is required to load the live GPUI specimen.</p></noscript></div><p class="specimen-note">${escapeHtml(item.limitation)}</p></section>
      <section class="usage" aria-labelledby="usage-title"><div><p class="eyebrow">Behavior notes</p><h2 id="usage-title">Caller-owned by design.</h2><ul class="behavior-notes"><li><strong>Ownership.</strong> ${escapeHtml(item.behavior.ownership)}</li><li><strong>Interaction.</strong> ${escapeHtml(item.behavior.interaction)}</li><li><strong>Semantics.</strong> ${escapeHtml(item.behavior.semantics)}</li><li><strong>Overflow and motion.</strong> ${escapeHtml(item.behavior.overflow)}</li></ul><a href="https://github.com/labcoder/gpui-ai/blob/main/${item.source}">Open implementation →</a></div><div class="code-panel"><div><span>Rust</span><button type="button" data-copy aria-describedby="copy-status-${item.slug}">Copy</button></div><pre tabindex="0"><code>${escapeHtml(`use gpui_ai::prelude::*;\n\n${item.usage};`)}</code></pre><p class="copy-status" id="copy-status-${item.slug}" role="status" aria-live="polite"></p></div></section>
      <aside class="verification-boundaries" aria-labelledby="boundaries-title"><p class="eyebrow">Pinned verification boundaries</p><h2 id="boundaries-title">What this browser specimen does not prove.</h2><ul><li>Native-input focus is not fully exercisable through macOS headless TestWindow.</li><li>Pinned WASM keyboard action dispatch can enter unsupported clock paths.</li><li>Assembled native AccessKit actions cannot be injected by TestPlatform.</li><li>Generated wasm-bindgen glue emits a non-fatal Vite direct-eval warning.</li></ul></aside>
      <nav class="plate-nav" aria-label="Component pages"><span class="plate-neighbor">${previous ? `<a href="../${previous.slug}/" rel="prev">← ${escapeHtml(previous.compactLabel)}</a>` : "Start of index"}</span><a href="../">Full index</a><span>${number} / ${components.length}</span><span class="plate-neighbor plate-neighbor-next">${next ? `<a href="../${next.slug}/" rel="next">${escapeHtml(next.compactLabel)} →</a>` : "End of index"}</span></nav>
    </main>`,
  });
}
