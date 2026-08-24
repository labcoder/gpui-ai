import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, readdir, rename, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";

import { buildSite } from "../scripts/build.mjs";
import catalog from "../generated/catalog.json" with { type: "json" };

const { components } = catalog;

async function createGalleryFixture(directory) {
  await mkdir(path.join(directory, "assets"), { recursive: true });
  await Promise.all([
    writeFile(path.join(directory, "index.html"), "gallery index"),
    writeFile(path.join(directory, "embed.html"), "gallery fixture"),
    writeFile(path.join(directory, "assets", "gallery_bg-fixture.wasm"), "wasm"),
  ]);
}

test("build emits stable catalog routes and copies one shared gallery", async (context) => {
  const temporaryRoot = await mkdtemp(path.join(tmpdir(), "mighty-site-"));
  context.after(() => rm(temporaryRoot, { force: true, recursive: true }));
  const galleryDir = path.join(temporaryRoot, "gallery-input");
  const outDir = path.join(temporaryRoot, "site-output");
  await createGalleryFixture(galleryDir);

  await buildSite({ galleryDir, outDir });

  const home = await readFile(path.join(outDir, "index.html"), "utf8");
  const catalog = await readFile(path.join(outDir, "components", "index.html"), "utf8");
  assert.match(home, /<main/);
  assert.match(catalog, new RegExp(`${components.length} components`));
  assert.equal(
    (catalog.match(/class="catalog-card"/g) ?? []).length,
    components.length,
  );

  for (const component of components) {
    const page = await readFile(
      path.join(outDir, "components", component.slug, "index.html"),
      "utf8",
    );
    assert.match(page, /<main/);
    assert.match(page, new RegExp(`data-story="${component.slug}"`));
    assert.match(page, /data-specimen-frame/);
    assert.doesNotMatch(page, /data-webgpu-fallback/);
    assert.match(page, /data-specimen-reload/);
    assert.match(page, /data-specimen-open/);
    assert.match(
      page,
      new RegExp(`data-src="\\.\\.\\/\\.\\.\\/gallery/embed\\.html\\?story=${component.slug}&amp;theme=light"`),
    );
    assert.doesNotMatch(page, /<iframe[^>]+\ssrc=/);
    assert.match(page, /<pre[^>]*><code/);
    assert.match(page, /href="\.\.\/"/);
    assert.doesNotMatch(page, /href="[^"#]*index\.html/);
  }

  assert.equal(
    await readFile(path.join(outDir, "gallery", "embed.html"), "utf8"),
    "gallery fixture",
  );
  const siblingArtifacts = (await readdir(temporaryRoot)).filter((name) =>
    /\.stage-|\.backup-/.test(name),
  );
  assert.deepEqual(siblingArtifacts, []);
});

test("generated pages share the same local assets", async (context) => {
  const temporaryRoot = await mkdtemp(path.join(tmpdir(), "mighty-assets-"));
  context.after(() => rm(temporaryRoot, { force: true, recursive: true }));
  const galleryDir = path.join(temporaryRoot, "gallery-input");
  const outDir = path.join(temporaryRoot, "site-output");
  await createGalleryFixture(galleryDir);

  await buildSite({ galleryDir, outDir });

  const page = await readFile(
    path.join(outDir, "components", components[0].slug, "index.html"),
    "utf8",
  );
  assert.match(page, /href="\.\.\/\.\.\/assets\/styles\.css"/);
  assert.match(page, /src="\.\.\/\.\.\/assets\/shell\.js"/);
  await readFile(path.join(outDir, "assets", "styles.css"), "utf8");
  await readFile(path.join(outDir, "assets", "shell.js"), "utf8");
  await readFile(path.join(outDir, "assets", "runtime.js"), "utf8");
});

test("every page exposes keyboard-operable three-theme controls", async (context) => {
  const temporaryRoot = await mkdtemp(path.join(tmpdir(), "mighty-themes-"));
  context.after(() => rm(temporaryRoot, { force: true, recursive: true }));
  const galleryDir = path.join(temporaryRoot, "gallery-input");
  const outDir = path.join(temporaryRoot, "site-output");
  await createGalleryFixture(galleryDir);
  await buildSite({ galleryDir, outDir });

  for (const route of ["index.html", path.join("components", "index.html"), path.join("components", "chat", "index.html")]) {
    const page = await readFile(path.join(outDir, route), "utf8");
    assert.match(page, /aria-label="Theme"/);
    assert.match(page, /<main id="content"[^>]+tabindex="-1"/);
    assert.equal((page.match(/data-theme-choice=/g) ?? []).length, 3);
    assert.match(page, /data-nav-toggle[^>]+aria-expanded="false"[^>]+aria-controls="site-nav-panel"/);
    assert.match(page, /id="site-nav-panel"[^>]+role="dialog"[^>]+aria-modal="true"[^>]+hidden/);
    const navigationCopies = route === "index.html" || route === path.join("components", "index.html") ? 1 : 2;
    assert.equal((page.match(/class="nav-component-link"/g) ?? []).length, components.length * navigationCopies);
  }
});

test("catalog page has a labeled search and live result count", async (context) => {
  const temporaryRoot = await mkdtemp(path.join(tmpdir(), "mighty-search-"));
  context.after(() => rm(temporaryRoot, { force: true, recursive: true }));
  const galleryDir = path.join(temporaryRoot, "gallery-input");
  const outDir = path.join(temporaryRoot, "site-output");
  await createGalleryFixture(galleryDir);
  await buildSite({ galleryDir, outDir });
  const catalog = await readFile(path.join(outDir, "components", "index.html"), "utf8");

  assert.match(catalog, /<label for="catalog-search">Find a pattern<\/label><input id="catalog-search" type="search" data-catalog-search/);
  assert.match(catalog, /data-catalog-status[^>]+role="status"[^>]+aria-live="polite"/);
  assert.equal((catalog.match(/data-catalog-item/g) ?? []).length, components.length);
});

test("home features three shared-gallery specimens", async (context) => {
  const temporaryRoot = await mkdtemp(path.join(tmpdir(), "mighty-featured-"));
  context.after(() => rm(temporaryRoot, { force: true, recursive: true }));
  const galleryDir = path.join(temporaryRoot, "gallery-input");
  const outDir = path.join(temporaryRoot, "site-output");
  await createGalleryFixture(galleryDir);
  await buildSite({ galleryDir, outDir });
  const home = await readFile(path.join(outDir, "index.html"), "utf8");

  assert.equal((home.match(/data-specimen-frame/g) ?? []).length, 3);
  assert.equal((home.match(/data-featured-specimen/g) ?? []).length, 3);
  assert.equal((home.match(/data-specimen-reload/g) ?? []).length, 3);
  assert.doesNotMatch(home, /data-webgpu-fallback/);
  assert.equal((home.match(/gallery\/embed\.html\?story=/g) ?? []).length, 6);
});

test("home publishes honest build metadata, architecture, and repository links", async (context) => {
  const temporaryRoot = await mkdtemp(path.join(tmpdir(), "mighty-home-anatomy-"));
  context.after(() => rm(temporaryRoot, { force: true, recursive: true }));
  const galleryDir = path.join(temporaryRoot, "gallery-input");
  const outDir = path.join(temporaryRoot, "site-output");
  await createGalleryFixture(galleryDir);
  await buildSite({ galleryDir, outDir });
  const home = await readFile(path.join(outDir, "index.html"), "utf8");

  assert.match(home, /class="build-metadata" aria-label="Build metadata"/);
  assert.match(home, new RegExp(`${components.length} stable stories`));
  assert.match(home, /One shared release WASM/);
  assert.match(home, /Not published/);
  assert.match(home, /class="architecture-strip" aria-labelledby="architecture-title"/);
  assert.match(home, /Page chrome: semantic HTML, CSS, and browser JavaScript/);
  assert.match(home, /<label for="catalog-search">Find a pattern<\/label><input id="catalog-search" type="search" data-catalog-search/);
  assert.equal((home.match(/data-catalog-item/g) ?? []).length, components.length);
  assert.equal((home.match(/class="catalog-group"/g) ?? []).length, new Set(components.map(({ category }) => category)).size);
  for (const file of ["README.md", "Cargo.toml", "site/README.md"]) {
    assert.match(home, new RegExp(`github\\.com/labcoder/gpui-ai/blob/main/${file.replaceAll("/", "\\/")}`));
  }
});

test("component pages link to stable previous and next neighbors", async (context) => {
  const temporaryRoot = await mkdtemp(path.join(tmpdir(), "mighty-neighbors-"));
  context.after(() => rm(temporaryRoot, { force: true, recursive: true }));
  const galleryDir = path.join(temporaryRoot, "gallery-input");
  const outDir = path.join(temporaryRoot, "site-output");
  await createGalleryFixture(galleryDir);
  await buildSite({ galleryDir, outDir });

  const first = await readFile(path.join(outDir, "components", components[0].slug, "index.html"), "utf8");
  const middle = await readFile(path.join(outDir, "components", components[1].slug, "index.html"), "utf8");
  const last = await readFile(path.join(outDir, "components", components.at(-1).slug, "index.html"), "utf8");
  assert.doesNotMatch(first, /rel="prev"/);
  assert.match(first, new RegExp(`href="\.\./${components[1].slug}/" rel="next"`));
  assert.match(middle, new RegExp(`href="\.\./${components[0].slug}/" rel="prev"`));
  assert.match(middle, new RegExp(`href="\.\./${components[2].slug}/" rel="next"`));
  assert.match(last, new RegExp(`href="\.\./${components.at(-2).slug}/" rel="prev"`));
  assert.doesNotMatch(last, /rel="next"/);
});

test("component pages expose a desktop rail, visible metadata, and behavior notes", async (context) => {
  const temporaryRoot = await mkdtemp(path.join(tmpdir(), "mighty-component-shell-"));
  context.after(() => rm(temporaryRoot, { force: true, recursive: true }));
  const galleryDir = path.join(temporaryRoot, "gallery-input");
  const outDir = path.join(temporaryRoot, "site-output");
  await createGalleryFixture(galleryDir);
  await buildSite({ galleryDir, outDir });
  const item = components.find(({ slug }) => slug === "records-table");
  const page = await readFile(path.join(outDir, "components", item.slug, "index.html"), "utf8");

  assert.match(page, /<body class="has-desktop-rail">/);
  assert.match(page, /class="desktop-rail" aria-label="Component catalog"/);
  assert.equal((page.match(/aria-current="page"/g) ?? []).length, 2);
  assert.match(page, new RegExp(`story=${item.slug}`));
  assert.match(page, new RegExp(item.source.replaceAll("/", "\\/")));
  assert.match(page, /class="behavior-notes"/);
  assert.match(page, /Interactive intent is reported through the typed RecordsTableEvent contract and stable application IDs\./);
  assert.match(page, /Pinned verification boundaries/);
  assert.match(page, /wasm-bindgen glue emits a non-fatal Vite direct-eval warning/);

  const noninteractive = await readFile(path.join(outDir, "components", components[0].slug, "index.html"), "utf8");
  assert.match(noninteractive, /This presentation surface adds no component-specific interaction event\./);
  assert.doesNotMatch(noninteractive, /typed [A-Za-z0-9_]+Event contract/);
});

test("drawer backdrop is pointer-only while the named close button stays focusable", async (context) => {
  const temporaryRoot = await mkdtemp(path.join(tmpdir(), "mighty-backdrop-"));
  context.after(() => rm(temporaryRoot, { force: true, recursive: true }));
  const galleryDir = path.join(temporaryRoot, "gallery-input");
  const outDir = path.join(temporaryRoot, "site-output");
  await createGalleryFixture(galleryDir);
  await buildSite({ galleryDir, outDir });
  const page = await readFile(path.join(outDir, "index.html"), "utf8");

  assert.match(page, /<div class="nav-backdrop" data-nav-close aria-hidden="true"><\/div>/);
  assert.match(page, /<button type="button" data-nav-close>Close<\/button>/);
  assert.doesNotMatch(page, /<button class="nav-backdrop"/);
});

test("build rejects an incomplete gallery before replacing valid output", async (context) => {
  const temporaryRoot = await mkdtemp(path.join(tmpdir(), "mighty-preflight-"));
  context.after(() => rm(temporaryRoot, { force: true, recursive: true }));
  const galleryDir = path.join(temporaryRoot, "gallery-input");
  const outDir = path.join(temporaryRoot, "site-output");
  await mkdir(galleryDir);
  await mkdir(outDir);
  await writeFile(path.join(galleryDir, "embed.html"), "incomplete");
  await writeFile(path.join(outDir, "sentinel.txt"), "preserve me");

  await assert.rejects(
    buildSite({ galleryDir, outDir }),
    /gallery build is incomplete/i,
  );
  assert.equal(await readFile(path.join(outDir, "sentinel.txt"), "utf8"), "preserve me");
});

test("double promotion failure preserves the prior output backup", async (context) => {
  const temporaryRoot = await mkdtemp(path.join(tmpdir(), "mighty-double-rollback-"));
  context.after(() => rm(temporaryRoot, { force: true, recursive: true }));
  const galleryDir = path.join(temporaryRoot, "gallery-input");
  const outDir = path.join(temporaryRoot, "site-output");
  await createGalleryFixture(galleryDir);
  await mkdir(outDir, { recursive: true });
  await writeFile(path.join(outDir, "index.html"), "previous production output");
  let renameCount = 0;

  await assert.rejects(
    buildSite({
      galleryDir,
      outDir,
      renamePath: async (source, destination) => {
        renameCount += 1;
        if (renameCount === 1) return rename(source, destination);
        throw new Error(renameCount === 2 ? "injected promotion failure" : "injected rollback failure");
      },
    }),
    /backup preserved at/,
  );

  const backups = (await readdir(temporaryRoot, { withFileTypes: true }))
    .filter((entry) => entry.isDirectory() && entry.name.startsWith(".site-output.backup-"));
  assert.equal(backups.length, 1);
  assert.equal(
    await readFile(path.join(temporaryRoot, backups[0].name, "previous", "index.html"), "utf8"),
    "previous production output",
  );
});
