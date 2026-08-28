import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { cp, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

const repository = fileURLToPath(new URL("../../", import.meta.url));
async function fixture(t) {
  const root = await mkdtemp(path.join(tmpdir(), "gpui-ai-theme-test-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  await mkdir(path.join(root, "themes/gpui-ai"), { recursive: true });
  await cp(path.join(repository, "themes/upstream/defaults"), path.join(root, "themes/upstream/defaults"), { recursive: true });
  const run = () => spawnSync(process.execPath, [path.join(repository, "script/generate-themes.mjs")], {
    encoding: "utf8", env: { ...process.env, GPUI_AI_SOURCE_ROOT: root, GPUI_AI_OUTPUT_ROOT: root },
  });
  return { root, run };
}

test("the real generator rejects independently broken authored palettes", async (t) => {
  const { root, run } = await fixture(t);
  const good = JSON.parse(await readFile(path.join(repository, "themes/gpui-ai/contrast.json"), "utf8"));
  const file = path.join(root, "themes/gpui-ai/contrast.json");
  await writeFile(file, JSON.stringify(good));
  assert.equal(run().status, 0, "the fixture starts from a valid palette");
  const before = await readFile(path.join(root, "site/generated/themes.json"));
  for (const [name, breakPalette, message] of [
    ["missing color", c => delete c.foreground, /required color "foreground"/],
    ["unknown reference", c => c.foreground = "nonexistent-500", /required color "foreground"/],
    ["chart spelling", c => { c.chart_1 = c["chart.1"]; delete c["chart.1"]; }, /required color "chart.1"/],
    ["unreadable text", c => c.foreground = c.background, /"foreground" on "background" is 1.00:1/],
  ]) {
    const broken = structuredClone(good);
    breakPalette(broken.themes[0].colors);
    await writeFile(file, JSON.stringify(broken));
    const result = run();
    assert.notEqual(result.status, 0, name);
    assert.match(result.stderr, message, name);
    assert.deepEqual(await readFile(path.join(root, "site/generated/themes.json")), before,
      "validation fails before publishing any new theme output");
  }
});

test("derivation preserves readable source colors and repairs unreadable muted and accent text", async (t) => {
  const { root, run } = await fixture(t);
  const themes = ["good", "bad"].map(name => ({
    name, mode: "light", colors: {
      background: "#ffffff", foreground: "#000000",
      "popover.background": "#ffffff",
      "accent.background": name === "good" ? "#ffffff" : "#000000",
      "muted.foreground": name === "good" ? "#000000" : "#bbbbbb",
    },
  }));
  await writeFile(path.join(root, "themes/upstream/fixtures.json"), JSON.stringify({ themes }));
  const result = run();
  assert.equal(result.status, 0, result.stderr);
  const generated = JSON.parse(await readFile(path.join(root, "site/generated/themes.json"), "utf8"));
  const [good, bad] = generated.groups.find(group => group.id === "gpui-component").themes;
  assert.equal(good.tokens["--ai-muted-text"], "#000000");
  assert.equal(good.tokens["--ai-accent-text"], "#000000");
  for (const [token, original, background] of [
    ["--ai-muted-text", "#bbbbbb", 1],
    ["--ai-accent-text", "#000000", 0],
  ]) {
    const repaired = bad.tokens[token];
    assert.notEqual(repaired, original, token);
    // Independent WCAG computation against known white/black, not the validator.
    const channels = repaired.slice(1).match(/../g).map(hex => parseInt(hex, 16) / 255);
    const linear = channels.map(c => c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
    const luminance = linear[0] * 0.2126 + linear[1] * 0.7152 + linear[2] * 0.0722;
    const ratio = (Math.max(background, luminance) + 0.05) / (Math.min(background, luminance) + 0.05);
    assert.ok(ratio >= 4.5, `${token}: ${repaired} is ${ratio}:1`);
  }
});
