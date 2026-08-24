import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFile, readdir } from "node:fs/promises";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const generated = path.join(repositoryRoot, "site", "generated");

const downloads = path.join(repositoryRoot, "site", "public", "themes");

const readOutputs = async () => ({
  json: await readFile(path.join(generated, "themes.json"), "utf8"),
  css: await readFile(path.join(generated, "themes.css"), "utf8"),
  files: (await readdir(downloads)).sort().join(" "),
});

test("the generated theme data is current and regenerating is idempotent", async () => {
  const committed = await readOutputs();

  const run = spawnSync(process.execPath, ["script/generate-themes.mjs"], {
    cwd: repositoryRoot,
    encoding: "utf8",
  });
  assert.equal(run.status, 0, `the generator failed: ${run.stderr}`);

  const regenerated = await readOutputs();
  assert.equal(
    regenerated.json,
    committed.json,
    "site/generated/themes.json is stale — run npm run generate and commit the result",
  );
  assert.equal(
    regenerated.css,
    committed.css,
    "site/generated/themes.css is stale — run npm run generate and commit the result",
  );
  assert.equal(
    regenerated.files,
    committed.files,
    "site/public/themes is stale — run npm run generate and commit the result",
  );
});

test("every theme can be downloaded as the file the registry would read", async () => {
  const { groups } = JSON.parse(await readFile(path.join(generated, "themes.json"), "utf8"));
  const all = groups.flatMap((group) => group.themes);
  const files = (await readdir(downloads)).filter((name) => name.endsWith(".json"));

  assert.equal(
    files.length,
    all.length,
    "the picker and the downloads describe different sets of themes",
  );

  for (const theme of all) {
    const pack = JSON.parse(await readFile(path.join(downloads, `${theme.slug}.json`), "utf8"));

    // A pack, not a palette. Someone downloading this wants to drop it into
    // themes/ and have the registry read it — a reconstruction from the
    // derived --ai-* values would look right and be a different theme.
    assert.equal(pack.name, theme.registryName, `${theme.slug} was renamed on the way out`);
    assert.equal(pack.themes.length, 1, `${theme.slug} should download on its own`);
    assert.equal(pack.themes[0].name, theme.registryName);
    assert.equal(pack.themes[0].mode ?? "light", theme.mode);
    assert.ok(
      Object.keys(pack.themes[0].colors ?? {}).length > 3,
      `${theme.slug} downloads with no colours`,
    );
  }
});

test("every theme carries the full token set the chrome is painted from", async () => {
  const { groups } = JSON.parse(await readFile(path.join(generated, "themes.json"), "utf8"));
  const required = [
    "--ai-background",
    "--ai-foreground",
    "--ai-muted",
    "--ai-border",
    "--ai-surface",
    "--ai-primary",
    "--ai-primary-foreground",
    "--ai-accent",
    "--ai-danger",
    "--ai-success",
    "--ai-warning",
    "--ai-info",
    "--ai-radius",
    "--ai-radius-lg",
    "--ai-font-size",
    "--ai-font-sans",
    "--ai-font-mono",
    "--ai-shadow",
    // Derived rather than read out of the theme file: a status colour is a
    // fill, and the code panel needs text that can be read on its surface.
    "--ai-code-comment",
    "--ai-code-keyword",
    "--ai-code-string",
    "--ai-code-type",
    "--ai-code-number",
  ];

  const slugs = [];
  for (const group of groups) {
    for (const theme of group.themes) {
      slugs.push(theme.slug);
      assert.match(theme.slug, /^[a-z0-9][a-z0-9-]*$/, `${theme.slug} is not a usable slug`);
      assert.match(theme.label, /\S/);
      assert.ok(["light", "dark"].includes(theme.mode), `${theme.slug} has no mode`);
      assert.equal(typeof theme.radius, "number");
      assert.equal(typeof theme.fontSize, "number");
      for (const token of required) {
        assert.match(
          theme.tokens[token] ?? "",
          /\S/,
          `${theme.slug} is missing ${token}`,
        );
      }
      // Colour tokens must be resolved values, never an unresolved palette name.
      for (const token of required.filter((name) => !/radius|font|shadow/.test(name))) {
        assert.match(
          theme.tokens[token],
          /^(#|rgb|hsl|oklch)/i,
          `${theme.slug} left ${token} as "${theme.tokens[token]}"`,
        );
      }
    }
  }

  assert.equal(new Set(slugs).size, slugs.length, "theme slugs must be unique");
  assert.ok(slugs.includes("light") && slugs.includes("dark"), "the basic pair must be present");
});

test("the upstream group is credited separately from gpui-ai's own themes", async () => {
  const { groups } = JSON.parse(await readFile(path.join(generated, "themes.json"), "utf8"));
  assert.deepEqual(
    groups.map((group) => group.id),
    ["gpui-ai", "gpui-component"],
    "the picker groups gpui-ai's presets first",
  );

  const upstream = groups[1];
  assert.equal(upstream.license, "Apache-2.0");
  assert.match(upstream.source, /github\.com\/longbridge\/gpui-component/);
  assert.ok(upstream.themes.length > 20, "the vendored pack should be substantial");
});

test("the stylesheet gives light the default selector and every theme a data-theme rule", async () => {
  const [{ themes: ours }, { themes: upstream }] = JSON.parse(
    await readFile(path.join(generated, "themes.json"), "utf8"),
  ).groups;
  const css = await readFile(path.join(generated, "themes.css"), "utf8");

  assert.match(css, /^:root,\n\[data-theme="light"\] \{/m, "light must paint the default chrome");
  for (const theme of [...ours, ...upstream]) {
    if (theme.slug === "light") continue;
    assert.ok(
      css.includes(`[data-theme="${theme.slug}"] {`),
      `themes.css has no rule for ${theme.slug}`,
    );
  }
  assert.ok(css.includes("color-scheme: dark"), "dark themes must set color-scheme");
});
