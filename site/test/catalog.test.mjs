import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";

import { components } from "../src/catalog.js";

const repositoryRoot = new URL("../../", import.meta.url);

test("catalog mirrors every stable Rust story exactly once", async () => {
  const storySource = await readFile(
    new URL("crates/gallery/src/story.rs", repositoryRoot),
    "utf8",
  );
  const rustSlugs = [...storySource.matchAll(/Self::\w+ => "([a-z0-9-]+)"/g)]
    .map((match) => match[1])
    .filter((slug) => slug !== "all");
  const siteSlugs = components.map(({ slug }) => slug);

  assert.equal(components.length, 24);
  assert.deepEqual(siteSlugs, rustSlugs);
  assert.equal(new Set(siteSlugs).size, siteSlugs.length);
});

test("every component has useful static documentation and a real source file", async () => {
  for (const component of components) {
    assert.equal(Number.isInteger(component.sequence), true);
    assert.match(component.compactLabel, /\S/);
    assert.match(component.viewport, /^(tall|wide)$/);
    assert.match(component.limitation, /\S.*\S/);
    assert.match(component.title, /\S/);
    assert.match(component.category, /\S/);
    assert.match(component.summary, /\S.*\S/);
    assert.match(component.usage, new RegExp(`${component.api}::new`));
    assert.doesNotMatch(component.usage, /\bpx\(/, "snippets import only the prelude");
    assert.match(component.source, /^crates\/mighty-gpui\/src\/[a-z_]+\.rs$/);
    const source = await readFile(new URL(component.source, repositoryRoot), "utf8");
    assert.match(source, new RegExp(`pub struct ${component.api}\\b`));
    assert.match(
      source,
      new RegExp(`impl ${component.api} \\{[\\s\\S]*?pub fn new(?:<[^>]+>)?\\(`),
    );
  }
});
