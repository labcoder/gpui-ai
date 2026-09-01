import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";
import path from "node:path";
import { fileURLToPath } from "node:url";

import catalog from "../generated/catalog.json" with { type: "json" };

const ROOT = fileURLToPath(new URL("../..", import.meta.url));
const SKILL = path.join(ROOT, "skills", "gpui-ai");

test("the consumer skill is lean and every routed reference exists", async () => {
  const body = await readFile(path.join(SKILL, "SKILL.md"), "utf8");
  assert.match(body, /^---\r?\nname: gpui-ai\r?\n/);
  assert.match(body, /description: .*streamed model output/);
  assert.ok(body.split(/\r?\n/).length < 500, "SKILL.md should use progressive disclosure");

  const references = [...body.matchAll(/\]\((references\/[^)#]+\.md)\)/g)]
    .map((match) => match[1]);
  assert.ok(references.length >= 8, "the routing table should expose the focused references");

  for (const reference of new Set(references)) {
    await assert.doesNotReject(
      readFile(path.join(SKILL, reference), "utf8"),
      `${reference} is linked but missing`,
    );
  }
});

test("the generated skill index is exactly the generated component catalog", async () => {
  const components = await readFile(
    path.join(SKILL, "references", "generated", "components.md"),
    "utf8",
  );
  const markers = [...components.matchAll(/<!-- component:([a-z0-9-]+) -->/g)]
    .map((match) => match[1]);

  assert.deepEqual(
    markers.toSorted(),
    catalog.components.map(({ slug }) => slug).toSorted(),
  );
  assert.equal(new Set(markers).size, markers.length, "a component appears more than once");
  for (const component of catalog.components) {
    assert.ok(
      components.includes("### `" + component.api + "` "),
      `${component.slug} is missing its API heading`,
    );
    assert.ok(
      components.includes(`\`${component.source}\``),
      `${component.slug} is missing its source module`,
    );
  }
});
