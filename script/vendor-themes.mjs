// Vendor gpui-component's bundled theme pack into themes/upstream.
//
//   node script/vendor-themes.mjs
//
// Run by script/update-upstream.sh so the vendored set always matches the
// revision Cargo.lock pins. Unlike the icons, these files are tracked: they are
// part of the theme picker the site ships, and a reviewer should see when an
// upstream bump changes a palette.

import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

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

// The manifest lives at <checkout>/crates/ui/Cargo.toml; the theme pack sits at
// the checkout root.
const checkout = resolve(dirname(component.manifest_path), "..", "..");
const source = join(checkout, "themes");

let files;
try {
  files = readdirSync(source).filter((name) => name.endsWith(".json")).sort();
} catch (error) {
  fail(`could not read upstream themes at ${source}: ${error.message}`);
}
if (files.length === 0) fail(`no theme JSON found in ${source}`);

rmSync(DESTINATION, { recursive: true, force: true });
mkdirSync(DESTINATION, { recursive: true });

let themes = 0;
for (const file of files) {
  copyFileSync(join(source, file), join(DESTINATION, file));
  const parsed = JSON.parse(readFileSync(join(source, file), "utf8"));
  themes += (parsed.themes ?? []).length;
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

Source: https://github.com/longbridge/gpui-component (themes/)
License: Apache-2.0 (see the upstream LICENSE-APACHE)
Revision: pinned by this repository's Cargo.lock
Regenerate: node script/vendor-themes.mjs (run by script/update-upstream.sh)

Do not edit these by hand — a regeneration overwrites them. gpui-ai's own
themes live in ../gpui-ai.
`,
);

process.stdout.write(`vendored ${files.length} upstream theme files holding ${themes} themes\n`);
