import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";

test("root workflows expose and compose site checks and builds", async () => {
  const packageJson = JSON.parse(
    await readFile(new URL("../../package.json", import.meta.url), "utf8"),
  );
  const { scripts } = packageJson;

  assert.equal(scripts["check:site"], "npm --prefix site test");
  assert.equal(scripts["build:site"], "npm --prefix site run build");
  assert.match(scripts["check:web"], /check:site/);
  assert.match(scripts["build:web"], /build:site/);
});
