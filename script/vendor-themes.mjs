// Vendor gpui-component's bundled theme pack into themes/upstream.
//
//   npm run vendor:themes
//
// Run by hand after a gpui-component bump. These files are tracked, unlike the
// icons: they are part of the theme picker the site ships, and a reviewer
// should see when a bump changes a palette.
//
// The pack is not in the published crate - only the two defaults below are - so
// it comes over the network from the tag matching the locked version. The
// lockfile still decides which themes we get; the tag is just where they live.

import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const REPOSITORY = "longbridge/gpui-kit";

const ROOT = fileURLToPath(new URL("..", import.meta.url));
const DESTINATION = join(ROOT, "themes", "upstream");

function fail(message) {
  process.stderr.write(`${message}\n`);
  process.exit(1);
}

const metadata = spawnSync("cargo", ["metadata", "--format-version", "1", "--locked"], {
  cwd: ROOT,
  encoding: "utf8",
  maxBuffer: 64 * 1024 * 1024,
});
if (metadata.status !== 0) {
  fail(`cargo metadata failed: ${metadata.stderr?.trim() || "unknown error"}`);
}

const component = JSON.parse(metadata.stdout).packages.find((entry) => entry.name === "gpui-component");
if (!component) fail("gpui-component is not in the dependency graph");

const tag = `v${component.version}`;

async function github(path) {
  const url = `https://api.github.com/repos/${REPOSITORY}/${path}`;
  const response = await fetch(url, { headers: { accept: "application/vnd.github+json" } });
  if (!response.ok) fail(`GET ${url} returned ${response.status} ${response.statusText}`);
  return response.json();
}

const listing = await github(`contents/themes?ref=${tag}`);
const entries = listing
  .filter((entry) => entry.type === "file" && entry.name.endsWith(".json"))
  .sort((left, right) => left.name.localeCompare(right.name));
if (entries.length === 0) fail(`no theme JSON at ${REPOSITORY}@${tag}`);

// Download before deleting: a half-vendored directory left by a dropped
// connection is worse than an unchanged one.
const downloaded = await Promise.all(
  entries.map(async (entry) => {
    const response = await fetch(entry.download_url);
    if (!response.ok) {
      fail(`GET ${entry.download_url} returned ${response.status} ${response.statusText}`);
    }
    const body = await response.text();
    try {
      JSON.parse(body);
    } catch (error) {
      fail(`${entry.name} at ${tag} is not JSON: ${error.message}`);
    }
    return { name: entry.name, body };
  }),
);

rmSync(DESTINATION, { recursive: true, force: true });
mkdirSync(DESTINATION, { recursive: true });

let themes = 0;
const files = downloaded.map((file) => file.name);
for (const file of downloaded) {
  writeFileSync(join(DESTINATION, file.name), file.body);
  themes += (JSON.parse(file.body).themes ?? []).length;
}

// gpui-component's own default Light and Dark live in the ui crate rather than
// the theme pack, and several packs name their colours from its palette
// ("blue-400") instead of writing hex. Both are needed to describe a theme
// outside Rust, so vendor them into a subdirectory: the gallery's build script
// only scans *.json directly under themes/, so these never become presets.
const defaults = join(DESTINATION, "defaults");
mkdirSync(defaults, { recursive: true });
const uiTheme = join(dirname(component.manifest_path), "src", "theme");
for (const file of ["default-theme.json", "default-colors.json"]) {
  try {
    copyFileSync(join(uiTheme, file), join(defaults, file));
  } catch (error) {
    fail(`could not vendor ${file} from ${uiTheme}: ${error.message}`);
  }
}

writeFileSync(
  join(DESTINATION, "NOTICE"),
  `The JSON files in this directory are copied verbatim from gpui-component,
which is licensed under Apache-2.0. They are vendored so the gallery and the
website can offer the upstream theme set without a network fetch, and so a
reviewer can see palette changes when the pinned revision moves.

Source: https://github.com/${REPOSITORY} (themes/ at ${tag})
License: Apache-2.0 (see the upstream LICENSE-APACHE)
Version: the gpui-component version this repository's Cargo.lock pins
Regenerate: npm run vendor:themes

Do not edit these by hand — a regeneration overwrites them. gpui-ai's own
themes live in ../gpui-ai.
`,
);

process.stdout.write(`vendored ${files.length} upstream theme files holding ${themes} themes\n`);
