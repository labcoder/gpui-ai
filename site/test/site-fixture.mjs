// Imported by the single artifact-suite entrypoint. One build per Node process,
// never a pre-existing developer dist. Fault-injection tests own separate roots.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { after } from "node:test";
import { buildSite } from "../scripts/build.mjs";

export async function createGalleryFixture(directory) {
  await mkdir(path.join(directory, "assets"), { recursive: true });
  await Promise.all([
    writeFile(path.join(directory, "index.html"), "gallery index"),
    writeFile(path.join(directory, "embed.html"), "gallery fixture"),
    writeFile(path.join(directory, "assets", "gallery_bg-fixture.wasm"), "wasm"),
  ]);
}

async function digest(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  return Promise.all(entries.sort((a, b) => a.name.localeCompare(b.name)).map(async entry => [
    entry.name, entry.isDirectory() ? await digest(path.join(directory, entry.name))
      : createHash("sha256").update(await readFile(path.join(directory, entry.name))).digest("hex"),
  ]));
}

let built;
let initial;
export function site() {
  built ??= (async () => {
    const root = await mkdtemp(path.join(tmpdir(), "gpui-ai-site-artifact-"));
    const outDir = path.join(root, "site-output"), galleryDir = path.join(root, "gallery-input");
    try {
      const start = performance.now();
      await createGalleryFixture(galleryDir);
      await buildSite({ galleryDir, outDir });
      initial = await digest(outDir);
      console.log(`Shared site artifact built once in ${(performance.now() - start).toFixed(0)} ms`);
      return Object.freeze({ root, outDir });
    } catch (error) {
      await rm(root, { recursive: true, force: true });
      throw error;
    }
  })();
  return built;
}

after(async () => {
  if (!built) return;
  let artifact;
  try { artifact = await built; } catch { return; } // failed setup already cleaned its root
  try {
    assert.deepEqual(await digest(artifact.outDir), initial, "read-only artifact checks mutated their fixture");
  } finally {
    await rm(artifact.root, { recursive: true, force: true });
  }
});
