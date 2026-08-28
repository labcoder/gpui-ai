import assert from "node:assert/strict";
import { test } from "node:test";
import { createServer, get } from "node:http";
import { EventEmitter } from "node:events";
import { execFileSync } from "node:child_process";
import { mkdtemp, readFile, readdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { browserFlags, Cdp, closeBrowser, closeServer } from "../scripts/cdp.mjs";
import { observeBrowser, saveBrowserEvidence, unexpectedBrowserEvents } from "../scripts/browser-evidence.mjs";
import { browserBuild } from "../../script/web-browser.mjs";
import { parseOptions, releaseTests } from "../../script/run-web-tests.mjs";

test("the release runner discovers every suite without matching human test names", () => {
  assert.deepEqual(releaseTests(["new.test.mjs", "helper.mjs", "mobile.test.mjs"]), ["mobile.test.mjs", "new.test.mjs"]);
  assert.throws(() => releaseTests(["mobile.test.mjs"], "moblie"), /No release tests/);
  assert.throws(() => releaseTests([]), /No release tests/);
  assert.deepEqual(parseOptions(["--browser-only", "--repeat", "3", "--suite", "mobile"]), { browserOnly: true, systemBrowser: false, repeat: 3, suite: "mobile" });
  for (const args of [["--repeat", "0"], ["--repeat", "NaN"], ["--suite"], ["--retries", "3"]]) assert.throws(() => parseOptions(args));
});

test("software GPU and density are explicit; the browser sandbox stays enabled", () => {
  const flags = browserFlags({ gpu: "software", deviceScaleFactor: 3 });
  assert.ok(flags.includes("--use-webgpu-adapter=swiftshader"));
  assert.ok(flags.includes("--use-vulkan=swiftshader"));
  assert.ok(flags.includes("--force-device-scale-factor=3"));
  assert.ok(!flags.includes("--no-sandbox"));
  assert.ok(!browserFlags({ gpu: "software", platform: "linux" }).includes("--headless=new"), "Linux must present WebGPU through Xvfb, not its headless compositor");
  assert.deepEqual(browserFlags({ gpu: "default" }), ["--headless=new"]);
  assert.throws(() => browserFlags({ gpu: "unknown" }));
  assert.throws(() => browserFlags({ deviceScaleFactor: 0 }));
  for (const [platform, arch] of [["win32", "x64"], ["linux", "x64"], ["darwin", "arm64"]]) {
    const build = browserBuild(platform, arch);
    assert.match(build.url, new RegExp(build.version.replaceAll(".", "\\.")));
    assert.ok(build.executable.startsWith(build.directory));
  }
});

test("system-browser diagnostics do not silently reuse the pinned browser", () => {
  assert.equal(parseOptions(["--system-browser"]).systemBrowser, true);
  const environment = { ...process.env, GPUI_AI_WEB_SYSTEM_BROWSER: "1" };
  delete environment.CHROME_PATH;
  const moduleUrl = new URL("../scripts/cdp.mjs", import.meta.url).href;
  const candidates = JSON.parse(execFileSync(process.execPath, ["--input-type=module", "-e",
    `import { browserCandidates } from ${JSON.stringify(moduleUrl)}; console.log(JSON.stringify(browserCandidates));`,
  ], { env: environment, encoding: "utf8" }));
  assert.ok(!candidates.includes(browserBuild().executable), "the system profile must bypass the cached Chrome for Testing");
});

test("event collection keeps expected pending icons but never hides real errors or overflow", async () => {
  const listeners = new Map();
  const events = await observeBrowser({ cdp: { on: (name, fn) => listeners.set(name, fn), send: async () => {} } });
  const log = (type, value) => listeners.get("Runtime.consoleAPICalled")({ type, args: [{ value }] });
  log("error", "[ERROR] : Wasm assets loading, will be available soon...");
  log("error", "[ERROR] : Wasm assets loading, will be available soon...");
  assert.equal(events[0].kind, "asset-pending");
  assert.equal(events[0].count, 2);
  assert.deepEqual(unexpectedBrowserEvents(events), []);
  // This is the exact diagnostic emitted by the Linux CI WASM build.
  log("error", "[ERROR] gpui::elements::svg: Wasm assets loading, will be available soon...");
  assert.deepEqual(unexpectedBrowserEvents(events), [], "pending SVG loads are not asset failures");
  log("error", "[ERROR] gpui::elements::svg: Failed to load icon.svg");
  log("error", "[ERROR] app: Wasm assets loading, will be available soon...");
  listeners.get("Network.responseReceived")({ response: { status: 404, url: "http://fixture/icon.svg" } });
  listeners.get("Runtime.exceptionThrown")({ exceptionDetails: { text: "Uncaught", exception: { description: "Rust panic" } } });
  assert.deepEqual(unexpectedBrowserEvents(events).map(({ kind }) => kind), ["error", "error", "http", "exception"]);
  for (let ix = 0; ix < 110; ix += 1) log("warning", `warning-${ix}`);
  assert.equal(events.length, 101);
  assert.match(unexpectedBrowserEvents(events).at(-1).detail, /limit exceeded/);
});

test("server teardown closes a request left hanging by a failed browser", { timeout: 2_000 }, async () => {
  let requested;
  const received = new Promise((resolve) => { requested = resolve; });
  const server = createServer(() => requested());
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const request = get(`http://127.0.0.1:${server.address().port}/never-finishes`);
  request.on("error", () => {});
  try {
    await received;
    await closeServer({ server });
    assert.equal(server.listening, false);
  } finally {
    request.destroy();
    server.closeAllConnections();
    server.close();
  }
});

test("browser close lets its display wrapper exit cleanly before forced cleanup", async () => {
  const child = Object.assign(new EventEmitter(), { exitCode: null, signalCode: null });
  let closed = false;
  await closeBrowser({
    child, socket: { close: () => { closed = true; } },
    cdp: { send: async () => { setTimeout(() => { child.exitCode = 0; child.emit("exit", 0); }, 20); } },
  });
  assert.equal(child.exitCode, 0, "Browser.close acknowledges before xvfb-run finishes its cleanup");
  assert.equal(closed, true);
});

test("diagnostics preserve command timings and stderr when the renderer is dead", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "gpui-ai-evidence-test-"));
  const previous = process.env.GPUI_AI_WEB_ARTIFACTS;
  process.env.GPUI_AI_WEB_ARTIFACTS = directory;
  try {
    await saveBrowserEvidence({
      cdp: { commands: [{ method: "Input.dispatchTouchEvent", error: "timeout" }], send: async () => { throw new Error("renderer gone"); } },
      events: [{ kind: "crash", detail: "Renderer crashed" }], stderr: () => "GPU process failed",
    }, "failure");
    const [file] = await readdir(directory);
    const saved = JSON.parse(await readFile(path.join(directory, file), "utf8"));
    assert.equal(saved.state.error, "renderer gone");
    assert.equal(saved.screenshotError, "renderer gone");
    assert.equal(saved.stderr, "GPU process failed");
    assert.equal(saved.commands[0].method, "Input.dispatchTouchEvent");
  } finally {
    if (previous === undefined) delete process.env.GPUI_AI_WEB_ARTIFACTS;
    else process.env.GPUI_AI_WEB_ARTIFACTS = previous;
    await rm(directory, { recursive: true, force: true });
  }
});

test("backend fallback and actual asset errors cannot masquerade as a passing GPU run", () => {
  const pending = { kind: "asset-pending", detail: "first request" };
  const error = { kind: "error", detail: "asset failed" };
  const fallback = { kind: "warning", detail: "WebGPU initialization failed; falling back to WebGL2: no adapter" };
  assert.deepEqual(unexpectedBrowserEvents([pending, error, fallback]), [error, fallback]);
});

test("CDP keeps the real JavaScript exception and records command timeouts", async () => {
  class Socket extends EventTarget { send() {} }
  const cdp = new Cdp(new Socket());
  await assert.rejects(cdp.send("Input.dispatchTouchEvent", {}, 10), /Input.dispatchTouchEvent timed out/);
  assert.equal(cdp.commands[0].error, "timeout");
  assert.ok(cdp.commands[0].durationMs >= 0);
  cdp.send = async () => ({ exceptionDetails: { text: "Uncaught", exception: { description: "TypeError: missing input" } } });
  await assert.rejects(cdp.evaluate("broken()"), /TypeError: missing input/);
});
