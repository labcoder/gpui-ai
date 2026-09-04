// Copy gpui-component's icon set into the web host so the browser gallery
// serves it from its own origin.
//
//   node crates/gallery-web/scripts/copy-icons.mjs
//
// Upstream's WASM asset source downloads icons on demand from
// `<endpoint>/assets/icons/<name>.svg`. The pages set `data-asset-base` to
// `./upstream`, so the files have to land under `public/upstream/assets/icons`.
// They are build output, not sources: the directory is git-ignored and this
// script reproduces it from whatever version Cargo.lock pins.

import { spawnSync } from "node:child_process";
import { cpSync, mkdirSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = fileURLToPath(new URL("../../..", import.meta.url));
const DESTINATION = join(ROOT, "crates", "gallery-web", "www", "public", "upstream", "assets", "icons");

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
  fail(`cargo metadata failed: ${metadata.stderr?.trim() || metadata.error?.message || "unknown error"}`);
}

// The package name, not the name this workspace imports it under: our manifest
// renames `gpui-kit-assets` to `gpui-component-assets`, and `cargo metadata`
// reports what upstream published.
const assetsPackage = JSON.parse(metadata.stdout).packages.find(
  (entry) => entry.name === "gpui-kit-assets",
);
if (!assetsPackage) fail("gpui-kit-assets is not in the dependency graph");

const source = join(dirname(assetsPackage.manifest_path), "assets", "icons");
let icons;
try {
  icons = readdirSync(source).filter((name) => name.endsWith(".svg"));
} catch (error) {
  fail(`could not read upstream icons at ${source}: ${error.message}`);
}
if (icons.length === 0) fail(`no SVG icons found in ${source}`);

rmSync(DESTINATION, { recursive: true, force: true });
mkdirSync(DESTINATION, { recursive: true });
for (const icon of icons) cpSync(join(source, icon), join(DESTINATION, icon));

writeFileSync(
  join(dirname(dirname(DESTINATION)), "NOTICE"),
  `These icons are copied verbatim from gpui-component, which is licensed under
Apache-2.0, so that the browser gallery serves them from its own origin instead
of fetching them from longbridge.github.io at runtime.

Source: https://github.com/longbridge/gpui-component (crates/assets/assets/icons)
Revision: pinned by this repository's Cargo.lock
Regenerate: node crates/gallery-web/scripts/copy-icons.mjs
`,
);

process.stdout.write(
  `copied ${icons.length} icons from ${assetsPackage.name} ${assetsPackage.version} into public/upstream/assets/icons\n`,
);
