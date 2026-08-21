import { categories, components } from "./catalog.js";

const escapeHtml = (value) =>
  String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");

function documentShell({ title, description, root, body }) {
  return `<!doctype html>
<html lang="en" data-theme="light">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta name="description" content="${escapeHtml(description)}">
  <title>${escapeHtml(title)} · mighty-gpui</title>
  <link rel="stylesheet" href="${root}assets/styles.css">
  <script type="module" src="${root}assets/shell.js"></script>
</head>
<body>
  <a class="skip-link" href="#content">Skip to content</a>
  <header class="masthead">
    <a class="wordmark" href="${root}" aria-label="mighty-gpui home"><span>mighty</span>/gpui</a>
    <div class="header-tools"><nav aria-label="Primary"><a href="${root}components/">Component index</a><a href="https://github.com/labcoder/gpui-ai">Source</a></nav><div class="theme-switcher" role="group" aria-label="Theme"><button type="button" data-theme-choice="light" aria-pressed="true">Light</button><button type="button" data-theme-choice="dark" aria-pressed="false">Dark</button><button type="button" data-theme-choice="contrast" aria-pressed="false">Contrast</button></div></div>
  </header>
  ${body}
  <footer><span>Field notes for AI interfaces in Rust.</span><span>24 components · 3 themes · one WASM gallery</span></footer>
</body>
</html>`;
}

export function homePage() {
  return documentShell({
    title: "AI interface field manual",
    description: "AI-native components for Rust and GPUI.",
    root: "",
    body: `<main id="content" class="home">
      <section class="hero" aria-labelledby="hero-title">
        <p class="eyebrow">Field manual / issue 01</p>
        <h1 id="hero-title">The missing interface layer for <em>AI-native</em> Rust apps.</h1>
        <p class="lede">Twenty-four composed GPUI components for streaming, tools, decisions, navigation, and dense data. Token-driven. Typed. Built to be read and operated.</p>
        <div class="hero-actions"><a class="button" href="components/">Inspect all 24</a><a class="text-link" href="https://github.com/labcoder/gpui-ai">Read the source →</a></div>
      </section>
      <section class="manifesto" aria-label="Library principles">
        <p><strong>01</strong><span>Application-independent components above gpui-component.</span></p>
        <p><strong>02</strong><span>Progress and interaction modeled once with typed state.</span></p>
        <p><strong>03</strong><span>Every visual value resolves through the active theme.</span></p>
      </section>
      <section class="index-preview" aria-labelledby="preview-title"><p class="eyebrow">Component index</p><h2 id="preview-title">A working vocabulary, not a screenshot pack.</h2><div class="ticker">${components.map((item, index) => `<a href="components/${item.slug}/"><span>${String(index + 1).padStart(2, "0")}</span>${escapeHtml(item.title)}</a>`).join("")}</div></section>
    </main>`,
  });
}

export function catalogPage() {
  return documentShell({
    title: "Component index",
    description: "Browse all 24 mighty-gpui components.",
    root: "../",
    body: `<main id="content" class="catalog-page">
      <header class="page-intro"><p class="eyebrow">Index / 24 components</p><h1>Component field notes</h1><p>Production-oriented primitives for the states AI applications actually inhabit.</p></header>
      ${categories.map((category) => `<section class="catalog-group" aria-labelledby="${category.toLowerCase().replaceAll(" ", "-")}"><div class="group-heading"><h2 id="${category.toLowerCase().replaceAll(" ", "-")}">${escapeHtml(category)}</h2><span>${components.filter((item) => item.category === category).length}</span></div><div class="catalog-grid">${components.filter((item) => item.category === category).map((item) => `<a class="catalog-card" href="${item.slug}/"><span class="catalog-number">${String(components.indexOf(item) + 1).padStart(2, "0")}</span><h3>${escapeHtml(item.title)}</h3><p>${escapeHtml(item.summary)}</p><span class="inspect">Inspect component →</span></a>`).join("")}</div></section>`).join("")}
    </main>`,
  });
}

export function componentPage(item) {
  const number = String(components.indexOf(item) + 1).padStart(2, "0");
  return documentShell({
    title: item.title,
    description: item.summary,
    root: "../../",
    body: `<main id="content" class="component-page">
      <nav class="breadcrumb" aria-label="Breadcrumb"><a href="../">Components</a><span aria-hidden="true">/</span><span>${escapeHtml(item.title)}</span></nav>
      <header class="component-intro"><p class="eyebrow">Plate ${number} / ${escapeHtml(item.category)}</p><h1>${escapeHtml(item.title)}</h1><p>${escapeHtml(item.summary)}</p></header>
      <section class="specimen" aria-labelledby="specimen-title" data-story="${item.slug}"><div class="section-label"><h2 id="specimen-title">Live specimen</h2><div class="specimen-actions"><span>GPUI · WASM</span><button type="button" data-specimen-reload>Reload</button><a data-specimen-open href="../../gallery/embed.html?story=${item.slug}&amp;theme=light" target="_blank" rel="noopener">Open</a></div></div><div class="specimen-stage specimen-${item.viewport}"><iframe title="Interactive ${escapeHtml(item.title)} example" data-specimen-frame data-src="../../gallery/embed.html?story=${item.slug}&amp;theme=light"></iframe><div class="webgpu-fallback" data-webgpu-fallback hidden role="status"><strong>Live specimen unavailable</strong><p>This browser does not expose WebGPU. The native GPUI component and source remain available.</p></div><noscript><p>JavaScript is required to load the live GPUI specimen.</p></noscript></div><p class="specimen-note">${escapeHtml(item.limitation)}</p></section>
      <section class="usage" aria-labelledby="usage-title"><div><p class="eyebrow">Usage note</p><h2 id="usage-title">Start with stable identity.</h2><p>The application owns durable data and work. The component reports intent through typed events.</p><a href="https://github.com/labcoder/gpui-ai/blob/main/${item.source}">Open implementation →</a></div><div class="code-panel"><div><span>Rust</span><button type="button" data-copy aria-describedby="copy-status-${item.slug}">Copy</button></div><pre tabindex="0"><code>${escapeHtml(`use mighty_gpui::prelude::*;\n\n${item.usage};`)}</code></pre><p class="copy-status" id="copy-status-${item.slug}" role="status" aria-live="polite"></p></div></section>
      <nav class="plate-nav" aria-label="Component pages"><a href="../">← Full index</a><span>${number} / ${components.length}</span></nav>
    </main>`,
  });
}
