import { spawn, execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import { availableParallelism, release, totalmem } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { installBrowser } from "./web-browser.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const testsDir = path.join(root, "site/test/release");

export function parseOptions(args) {
  const options = { browserOnly: false, systemBrowser: false, repeat: 1 };
  for (let ix = 0; ix < args.length; ix += 1) {
    const arg = args[ix];
    if (arg === "--browser-only") options.browserOnly = true;
    else if (arg === "--system-browser") options.systemBrowser = true;
    else if (arg === "--suite") {
      options.suite = args[++ix];
      if (!/^[a-z-]+$/.test(options.suite ?? "")) throw new Error("--suite needs a suite name");
    } else if (arg === "--repeat") {
      options.repeat = Number(args[++ix]);
      if (!Number.isInteger(options.repeat) || options.repeat < 1 || options.repeat > 20) throw new Error("--repeat must be 1..20");
    } else throw new Error(`Unknown option: ${arg}`);
  }
  return options;
}

export function releaseTests(files, suite) {
  const tests = files.filter((file) => file.endsWith(".test.mjs") && (!suite || file === `${suite}.test.mjs`)).sort();
  if (!tests.length) throw new Error(`No release tests found${suite ? ` for ${suite}` : ""}`);
  return tests;
}

function run(command, args, environment = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { cwd: root, env: { ...process.env, ...environment }, stdio: "inherit" });
    child.once("error", reject);
    child.once("exit", (code, signal) => code === 0 ? resolve() : reject(new Error(`${command} ${args.join(" ")} failed with ${signal ?? `exit code ${code}`}`)));
  });
}

export async function runWebTests(args = process.argv.slice(2)) {
  const options = parseOptions(args);
  const tests = releaseTests(await readdir(testsDir), options.suite);
  // Resolve the browser before a long build. A missing browser is a setup
  // failure, never a skipped-green release gate. No package dependency needed.
  const browser = options.systemBrowser ? undefined : await installBrowser();
  const environment = {
    ...(browser ? { CHROME_PATH: browser.executable } : {}),
    GPUI_AI_WEB_SYSTEM_BROWSER: options.systemBrowser ? "1" : "",
    // Chrome's Vulkan SwiftShader adapter is available on Linux. Windows and
    // macOS exercise their actual WebGPU adapter; use Linux/WSL for CI parity.
    GPUI_AI_WEB_GPU: process.env.GPUI_AI_WEB_GPU ?? (process.platform === "linux" ? "software" : "default"),
    GPUI_AI_WEB_CHROME_VERSION: browser?.version ?? "",
  };
  if (!options.browserOnly) {
    const npm = process.env.npm_execpath;
    if (!npm) throw new Error("Run this gate through npm run check:web:release");
    await run(process.execPath, [npm, "run", "build:wasm"]);
    await run(process.execPath, [npm, "--prefix", "crates/gallery-web/www", "run", "build"]);
  }
  const assets = path.join(root, "crates/gallery-web/www/dist/assets");
  const hashes = {};
  for (const file of (await readdir(assets)).filter((file) => /\.(wasm|js)$/.test(file)).sort()) {
    hashes[file] = createHash("sha256").update(await readFile(path.join(assets, file))).digest("hex");
  }
  if (!Object.keys(hashes).some((file) => file.endsWith(".wasm"))) throw new Error("No built WASM artifact; run npm run check:web:release");
  const evidenceRoot = process.env.GPUI_AI_WEB_EVIDENCE_ROOT ?? path.join(root, "target/web-evidence");
  const evidence = path.resolve(evidenceRoot, new Date().toISOString().replaceAll(/[:.]/g, "-"));
  await mkdir(evidence, { recursive: true });
  await writeFile(path.join(evidence, "manifest.json"), JSON.stringify({
    commit: execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).trim(),
    dirty: execFileSync("git", ["status", "--porcelain"], { cwd: root, encoding: "utf8" }).trim(),
    node: process.version, platform: process.platform, browser: browser?.version ?? "system", gpu: environment.GPUI_AI_WEB_GPU,
    osRelease: release(), arch: process.arch, cpus: availableParallelism(), memoryBytes: totalmem(),
    browserOnly: options.browserOnly, tests, repeat: options.repeat, hashes,
  }, null, 2));
  console.log(`Web evidence: ${evidence}`);
  for (let iteration = 1; iteration <= options.repeat; iteration += 1) {
    const artifacts = path.join(evidence, `run-${iteration}`);
    await mkdir(artifacts);
    console.log(`Browser pass ${iteration}/${options.repeat}: ${tests.join(", ")} (${environment.GPUI_AI_WEB_GPU})`);
    // Discover files, not test-name patterns. New suites cannot silently miss
    // CI, and repeated runs must all pass: this is stress testing, not retries.
    await run(process.execPath, [
      "--test", "--test-concurrency=1", "--test-reporter=spec", "--test-reporter-destination=stdout",
      "--test-reporter=junit", `--test-reporter-destination=${path.join(artifacts, "results.xml")}`,
      ...tests.map((file) => path.join(testsDir, file)),
    ], { ...environment, GPUI_AI_WEB_ARTIFACTS: artifacts });
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await runWebTests();
