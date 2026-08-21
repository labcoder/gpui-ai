import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";

import { buildSite } from "../scripts/build.mjs";
import { components } from "../src/catalog.js";

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
  assert.match(catalog, /24 components/);
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
  await readFile(path.join(outDir, "assets", "catalog.js"), "utf8");
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
