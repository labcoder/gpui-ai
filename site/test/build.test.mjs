import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, readdir, rename, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";

import { buildSite } from "../scripts/build.mjs";
import catalog from "../generated/catalog.json" with { type: "json" };

const { components } = catalog;

// Routes the build must emit as real files. Derived the same way the app
// derives them, so a component added in Rust is covered here automatically.
const expectedRoutes = [
  "",
  "components",
  "themes",
  ...components.map((component) => `components/${component.slug}`),
];

async function createGalleryFixture(directory) {
  await mkdir(path.join(directory, "assets"), { recursive: true });
  await Promise.all([
    writeFile(path.join(directory, "index.html"), "gallery index"),
    writeFile(path.join(directory, "embed.html"), "gallery fixture"),
    writeFile(path.join(directory, "assets", "gallery_bg-fixture.wasm"), "wasm"),
  ]);
}

test("every route is a real file, not a client-side route", async (context) => {
  const temporaryRoot = await mkdtemp(path.join(tmpdir(), "mighty-site-"));
  context.after(() => rm(temporaryRoot, { force: true, recursive: true }));
  const galleryDir = path.join(temporaryRoot, "gallery-input");
  const outDir = path.join(temporaryRoot, "site-output");
  await createGalleryFixture(galleryDir);

  await buildSite({ galleryDir, outDir });

  // GitHub Pages has no server to rewrite a deep link, so a route that is not
  // on disk is a hard 404 for anyone who refreshes or arrives from a link.
  for (const route of expectedRoutes) {
    const file = path.join(outDir, route, "index.html");
    const html = await readFile(file, "utf8");
    assert.match(html, /<main/, `${route || "/"} rendered no markup`);
  }

  assert.equal(
    await readFile(path.join(outDir, "gallery", "embed.html"), "utf8"),
    "gallery fixture",
    "the shared gallery is copied once, not rebuilt per page",
  );

  const siblingArtifacts = (await readdir(temporaryRoot)).filter((name) =>
    /\.stage-|\.backup-/.test(name),
  );
  assert.deepEqual(siblingArtifacts, [], "staging directories must not be left behind");
});

test("a page carries its own content before any JavaScript runs", async (context) => {
  const temporaryRoot = await mkdtemp(path.join(tmpdir(), "mighty-prerender-"));
  context.after(() => rm(temporaryRoot, { force: true, recursive: true }));
  const galleryDir = path.join(temporaryRoot, "gallery-input");
  const outDir = path.join(temporaryRoot, "site-output");
  await createGalleryFixture(galleryDir);

  await buildSite({ galleryDir, outDir });

  const component = components[0];
  const page = await readFile(
    path.join(outDir, "components", component.slug, "index.html"),
    "utf8",
  );

  // Pre-rendered, not an empty shell waiting on hydration.
  assert.match(page, new RegExp(component.title.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
  assert.doesNotMatch(page, /<div id="root"><\/div>/, "the root must not be empty");
  assert.doesNotMatch(page, /<!--app-html-->/, "the placeholder must be replaced");

  // Per-route metadata, so a shared link and a search result are not all "gpui-ai".
  assert.match(page, /<title>[^<]*gpui-ai<\/title>/);
  assert.match(page, /<meta name="description" content="[^"]+"/);
  const home = await readFile(path.join(outDir, "index.html"), "utf8");
  const homeTitle = /<title>([^<]*)<\/title>/.exec(home)?.[1];
  const pageTitle = /<title>([^<]*)<\/title>/.exec(page)?.[1];
  assert.notEqual(homeTitle, pageTitle, "each route needs its own title");

  // Assets resolve under the project-page base path.
  assert.match(page, /\/gpui-ai\/assets\//, "assets must be addressed from the Pages base");
});

test("build rejects an incomplete gallery before replacing valid output", async (context) => {
  const temporaryRoot = await mkdtemp(path.join(tmpdir(), "mighty-guard-"));
  context.after(() => rm(temporaryRoot, { force: true, recursive: true }));
  const galleryDir = path.join(temporaryRoot, "gallery-input");
  const outDir = path.join(temporaryRoot, "site-output");
  await createGalleryFixture(galleryDir);
  await buildSite({ galleryDir, outDir });
  const before = await readFile(path.join(outDir, "index.html"), "utf8");

  await rm(path.join(galleryDir, "embed.html"));
  await assert.rejects(buildSite({ galleryDir, outDir }), /Gallery build is incomplete/);

  assert.equal(
    await readFile(path.join(outDir, "index.html"), "utf8"),
    before,
    "a failed build must leave the previous site exactly as it was",
  );
});

test("double promotion failure preserves the prior output backup", async (context) => {
  const temporaryRoot = await mkdtemp(path.join(tmpdir(), "mighty-backup-"));
  context.after(() => rm(temporaryRoot, { force: true, recursive: true }));
  const galleryDir = path.join(temporaryRoot, "gallery-input");
  const outDir = path.join(temporaryRoot, "site-output");
  await createGalleryFixture(galleryDir);
  await buildSite({ galleryDir, outDir });

  let calls = 0;
  const failingRename = async (from, to) => {
    calls += 1;
    // Fail promoting the new output, then fail restoring the backup.
    if (calls >= 2) throw new Error("rename refused");
    return rename(from, to);
  };

  await assert.rejects(buildSite({ galleryDir, outDir, renamePath: failingRename }));

  const leftovers = (await readdir(temporaryRoot)).filter((name) => /\.backup-/.test(name));
  assert.equal(leftovers.length, 1, "the only copy of the previous site must be kept");
  const preserved = await readFile(
    path.join(temporaryRoot, leftovers[0], "previous", "index.html"),
    "utf8",
  );
  assert.match(preserved, /<main/);
});
