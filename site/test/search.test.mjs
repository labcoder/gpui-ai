import assert from "node:assert/strict";
import { test } from "node:test";

import { buildIndex, search } from "../app/search.mjs";
import catalog from "../generated/catalog.json" with { type: "json" };

const index = buildIndex(catalog.components);
const found = (query) => search(index, query).map((component) => component.slug);

test("an empty query is every component, in catalog order", () => {
  assert.deepEqual(
    found("   "),
    catalog.components.map((component) => component.slug),
  );
});

test("a component's own name puts it first", () => {
  for (const component of catalog.components) {
    const results = found(component.title);
    assert.equal(
      results[0],
      component.slug,
      `searching "${component.title}" put ${results[0]} above ${component.slug}`,
    );
  }
});

test("a type name puts its component first", () => {
  for (const component of catalog.components) {
    const results = found(component.api);
    assert.equal(
      results[0],
      component.slug,
      `searching "${component.api}" put ${results[0]} above ${component.slug}`,
    );
  }
});

test("a prefix of a word in the name finds it", () => {
  // The whole reason this is not a substring match over one joined string:
  // "approve" appears nowhere in "Approval card", and a reader looking for the
  // approval component types the verb.
  assert.ok(found("approv").includes("approval"), "approv finds the approval card");
  assert.ok(found("stream").includes("streaming-text"), "stream finds streaming text");
  assert.ok(found("recommend").includes("recommendation"), "recommend finds the recommendation card");
});

test("an event name finds the component that emits it", () => {
  const emitters = catalog.components.filter((component) => component.events.length > 0);
  assert.ok(emitters.length > 0, "the catalog must have components with events to search");

  for (const component of emitters) {
    for (const event of component.events) {
      assert.ok(
        found(event).includes(component.slug),
        `searching "${event}" does not find ${component.slug}`,
      );
    }
  }
});

/** A component-shaped record, for claims about the ranking itself. */
function made(slug, over = {}) {
  return {
    sequence: 1,
    slug,
    title: slug,
    compactLabel: slug,
    category: "Made up",
    summary: "",
    api: slug,
    usage: "",
    events: [],
    behavior: {},
    ...over,
  };
}

test("a hit at the start of a word beats one buried inside it", () => {
  // Someone typing a prefix means the start of a word. Without this, a
  // component whose name merely contains the letters ranks with one whose name
  // begins with them, and the list stops being an answer.
  const starts = made("cursor", { sequence: 2 });
  const buried = made("precursor", { sequence: 1 });

  assert.deepEqual(
    search(buildIndex([buried, starts]), "cursor").map((component) => component.slug),
    ["cursor", "precursor"],
    "a word-start hit must outrank a mid-word one even from later in the catalog",
  );
});

test("a field's weight decides which component wins, not which field was searched", () => {
  // The same word, in the type name of one component and the prose of another.
  // The scores exist to say that the first of those means far more.
  const named = made("widget", { sequence: 9, api: "Widget" });
  const mentioned = made("other", { sequence: 1, behavior: { ownership: "Sits beside a widget." } });

  assert.deepEqual(
    search(buildIndex([mentioned, named]), "widget").map((component) => component.slug),
    ["widget", "other"],
  );
});

test("a second word narrows the result rather than widening it", () => {
  const one = found("table");
  const two = found("table filter");

  assert.ok(one.length > 1, "table alone must match more than one component");
  assert.ok(two.length < one.length, "adding a word must not add results");
  assert.ok(
    two.every((slug) => one.includes(slug)),
    "every result for two words must have matched the first",
  );
  assert.equal(two[0], "filter-table");
});

test("a query that matches nothing returns nothing", () => {
  assert.deepEqual(found("zzzzz"), []);
  // One real word and one nonsense word is still nonsense: every term counts.
  assert.deepEqual(found("chat zzzzz"), []);
});

test("in the real catalog, a name beats a passing mention", () => {
  // The claim above, against the data the site actually ships. Approval card
  // is the answer to "approv"; Tool calls and Plan card only talk about
  // approving, and belong below it rather than in front of it.
  const results = found("approv");
  assert.equal(results[0], "approval");
  assert.ok(results.length > 1, "other components' prose mentions approving");

  // Same shape, different fields: Search results is named for it, Command
  // search contains it, and Thread list only describes it.
  assert.deepEqual(found("search"), ["search", "command-search", "thread-list"]);
});
