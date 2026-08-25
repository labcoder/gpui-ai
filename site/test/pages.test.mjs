import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { mkdtemp, mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { after, test } from "node:test";
import { fileURLToPath } from "node:url";

import { buildSite } from "../scripts/build.mjs";
import buildInfo from "../generated/build.json" with { type: "json" };
import catalog from "../generated/catalog.json" with { type: "json" };
import highlightFile from "../generated/highlight.json" with { type: "json" };
import snippetFile from "../generated/snippets.json" with { type: "json" };
import themeFile from "../generated/themes.json" with { type: "json" };

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

// What the pages must contain, checked against the HTML the build actually
// writes rather than against the components in isolation. These assertions
// existed before the Vite rewrite and were parked in the order's plan while the
// app had no pages; they are requirements, not markup trivia — a page that
// loses its prev/next links or its API link is broken for a visitor and for a
// crawler, and nothing else would notice.
const { components } = catalog;
const installSnippet = highlightFile.extras.install;
const BASE = "/gpui-ai";
const ROUTES = ["/", "/components/", "/themes/", `/components/${components[0].slug}/`];

// The build is expensive, so every test reads one.
let built;
function site() {
  built ??= (async () => {
    const root = await mkdtemp(path.join(tmpdir(), "mighty-pages-"));
    const galleryDir = path.join(root, "gallery-input");
    const outDir = path.join(root, "site-output");
    await mkdir(path.join(galleryDir, "assets"), { recursive: true });
    await Promise.all([
      writeFile(path.join(galleryDir, "index.html"), "gallery index"),
      writeFile(path.join(galleryDir, "embed.html"), "gallery fixture"),
      writeFile(path.join(galleryDir, "assets", "gallery_bg-fixture.wasm"), "wasm"),
    ]);
    await buildSite({ galleryDir, outDir });
    return { root, outDir };
  })();
  return built;
}

after(async () => {
  if (!built) return;
  const { root } = await built;
  await rm(root, { force: true, recursive: true });
});

async function page(route) {
  const { outDir } = await site();
  return readFile(path.join(outDir, ...route.split("/").filter(Boolean), "index.html"), "utf8");
}

/** Escapes a string the way `renderToString` escapes a text node. */
function asRendered(text) {
  return text
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#x27;");
}

test("the catalog states the count and renders one card per component", async () => {
  const html = await page("/components/");

  assert.match(html, new RegExp(`${components.length} components`));
  for (const component of components) {
    assert.match(
      html,
      new RegExp(`data-component="${component.slug}"`),
      `${component.slug} has no card`,
    );
    assert.match(
      html,
      new RegExp(`href="${BASE}/components/${component.slug}/"`),
      `${component.slug} is not linked`,
    );
  }

  const cards = html.match(/data-component="/g) ?? [];
  assert.equal(cards.length, components.length, "one card each, no duplicates");
});

test("the catalog filter is labelled and reports a live result count", async () => {
  const html = await page("/components/");

  // A search box with no label and no announced count is unusable with a
  // screen reader, and the count is how a visitor knows the filter did
  // anything when the matches are below the fold.
  assert.match(html, /<label for="component-filter">Filter components<\/label>/);
  assert.match(html, /id="component-filter"/);
  assert.match(html, /aria-live="polite"/);
  assert.match(
    html,
    new RegExp(`${components.length} of ${components.length}`),
    "the pre-rendered count must match the unfiltered catalog",
  );
});

test("every component page links stable previous and next neighbours", async () => {
  for (const [index, component] of components.entries()) {
    const html = await page(`/components/${component.slug}/`);
    const previous = components[index - 1];
    const next = components[index + 1];

    if (previous) {
      assert.match(
        html,
        new RegExp(`href="${BASE}/components/${previous.slug}/" rel="prev"`),
        `${component.slug} does not link back to ${previous.slug}`,
      );
    } else {
      assert.doesNotMatch(html, /rel="prev"/, "the first component has nothing before it");
    }

    if (next) {
      assert.match(
        html,
        new RegExp(`href="${BASE}/components/${next.slug}/" rel="next"`),
        `${component.slug} does not link on to ${next.slug}`,
      );
    } else {
      assert.doesNotMatch(html, /rel="next"/, "the last component has nothing after it");
    }
  }
});

test("every component page carries its reference, metadata, and behaviour notes", async () => {
  for (const component of components) {
    const html = await page(`/components/${component.slug}/`);
    const where = component.slug;

    assert.match(html, /class="component-reference"/, `${where} has no reference block`);
    assert.match(html, new RegExp(asRendered(component.api)), `${where} does not name its type`);
    assert.match(
      html,
      new RegExp(component.source.replace("crates/gpui-ai/src/", "").replace(".", "\\.")),
      `${where} does not name its source file`,
    );
    assert.match(html, new RegExp(`${component.height} px`), `${where} does not state its height`);

    for (const note of Object.values(component.behavior)) {
      assert.match(html, new RegExp(asRendered(note)), `${where} is missing a behaviour note`);
    }
    for (const event of component.events) {
      assert.match(html, new RegExp(event), `${where} does not list ${event}`);
    }
  }
});

test("every component page sizes its demo from the measured height", async () => {
  for (const component of components) {
    const html = await page(`/components/${component.slug}/`);

    // The whole point of S-15: a three-chip row must not sit in a box sized
    // for a data table. The frame reads the height the gallery measured.
    assert.match(
      html,
      new RegExp(`--demo-height:\\s*${component.height}px`),
      `${component.slug} does not use its measured height`,
    );
    assert.match(html, /data-specimen-frame/, `${component.slug} has no demo frame`);
    assert.match(
      html,
      new RegExp(`data-src="${BASE}/gallery/embed\\.html\\?story=${component.slug}"`),
      `${component.slug} does not point at its story`,
    );
    // Nothing may fetch the shared binary before the frame is looked at.
    assert.doesNotMatch(html, /<iframe/, `${component.slug} loads its demo eagerly`);
  }
});

/** The text of the first code block on a page, with its highlighting removed. */
function renderedCode(html) {
  const block = /<pre class="code"><code>([\s\S]*?)<\/code><\/pre>/.exec(html)?.[1];
  if (block === undefined) return undefined;
  return block
    .replace(/<[^>]+>/g, "")
    .replaceAll("&#x27;", "'")
    .replaceAll("&quot;", '"')
    .replaceAll("&gt;", ">")
    .replaceAll("&lt;", "<")
    .replaceAll("&amp;", "&")
    .replace(/\n$/, "");
}

test("every component page shows the snippet cut from the gallery, not the one-line usage", async () => {
  let richerThanUsage = 0;

  for (const component of components) {
    const html = await page(`/components/${component.slug}/`);
    const code = snippetFile.snippets[component.slug]?.default;
    assert.ok(code, `${component.slug} has no default snippet`);
    if (code !== component.usage) richerThanUsage += 1;

    // Compared after the highlighting is stripped back off, not by looking for
    // the snippet as one run of text: the tokens are wrapped in spans now. The
    // stronger claim, and the one that matters, is that what a reader sees
    // reassembles into the snippet exactly — not a line missing, not a
    // character changed.
    assert.equal(
      renderedCode(html),
      code,
      `${component.slug} renders code that is not its snippet`,
    );
  }

  // Without this the test would still pass if every page fell back to the
  // one-line `usage` field, which is what these pages used to show.
  assert.ok(
    richerThanUsage > components.length / 2,
    `only ${richerThanUsage} snippets say more than the usage line`,
  );
});

test("code is highlighted in the build, not in the browser", async () => {
  const html = await page(`/components/${components.find((c) => c.slug === "chat").slug}/`);

  // Pre-rendered spans, so the panel is coloured before any JavaScript runs
  // and no highlighter is shipped. Shiki is a devDependency of the generate
  // step; finding it in the bundle would mean it followed the data in.
  assert.match(html, /<pre class="code"><code>.*<span class="t-/s, "the code is not highlighted");
  for (const category of ["keyword", "string", "type"]) {
    assert.match(html, new RegExp(`class="t-${category}"`), `no ${category} tokens were emitted`);
  }

  const { outDir } = await site();
  const bundles = (await readdir(path.join(outDir, "assets"))).filter((name) =>
    name.endsWith(".js"),
  );
  for (const name of bundles) {
    const source = await readFile(path.join(outDir, "assets", name), "utf8");
    assert.doesNotMatch(source, /shiki|textmate|oniguruma/i, `${name} carries a highlighter`);
  }
});

test("code reads like an editor, and the line numbers are not in it", async () => {
  const component = components.find((entry) => entry.slug === "chat") ?? components[0];
  const html = await page(`/components/${component.slug}/`);

  // The file the snippet was cut from, on the strip above it. That is the
  // gallery story, not the component's own implementation file: `source` says
  // where the type lives, and the code on this page came from somewhere else.
  // The snippet markers are only ever in one file, and the exporter publishes
  // which one rather than the site assuming.
  assert.match(
    html,
    new RegExp(`<span class="code-file" data-code-file="${catalog.snippetSource}">`),
    `the snippet does not name ${catalog.snippetSource}`,
  );
  assert.doesNotMatch(
    html,
    new RegExp(`data-code-file="${component.source}"`),
    "the strip names the implementation file, which is not where the snippet came from",
  );

  // Every line is a .code-line, which is what the stylesheet counts to draw
  // the gutter.
  const block = /<pre class="code"><code>([\s\S]*?)<\/code><\/pre>/.exec(html)?.[1];
  assert.ok(block, "the component page has no code block");
  const snippetText = snippetFile.snippets[component.slug].default;
  assert.equal(
    (block.match(/<span class="code-line">/g) ?? []).length,
    snippetText.split("\n").length,
    "one .code-line per line, or the numbering skips",
  );

  // And nothing else. The numbers are generated content, so they are not in
  // the document at all: a number that reached the clipboard would be a
  // snippet that does not compile, and the surest way to prevent that is for
  // it never to be text on the page. Copy reads the data layer, but a reader
  // dragging a selection across the block does not.
  const text = block.replaceAll(/<[^>]+>/g, "").replace(/\n$/, "");
  assert.equal(text, asRendered(snippetText), "the code block holds more than the code");
  assert.doesNotMatch(block, /counter|line-number|data-line/, "a number leaked into the markup");
});

test("the home page's dependency lines are highlighted too", async () => {
  const html = await page("/");

  // They are TOML, not Rust, and they are not cut from a story — which is
  // exactly why they were the one code block on the site still rendering as
  // plain text.
  const block = /<pre class="code"><code>([\s\S]*?)<\/code><\/pre>/.exec(html)?.[1];
  assert.ok(block, "the home page has no code block");
  assert.match(block, /<span class="t-type">dependencies<\/span>/, "the table header is plain");
  assert.match(block, /<span class="t-string">&quot;https/, "the repository URL is plain");

  // Whatever the highlighter did to it, the text is still the two lines that
  // install this release, from the manifests rather than from a component.
  // Every rendered line carries its own newline, the last one included.
  const text = block.replaceAll(/<[^>]+>/g, "").replace(/\n$/, "");
  assert.equal(text, asRendered(installSnippet.code));
  // Quotes arrive escaped, so these compare rendered text, not source.
  assert.ok(
    text.includes(asRendered(`tag = "v${buildInfo.version}"`)),
    `the page does not offer v${buildInfo.version}`,
  );
  assert.ok(text.includes(buildInfo.repository), "the page does not name this repository");
});

test("every component page links its own type under /api/", async () => {
  for (const component of components) {
    const html = await page(`/components/${component.slug}/`);
    const module = path.basename(component.source, ".rs");

    assert.match(
      html,
      new RegExp(`href="${BASE}/api/gpui_ai/${module}/struct\\.${component.api}\\.html"`),
      `${component.slug} does not link ${component.api}'s rustdoc page`,
    );
  }
});

test("every component page explains what the demo does and does not show", async () => {
  for (const component of components) {
    const html = await page(`/components/${component.slug}/`);

    assert.match(html, /What the demo does and does not show/, component.slug);
    assert.match(html, new RegExp(asRendered(component.limitation)), component.slug);
    // Written for a visitor: what they can act on, not how the site is built.
    assert.match(html, /WebGPU/, component.slug);
    assert.doesNotMatch(html, /TestWindow|Vite|eval/, `${component.slug} leaks build internals`);
  }
});

test("the home page publishes what this build is and where it came from", async () => {
  const html = await page("/");

  assert.match(html, new RegExp(`v${buildInfo.version.replace(/\./g, "\\.")}`));
  assert.match(html, new RegExp(buildInfo.license));
  assert.match(html, new RegExp(buildInfo.repository.replace(/[/.]/g, "\\$&")));
  for (const pin of buildInfo.upstream) {
    // A Git dependency is only reproducible if the site says which commit.
    assert.match(html, new RegExp(pin.commit), `the home page hides the ${pin.id} pin`);
  }
  assert.match(html, /How this site works/, "the home page must say what it is doing");
  assert.match(html, new RegExp(`href="${BASE}/api/"`), "the API docs are not linked");
});

test("the home page puts installing it above the demo", async () => {
  const html = await page("/");

  // The hero is a 706px demo. A visitor who has already decided to try this
  // should not have to scroll past it to find the two lines that install it.
  const install = html.indexOf('id="install"');
  const demo = html.indexOf("data-specimen-frame");
  assert.ok(install > 0, "the home page has no install section");
  assert.ok(demo > 0, "the home page has no demo");
  assert.ok(install < demo, `install is at ${install}, below the demo at ${demo}`);
});

test("the home page aligns the install panel and demo with space between them", async () => {
  const html = await page("/");
  const css = await readFile(path.join(repositoryRoot, "site", "app", "site.css"), "utf8");

  assert.match(html, /<section class="home-install" aria-labelledby="install">/);
  assert.match(
    css,
    /\.home-install\s*\{[^}]*max-width:\s*var\(--demo-width\)/s,
    "the install section does not share the demo width",
  );
  assert.match(
    css,
    /\.home-install \+ \.demo\s*\{[^}]*margin-top:\s*var\(--space-6\)/s,
    "the install panel and demo have no gap",
  );
});

test("the README links every component, and every site link it makes resolves", async () => {
  const { outDir } = await site();
  const readme = await readFile(path.join(repositoryRoot, "README.md"), "utf8");
  const origin = buildInfo.homepage.replace(/\/$/, "");

  // The README is the first thing anyone reads and the only page here nothing
  // generates, so its links are the ones most able to rot. A renamed component
  // is a 404 for whoever followed it.
  const missing = [];
  const linked = new Set();
  for (const [, link] of readme.matchAll(
    new RegExp(`\\]\\((${origin.replace(/[/.]/g, "\\$&")}[^)]*)\\)`, "g"),
  )) {
    const route = link.slice(origin.length) || "/";
    // /api/ is rustdoc's tree, built by `npm run build:docs` and assembled by
    // the Pages workflow rather than by this build.
    if (route.startsWith("/api/")) continue;
    if (!existsSync(path.join(outDir, ...route.split("/").filter(Boolean), "index.html"))) {
      missing.push(route);
    }
    const slug = /^\/components\/([a-z0-9-]+)\/$/.exec(route)?.[1];
    if (slug) linked.add(slug);
  }
  assert.deepEqual(missing, [], "the README points at pages the site does not build");

  assert.deepEqual(
    components.map((component) => component.slug).filter((slug) => !linked.has(slug)),
    [],
    "the README's component table has to stay the whole list",
  );
  assert.match(readme, new RegExp(`<summary>View all ${components.length} components</summary>`));
  assert.match(readme, /<summary>Stateless and stateful examples<\/summary>/);

  // Keep the two public counts tied to generated data without requiring badges
  // or an exhaustive table in the README.
  assert.match(
    readme,
    new RegExp(`all ${components.length} components`, "i"),
    `the README does not say there are ${components.length} components`,
  );
  const themeCount = themeFile.groups.reduce((total, group) => total + group.themes.length, 0);
  assert.match(
    readme,
    new RegExp(`includes ${themeCount} themes`, "i"),
    `the README does not say there are ${themeCount} themes`,
  );
});

test("the README's Kind column matches what each type implements", async () => {
  const readme = await readFile(path.join(repositoryRoot, "README.md"), "utf8");
  const directory = path.join(repositoryRoot, "crates", "gpui-ai", "src");
  const sources = (
    await Promise.all(
      (await readdir(directory))
        .filter((name) => name.endsWith(".rs"))
        .map((name) => readFile(path.join(directory, name), "utf8")),
    )
  ).join("\n");

  const rows = [
    ...readme.matchAll(/^\|\s*\[([^\]]+)\]\([^)]*\)\s*\|\s*(stateless|entity)\s*\|/gm),
  ];
  assert.equal(rows.length, components.length, `the table contains ${rows.length} component rows`);

  const wrong = [];
  for (const [, cell, kind] of rows) {
    const names = [...cell.matchAll(/`([^`]+)`/g)].map((match) => match[1]);
    for (const name of names) {
      const entity = new RegExp(`impl Render for ${name}\\b`).test(sources);
      const builder = new RegExp(`impl RenderOnce for ${name}\\b`).test(sources);
      if (!entity && !builder) {
        wrong.push(`${name} implements neither Render nor RenderOnce in this crate`);
        continue;
      }
      const actual = entity ? "entity" : "stateless";
      if (actual !== kind) wrong.push(`${name} is ${actual}, the table says ${kind}`);
    }
  }

  assert.deepEqual(wrong, [], `the table describes ${wrong.length} components wrongly`);
});

test("every face the build produced is preloaded, on every page", async () => {
  const { outDir } = await site();
  const faces = (await readdir(path.join(outDir, "assets"))).filter((name) =>
    name.endsWith(".woff2"),
  );
  assert.ok(faces.length > 0, "the build produced no .woff2 faces");

  // The faces arrive through an @import inside site.css, and Vite does not
  // preload what an @import pulled in. Without these the chrome paints in the
  // system fallback and moves when Plex and Lilex land.
  for (const route of ROUTES) {
    const html = await page(route);
    const links = html.match(/<link rel="preload" as="font"[^>]*>/g) ?? [];
    assert.equal(
      links.length,
      faces.length,
      `${route} preloads ${links.length} of the ${faces.length} faces the build wrote`,
    );
    for (const face of faces) {
      assert.ok(
        links.some((link) => link.includes(face)),
        `${route} does not preload ${face}`,
      );
    }
    // Fonts are fetched in CORS mode even from the same origin; a preload
    // without this is a second download rather than a head start.
    for (const link of links) {
      assert.match(link, /crossorigin/, `a font preload on ${route} is missing crossorigin`);
      assert.match(link, /type="font\/woff2"/);
    }
  }
});

test("every font the site uses is served from the site", async () => {
  const { outDir } = await site();
  const assets = await readdir(path.join(outDir, "assets"));
  const stylesheets = assets.filter((name) => name.endsWith(".css"));
  assert.ok(stylesheets.length > 0, "the build emitted no stylesheet");

  const css = (
    await Promise.all(
      stylesheets.map((name) => readFile(path.join(outDir, "assets", name), "utf8")),
    )
  ).join("\n");

  // A third-party font request tells another host about every visitor and adds
  // a connection the first paint waits on. Self-hosting is the whole reason
  // @fontsource is a dependency rather than a <link> to a CDN.
  const external = css.match(/url\(\s*['"]?(https?:)?\/\/[^)]*\)/g) ?? [];
  assert.deepEqual(external, [], "a stylesheet fetches a font from another origin");
  // An @import survives bundling and fetches at run time like any other URL,
  // so neither the url() rule above nor a list of known vendors would see it.
  const imports = css.match(/@import\s+(url\()?\s*['"]?(https?:)?\/\//g) ?? [];
  assert.deepEqual(imports, [], "a stylesheet imports from another origin");
  assert.doesNotMatch(css, /fonts\.googleapis\.com|fonts\.gstatic\.com|use\.typekit/);

  const families = [...css.matchAll(/@font-face\{[^}]*font-family:\s*([^;}]+)/g)].map((match) =>
    match[1].replace(/['"]/g, "").trim(),
  );
  assert.deepEqual(
    [...new Set(families)].sort(),
    ["IBM Plex Sans", "IBM Plex Serif", "Lilex"],
    "the faces the theme tokens name must be the faces the build ships",
  );
  assert.ok(
    assets.filter((name) => name.endsWith(".woff2")).length >= families.length,
    "each declared face needs a woff2 next to it",
  );
});

test("no page links to a bare index.html", async () => {
  for (const route of ["/", "/components/", "/themes/", `/components/${components[0].slug}/`]) {
    const html = await page(route);
    // Pages serves the directory; linking the file splits every URL in two and
    // makes the client resolve a route the pre-render did not write.
    assert.doesNotMatch(html, /href="[^"]*\/index\.html"/, `${route} links a bare index.html`);
  }
});
