import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { after, test } from "node:test";

import { buildSite } from "../scripts/build.mjs";
import buildInfo from "../generated/build.json" with { type: "json" };
import catalog from "../generated/catalog.json" with { type: "json" };
import snippetFile from "../generated/snippets.json" with { type: "json" };

// What the pages must contain, checked against the HTML the build actually
// writes rather than against the components in isolation. These assertions
// existed before the Vite rewrite and were parked in the order's plan while the
// app had no pages; they are requirements, not markup trivia — a page that
// loses its prev/next links or its API link is broken for a visitor and for a
// crawler, and nothing else would notice.
const { components } = catalog;
const BASE = "/gpui-ai";

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

test("every component page carries a rail, its metadata, and its behaviour notes", async () => {
  for (const component of components) {
    const html = await page(`/components/${component.slug}/`);
    const where = component.slug;

    assert.match(html, /class="component-rail"/, `${where} has no rail`);
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

test("every component page shows the snippet cut from the gallery, not the one-line usage", async () => {
  let richerThanUsage = 0;

  for (const component of components) {
    const html = await page(`/components/${component.slug}/`);
    const code = snippetFile.snippets[component.slug]?.default;
    assert.ok(code, `${component.slug} has no default snippet`);
    if (code !== component.usage) richerThanUsage += 1;

    assert.match(
      html,
      new RegExp(asRendered(code).replace(/[.*+?^${}()|[\]\\]/g, "\\$&")),
      `${component.slug} does not render its snippet`,
    );
  }

  // Without this the test would still pass if every page fell back to the
  // one-line `usage` field, which is what these pages used to show.
  assert.ok(
    richerThanUsage > components.length / 2,
    `only ${richerThanUsage} snippets say more than the usage line`,
  );
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

test("no page links to a bare index.html", async () => {
  for (const route of ["/", "/components/", "/themes/", `/components/${components[0].slug}/`]) {
    const html = await page(route);
    // Pages serves the directory; linking the file splits every URL in two and
    // makes the client resolve a route the pre-render did not write.
    assert.doesNotMatch(html, /href="[^"]*\/index\.html"/, `${route} links a bare index.html`);
  }
});
