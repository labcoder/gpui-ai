// Generate into an isolated root; never repair a stale checkout during a test.
import { spawnSync } from "node:child_process";
import { mkdtemp, readFile, readdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const ROOT = fileURLToPath(new URL("..", import.meta.url));
const OUTPUTS = ["site/generated", "site/public/themes"];

async function files(directory, prefix = "") {
  const found = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const name = path.posix.join(prefix, entry.name);
    if (entry.isDirectory()) found.push(...await files(path.join(directory, entry.name), name));
    else found.push(name);
  }
  return found.sort();
}

/** All files and bytes, including unexpected/stale outputs. No writes. */
export async function compareGenerated(expectedRoot, actualRoot) {
  const differences = [];
  for (const directory of OUTPUTS) {
    const expectedDir = path.join(expectedRoot, directory);
    const actualDir = path.join(actualRoot, directory);
    const expected = await files(expectedDir);
    let actual;
    try { actual = await files(actualDir); }
    catch (error) { if (error.code !== "ENOENT") throw error; actual = []; }
    for (const name of new Set([...expected, ...actual])) {
      const label = path.posix.join(directory, name);
      if (!expected.includes(name)) differences.push(`unexpected: ${label}`);
      else if (!actual.includes(name)) differences.push(`missing: ${label}`);
      else if (!(await readFile(path.join(expectedDir, name))).equals(
        await readFile(path.join(actualDir, name)),
      )) differences.push(`changed: ${label}`);
    }
  }
  return differences.sort();
}

/** Real generators, with their intermediate reads/writes in the same new root. */
export function generateInto(outputRoot) {
  const env = { ...process.env, GPUI_AI_OUTPUT_ROOT: outputRoot };
  delete env.GPUI_AI_SOURCE_ROOT;
  for (const [command, args] of [
    ["cargo", ["run", "-q", "-p", "gallery", "--features", "export", "--bin", "gallery-catalog-export"]],
    [process.execPath, ["script/generate-themes.mjs"]],
    [process.execPath, ["script/generate-build-info.mjs"]],
    [process.execPath, ["site/scripts/generate-highlight.mjs"]],
  ]) {
    const result = spawnSync(command, args, { cwd: ROOT, env, stdio: "inherit" });
    if (result.error) throw result.error;
    if (result.status !== 0) throw new Error(`${command} ${args.join(" ")} failed (${result.status})`);
  }
}

export async function checkGenerated(actualRoot = ROOT) {
  const temporaryRoot = await mkdtemp(path.join(tmpdir(), "gpui-ai-generated-"));
  try {
    generateInto(temporaryRoot);
    const differences = await compareGenerated(temporaryRoot, actualRoot);
    if (differences.length) throw new Error(
      `Generated artifacts are stale; run npm run generate:\n${differences.join("\n")}`,
    );
    console.log("All generated file sets and bytes are current; checkout untouched.");
  } finally {
    await rm(temporaryRoot, { recursive: true, force: true });
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  await checkGenerated();
}
