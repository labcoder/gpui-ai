import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const generated = path.join(repositoryRoot, "site", "generated", "build.json");

test("the published build facts are current and regenerating is idempotent", async () => {
  const committed = await readFile(generated, "utf8");

  const run = spawnSync(process.execPath, ["script/generate-build-info.mjs"], {
    cwd: repositoryRoot,
    encoding: "utf8",
  });
  assert.equal(run.status, 0, `the generator failed: ${run.stderr}`);

  assert.equal(
    await readFile(generated, "utf8"),
    committed,
    "site/generated/build.json is stale — run npm run generate and commit the result",
  );
});

test("the build facts match the manifests the crate is actually built from", async () => {
  const build = JSON.parse(await readFile(generated, "utf8"));
  const lock = await readFile(path.join(repositoryRoot, "Cargo.lock"), "utf8");
  const workspace = await readFile(path.join(repositoryRoot, "Cargo.toml"), "utf8");

  // The home page tells a visitor which commits this release is pinned to.
  // That is the only reproducible thing about a Git dependency, so a stale
  // number here is worse than no number.
  assert.match(workspace, new RegExp(`^version = "${build.version}"`, "m"));
  assert.equal(build.upstream.length, 2, "both upstream repositories must be published");

  for (const pin of build.upstream) {
    assert.match(pin.commit, /^[0-9a-f]{40}$/, `${pin.id} has no resolved commit`);
    // A `rev` spec puts a query string between the URL and the commit, so
    // match the resolved commit against the repository rather than the whole
    // source string.
    assert.ok(
      new RegExp(`git\\+${pin.repository}[^#"]*#${pin.commit}`).test(lock),
      `${pin.id} is pinned to ${pin.commit}, which Cargo.lock does not resolve`,
    );
    assert.match(pin.note, /\S.*\S/, `${pin.id} needs a line explaining what it is`);
  }
});
