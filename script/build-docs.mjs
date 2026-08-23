// Build the publishable rustdoc tree for the public site.
//
//   npm run build:docs
//
// Emits `target/doc`, ready for the Pages workflow to copy to `<site>/api/`.
// Rustdoc emits only relative references, so the tree works under any base
// path. Cargo writes no root index for a single crate, so this adds one that
// redirects `/api/` to `/api/gpui_ai/`.

import { spawnSync } from "node:child_process";
import { readdirSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

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

const { files, bytes } = measure(OUTPUT);
process.stdout.write(
  `rustdoc tree ready: ${files.toLocaleString()} files, ${(bytes / 1024 / 1024).toFixed(1)} MB\n` +
    `entry: target/doc/${CRATE}/index.html (root index.html redirects to it)\n`,
);
