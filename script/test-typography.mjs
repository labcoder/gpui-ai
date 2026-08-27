// Native DirectWrite glyph coverage. Mock font metrics cannot catch clipping.
import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import themes from "../site/generated/themes.json" with { type: "json" };

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
if (process.platform !== "win32") {
  throw new Error("The typography raster gate currently requires Windows/DirectWrite and a GPU.");
}
function run(command, args) {
  const result = spawnSync(command, args, { cwd: root, stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} ${args.join(" ")} failed (${result.status ?? result.signal})`);
}
run("cargo", ["build", "-p", "gallery", "--example", "button_typography", "--features", "gpui_platform/test-support"]);
const executable = path.join(root, "target/debug/examples/button_typography.exe");
for (const theme of themes.groups.flatMap((group) => group.themes)) {
  for (const size of ["medium", "small"]) run(executable, [theme.slug, "default", size]);
  console.log(`glyphs: ${theme.slug} (medium, small)`);
}
for (const rem of [16, 17, 24, 32]) {
  for (const size of ["medium", "small"]) run(executable, ["sunday-panel", String(rem), size]);
  console.log(`glyphs: sunday-panel at ${rem}px rem (medium, small)`);
}
