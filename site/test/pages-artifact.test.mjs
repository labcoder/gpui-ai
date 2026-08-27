import assert from "node:assert/strict";
import { cp, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { sealSite, verifySite } from "../../script/pages-artifact.mjs";

const source = { sha: "a".repeat(40), runId: "12345" };
async function publication(t) {
  const root = await mkdtemp(path.join(tmpdir(), "gpui-ai-publication-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  for (const file of ["index.html", "404.html", "gallery/embed.html", "gallery/assets/gallery.wasm", "api/gpui_ai/index.html", "posters/chat-dark.webp", "og/index.png"]) {
    await mkdir(path.dirname(path.join(root, file)), { recursive: true });
    await writeFile(path.join(root, file), `fixture: ${file}`);
  }
  return root;
}

test("a publication survives transfer and verifies without rebuilding", async (t) => {
  const root = await publication(t);
  await sealSite(root, source);
  const destination = await mkdtemp(path.join(tmpdir(), "gpui-ai-downloaded-site-"));
  t.after(() => rm(destination, { recursive: true, force: true }));
  await cp(root, destination, { recursive: true });
  await verifySite(destination, source);
  assert.deepEqual(await readFile(path.join(destination, "gallery/assets/gallery.wasm")), await readFile(path.join(root, "gallery/assets/gallery.wasm")));
});

test("an otherwise valid artifact from another commit or run is rejected", async (t) => {
  const root = await publication(t);
  await sealSite(root, source);
  await assert.rejects(verifySite(root, { ...source, sha: "b".repeat(40) }), /different commit/);
  await assert.rejects(verifySite(root, { ...source, runId: "12346" }), /different workflow run/);
});

test("changed bytes, extra files, and missing files fail the handoff", async (t) => {
  const root = await publication(t);
  await sealSite(root, source);
  const wasm = path.join(root, "gallery/assets/gallery.wasm");
  const before = await readFile(wasm);
  await writeFile(wasm, "replaced after the browser test");
  await assert.rejects(verifySite(root, source), /bytes changed/);
  await writeFile(wasm, before);
  await writeFile(path.join(root, "unexpected.html"), "extra");
  await assert.rejects(verifySite(root, source), /bytes changed/);
  await rm(path.join(root, "unexpected.html"));
  await rm(path.join(root, "api/gpui_ai/index.html"));
  await assert.rejects(verifySite(root, source), /missing api/);
});

test("partial builds and malformed provenance cannot be sealed", async (t) => {
  const root = await publication(t);
  await assert.rejects(sealSite(root, { ...source, sha: "main" }), /full commit/);
  await assert.rejects(sealSite(root, { ...source, runId: "" }), /producing workflow/);
  await rm(path.join(root, "og/index.png"));
  await assert.rejects(sealSite(root, source), /missing og/);
});
