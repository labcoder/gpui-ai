// Bind a complete publication to its source/run and verify the same bytes at
// handoff. Artifact downloads also enforce GitHub's archive digest.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { readFile, readdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const manifestName = "gpui-ai-build.json";

function identity({ sha, runId }) {
  assert.match(sha ?? "", /^[0-9a-f]{40}$/, "site source must be a full commit SHA");
  assert.match(String(runId ?? ""), /^[1-9][0-9]*$/, "site must name its producing workflow run");
  return { sha, runId: String(runId) };
}

async function filesIn(directory, prefix = "") {
  const files = {};
  const entries = (await readdir(directory, { withFileTypes: true })).sort((a, b) => a.name.localeCompare(b.name));
  for (const entry of entries) {
    const relative = `${prefix}${entry.name}`;
    if (relative === manifestName) {
      assert.ok(entry.isFile(), "publication manifest must be a regular file");
      continue;
    }
    const file = path.join(directory, entry.name);
    if (entry.isDirectory()) Object.assign(files, await filesIn(file, `${relative}/`));
    else {
      assert.ok(entry.isFile(), `publication contains a non-regular file: ${relative}`);
      files[relative] = createHash("sha256").update(await readFile(file)).digest("hex");
    }
  }
  return files;
}

function requirePublication(files) {
  for (const name of ["index.html", "404.html", "gallery/embed.html", "api/gpui_ai/index.html"]) {
    assert.ok(Object.hasOwn(files, name), `publication is missing ${name}`);
  }
  for (const [prefix, suffix] of [["gallery/assets/", ".wasm"], ["posters/", ".webp"], ["og/", ".png"]]) {
    assert.ok(Object.keys(files).some((file) => file.startsWith(prefix) && file.endsWith(suffix)), `publication is missing ${prefix}*${suffix}`);
  }
}

export async function sealSite(directory, source) {
  const owner = identity(source);
  const files = await filesIn(directory);
  requirePublication(files);
  await writeFile(path.join(directory, manifestName), JSON.stringify({ schema: 1, ...owner, files }, null, 2) + "\n");
}

export async function verifySite(directory, source) {
  const owner = identity(source);
  const manifest = JSON.parse(await readFile(path.join(directory, manifestName), "utf8"));
  assert.equal(manifest.schema, 1, "unknown site artifact schema");
  assert.equal(manifest.sha, owner.sha, "artifact belongs to a different commit");
  assert.equal(manifest.runId, owner.runId, "artifact belongs to a different workflow run");
  const files = await filesIn(directory);
  requirePublication(files);
  assert.deepEqual(files, manifest.files, "publication bytes changed after assembly");
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [command, directory, ...extra] = process.argv.slice(2);
  if (!["seal", "verify"].includes(command) || !directory || extra.length) throw new Error("Usage: pages-artifact.mjs seal|verify <directory>");
  const source = { sha: process.env.SITE_SHA, runId: process.env.SITE_RUN_ID };
  if (command === "seal") {
    assert.equal(execFileSync("git", ["rev-parse", "HEAD"], { encoding: "utf8" }).trim(), source.sha, "checkout must match the publication source");
    await sealSite(directory, source);
  } else await verifySite(directory, source);
  console.log(`Site artifact ${command}: ${source.sha} (run ${source.runId})`);
}
