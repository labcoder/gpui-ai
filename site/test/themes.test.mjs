import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFile, readdir } from "node:fs/promises";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { DEFAULT } from "../app/theme-resolve.mjs";

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
    "--ai-muted-text",
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

/** WCAG relative luminance, from an `#rrggbb` string. */
function luminance(hex) {
  const match = /^#([0-9a-f]{6})$/i.exec(hex);
  assert.ok(match, `${hex} is not a six-digit hex colour`);
  const value = Number.parseInt(match[1], 16);
  const channels = [(value >> 16) & 255, (value >> 8) & 255, value & 255].map((c) => {
    const unit = c / 255;
    return unit <= 0.03928 ? unit / 12.92 : ((unit + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2];
}

function contrast(a, b) {
  const [hi, lo] = luminance(a) >= luminance(b) ? [a, b] : [b, a];
  return (luminance(hi) + 0.05) / (luminance(lo) + 0.05);
}

test("secondary text is readable on the page and on a card, in every theme", async () => {
  const { groups } = JSON.parse(await readFile(path.join(generated, "themes.json"), "utf8"));

  // The site paints paragraphs, captions, and the navigation links in this
  // colour. Read straight from the registry, `--ai-muted` put 26 of the 45
  // themes below AA and nine of them below 3:1, which is why `--ai-muted-text`
  // is derived from it rather than being it.
  const failures = [];
  let nudged = 0;
  for (const group of groups) {
    for (const theme of group.themes) {
      const { tokens } = theme;
      const text = tokens["--ai-muted-text"];
      if (text.toLowerCase() !== tokens["--ai-muted"].toLowerCase()) nudged += 1;
      for (const ground of ["--ai-background", "--ai-surface"]) {
        const ratio = contrast(text, tokens[ground]);
        if (ratio < 4.5) {
          failures.push(`${theme.slug}: ${text} on ${ground} is ${ratio.toFixed(2)}:1`);
        }
      }
    }
  }

  assert.deepEqual(failures, [], `secondary text fails AA in ${failures.length} places`);
  // A theme that already cleared AA is left exactly as its author wrote it, so
  // this must never become "all of them".
  assert.ok(nudged > 0 && nudged < 45, `${nudged} themes were altered, which is not plausible`);
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

test("the stylesheet gives the default theme :root and every theme a data-theme rule", async () => {
  const [{ themes: ours }, { themes: upstream }] = JSON.parse(
    await readFile(path.join(generated, "themes.json"), "utf8"),
  ).groups;
  const css = await readFile(path.join(generated, "themes.css"), "utf8");

  // A visitor with JavaScript disabled never runs the inline script, so
  // whatever :root paints is what they see — it has to be the theme the rest
  // of the page says is current.
  const root = css.indexOf(":root {");
  assert.ok(root >= 0, "no rule paints the default chrome");
  assert.equal(css.match(/^:root \{/gm)?.length, 1, "exactly one rule may claim :root");
  const defaultTokens = [...ours, ...upstream].find((theme) => theme.slug === DEFAULT)?.tokens;
  assert.ok(defaultTokens, `the registry does not ship ${DEFAULT}`);
  const rootBlock = css.slice(root, css.indexOf("}", root));
  assert.ok(
    rootBlock.includes(`--ai-background: ${defaultTokens["--ai-background"]};`),
    `:root must paint ${DEFAULT}, not something else`,
  );
  // :root and [data-theme="…"] have the same specificity, so a base rule
  // written after any theme would override it. This is the ordering that makes
  // every theme rule win.
  assert.ok(
    root < css.indexOf('[data-theme="'),
    ":root must come before every theme rule, or it overrides the ones above it",
  );
  for (const theme of [...ours, ...upstream]) {
    assert.ok(
      css.includes(`[data-theme="${theme.slug}"] {`),
      `themes.css has no rule for ${theme.slug}`,
    );
  }
  assert.ok(css.includes("color-scheme: dark"), "dark themes must set color-scheme");
});
