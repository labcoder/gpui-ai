import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const generated = path.join(repositoryRoot, "site", "generated", "build.json");

test("the build facts match the manifests the crate is actually built from", async () => {
  const build = JSON.parse(await readFile(generated, "utf8"));
  const lock = await readFile(path.join(repositoryRoot, "Cargo.lock"), "utf8");
  const workspace = await readFile(path.join(repositoryRoot, "Cargo.toml"), "utf8");

  // The home page and the install block tell a visitor which versions this
  // release was built against. gpui-ai, gpui-component, and GPUI move as one
  // set, so a stale number here is worse than no number: it compiles into two
  // copies of GPUI's types.
  assert.match(workspace, new RegExp(`^version = "${build.version}"`, "m"));
  assert.equal(build.upstream.length, 2, "both upstream crates must be published");

  for (const pin of build.upstream) {
    assert.match(pin.version, /^\d+\.\d+\.\d+/, `${pin.id} has no resolved version`);
    assert.ok(
      new RegExp(`\\[\\[package\\]\\]\\nname = "${pin.crate}"\\nversion = "${pin.version}"`).test(lock),
      `${pin.id} is published as ${pin.crate} ${pin.version}, which Cargo.lock does not resolve`,
    );
    assert.match(pin.note, /\S.*\S/, `${pin.id} needs a line explaining what it is`);
  }
});
