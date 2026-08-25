// Build the publishable rustdoc tree for the public site.
//
//   npm run build:docs
//
// Emits `target/doc`, ready for the Pages workflow to copy to `<site>/api/`.
// Rustdoc emits only relative references, so the tree works under any base
// path. Cargo writes no root index for a single crate, so this adds one that
// redirects `/api/` to `/api/gpui_ai/`.

import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";
import { fileURLToPath } from "node:url";

import { stripDeadReadMore } from "./docs-cleanup.mjs";

const CRATE = "gpui_ai";
const ROOT = fileURLToPath(new URL("..", import.meta.url));
const OUTPUT = join(ROOT, "target", "doc");

const build = spawnSync("cargo", ["doc", "--no-deps", "-p", "gpui-ai"], {
  cwd: ROOT,
  stdio: "inherit",
});
if (build.error) {
  process.stderr.write(`${build.error.message}\n`);
  process.exit(1);
}
if (build.status !== 0) process.exit(build.status ?? 1);

writeFileSync(
  join(OUTPUT, "index.html"),
  `<!doctype html>
<meta charset="utf-8">
<title>gpui-ai API documentation</title>
<meta http-equiv="refresh" content="0; url=${CRATE}/index.html">
<link rel="canonical" href="${CRATE}/index.html">
<p><a href="${CRATE}/index.html">gpui-ai API documentation</a></p>
`,
);

/** Every `.html` file in the tree, deepest first order not being important. */
function pages(directory, found = []) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) pages(path, found);
    else if (entry.name.endsWith(".html")) found.push(path);
  }
  return found;
}

let deadLinks = 0;
let cleanedPages = 0;
for (const page of pages(OUTPUT)) {
  const { html, removed } = stripDeadReadMore(readFileSync(page, "utf8"));
  if (removed === 0) continue;
  writeFileSync(page, html);
  deadLinks += removed;
  cleanedPages += 1;
}

function measure(directory) {
  let files = 0;
  let bytes = 0;
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) {
      const nested = measure(path);
      files += nested.files;
      bytes += nested.bytes;
    } else {
      files += 1;
      bytes += statSync(path).size;
    }
  }
  return { files, bytes };
}

// Every component page links its own type here (S-16), deriving the path from
// the catalog. Source-level checks cannot prove that derivation: a type moved
// behind a private module and re-exported still reads as `pub struct` in the
// same file while rustdoc documents it somewhere else. This is the only place
// the artifact exists, so this is where the links get checked.
const catalog = JSON.parse(readFileSync(join(ROOT, "site", "generated", "catalog.json"), "utf8"));
const missing = catalog.components
  .map((component) => ({
    slug: component.slug,
    page: join(CRATE, basename(component.source, ".rs"), `struct.${component.api}.html`),
  }))
  .filter(({ page }) => !existsSync(join(OUTPUT, page)));

if (missing.length > 0) {
  process.stderr.write(
    `rustdoc has no page for ${missing.length} catalogued component${missing.length === 1 ? "" : "s"}, ` +
      `so the site would link a 404:\n` +
      missing.map(({ slug, page }) => `  ${slug} -> ${page}`).join("\n") +
      `\nUpdate site/app/links.ts, or re-export the type from its own module.\n`,
  );
  process.exit(1);
}

const { files, bytes } = measure(OUTPUT);
process.stdout.write(
  `rustdoc tree ready: ${files.toLocaleString()} files, ${(bytes / 1024 / 1024).toFixed(1)} MB\n` +
    `removed ${deadLinks.toLocaleString()} dead "Read more" links from ${cleanedPages} pages\n` +
    `every one of the ${catalog.components.length} component API links resolves\n` +
    `entry: target/doc/${CRATE}/index.html (root index.html redirects to it)\n`,
);
