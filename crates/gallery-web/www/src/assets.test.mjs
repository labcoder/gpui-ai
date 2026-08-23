import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";

const pages = ["index.html", "embed.html"];

test("no page sends a visitor to a third-party origin for assets", async () => {
  for (const page of pages) {
    const markup = await readFile(new URL(`../${page}`, import.meta.url), "utf8");
    const base = /data-asset-base="([^"]*)"/.exec(markup)?.[1];

    if (base !== undefined) {
      assert.doesNotMatch(
        base,
        /^[a-z]+:\/\//i,
        `${page} must resolve its asset base against its own origin, not ${base}`,
      );
    }
    assert.doesNotMatch(
      markup,
      /longbridge\.github\.io/,
      `${page} must not fetch upstream assets at runtime`,
    );
  }
});

test("the embed asks for the copied icon set", async () => {
  const markup = await readFile(new URL("../embed.html", import.meta.url), "utf8");
  assert.match(
    markup,
    /data-asset-base="\.\/upstream"/,
    "copy-icons.mjs writes public/upstream/assets/icons, so the embed must point there",
  );
});
