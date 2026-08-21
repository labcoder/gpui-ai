import assert from "node:assert/strict";
import { test } from "node:test";

import {
  copyFeedback,
  hasWebGpu,
  normalizeTheme,
  persistTheme,
  readStoredTheme,
  specimenOverdrawMargin,
  specimenTransition,
  specimenUrl,
  withTheme,
} from "../src/runtime.js";

test("theme selection accepts three stable values and respects system fallback", () => {
  assert.equal(normalizeTheme("?theme=light", true), "light");
  assert.equal(normalizeTheme("?theme=dark", false), "dark");
  assert.equal(normalizeTheme("?theme=contrast", false), "contrast");
  assert.equal(normalizeTheme("?theme=neon", true), "dark");
  assert.equal(normalizeTheme("", false), "light");
  assert.equal(normalizeTheme("", false, "contrast"), "contrast");
  assert.equal(normalizeTheme("?theme=light", true, "contrast"), "light");
});

test("theme storage validates values and tolerates unavailable storage", () => {
  const values = new Map();
  const storage = {
    getItem: (key) => values.get(key),
    setItem: (key, value) => values.set(key, value),
  };
  assert.equal(persistTheme(storage, "contrast"), true);
  assert.equal(readStoredTheme(storage), "contrast");
  values.set("mighty-gpui-theme", "neon");
  assert.equal(readStoredTheme(storage), undefined);
  const blocked = { getItem: () => { throw new Error("blocked"); }, setItem: () => { throw new Error("blocked"); } };
  assert.equal(readStoredTheme(blocked), undefined);
  assert.equal(persistTheme(blocked, "dark"), false);
});

test("theme changes preserve the route and unrelated query parameters", () => {
  assert.equal(
    withTheme("https://example.test/components/chat/?ref=index&theme=light", "contrast"),
    "https://example.test/components/chat/?ref=index&theme=contrast",
  );
});

test("specimen URLs address one shared gallery with explicit story and theme", () => {
  assert.equal(
    specimenUrl("../../gallery/embed.html", "prompt-bar", "dark"),
    "../../gallery/embed.html?story=prompt-bar&theme=dark",
  );
});

test("WebGPU capability detection fails closed", () => {
  assert.equal(hasWebGpu({ gpu: {} }), true);
  assert.equal(hasWebGpu({}), false);
  assert.equal(hasWebGpu(undefined), false);
});

test("specimen lifecycle loads, unloads, and restores from retained data", () => {
  assert.equal(specimenOverdrawMargin, "400px 0px");
  assert.equal(specimenTransition("near", false), "load");
  assert.equal(specimenTransition("far", true), "unload");
  assert.equal(specimenTransition("far", false), "idle");
  assert.equal(specimenTransition("near", true), "idle");
});

test("copy feedback covers clipboard success and manual fallback", () => {
  assert.deepEqual(copyFeedback(true), { button: "Copied", status: "Rust example copied to the clipboard." });
  assert.deepEqual(copyFeedback(false), { button: "Copy", status: "Could not copy automatically. Select the code and copy it manually." });
});
