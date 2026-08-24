import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdtemp, mkdir, readFile, rm, stat, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { buildSite } from "../scripts/build.mjs";
import catalog from "../generated/catalog.json" with { type: "json" };
import snippetFile from "../generated/snippets.json" with { type: "json" };
import themeFile from "../generated/themes.json" with { type: "json" };
import { auditExpression, report } from "./contrast.mjs";

const { components } = catalog;

const browserCandidates = process.platform === "win32"
  ? [
      process.env.CHROME_PATH,
      "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
      "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
      "C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe",
    ]
  : [
      process.env.CHROME_PATH,
      "/usr/bin/google-chrome",
      "/usr/bin/chromium",
      "/usr/bin/chromium-browser",
      "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
      "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
    ];
const browserPath = browserCandidates.find((candidate) => candidate && existsSync(candidate));
const releaseIntegrationRequested = process.env.GPUI_AI_RELEASE_INTEGRATION === "1";
// Skipping the release gate is a developer convenience, never a CI outcome: a
// runner without a browser would report green while proving nothing.
const releaseGateIsMandatory = releaseIntegrationRequested && process.env.CI === "true";
const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const releaseGalleryDir = path.join(repositoryRoot, "crates/gallery-web/www/dist");

const delay = (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds));

// Runs every cleanup step even when an earlier one throws, then reports the
// first failure. A browser that refuses to close must not strand the HTTP
// server or the temporary directory behind it.
async function settleAll(steps) {
  let failure;
  for (const step of steps) {
    try {
      await step();
    } catch (error) {
      failure ??= error;
    }
  }
  if (failure) throw failure;
}

async function createGalleryFixture(directory) {
  await mkdir(path.join(directory, "assets"), { recursive: true });
  await Promise.all([
    writeFile(path.join(directory, "index.html"), "gallery index"),
    writeFile(path.join(directory, "embed.html"), "<!doctype html><title>Gallery fixture</title>"),
    writeFile(path.join(directory, "assets", "gallery_bg-fixture.wasm"), "wasm"),
  ]);
}

async function serve(directory) {
  const server = createServer(async (request, response) => {
    try {
      const requestPath = decodeURIComponent(new URL(request.url, "http://localhost").pathname);
      if (requestPath === "/favicon.ico") {
        response.statusCode = 204;
        response.end();
        return;
      }
      // `/manual` is this harness's own mount point. `/gpui-ai` is the project
      // page's base path, which is baked into every asset URL the site emits:
      // without it the pages 404 their own bundle and never hydrate, so the
      // demos would silently never load.
      const mount = ["/manual/", "/gpui-ai/"].find((prefix) => requestPath.startsWith(prefix));
      const mountedPath = mount ? requestPath.slice(mount.length - 1) : requestPath;
      let file = path.resolve(directory, `.${mountedPath}`);
      if (!file.startsWith(path.resolve(directory))) throw new Error("outside site root");
      if ((await stat(file)).isDirectory()) file = path.join(file, "index.html");
      const extension = path.extname(file);
      const contentType = {
        ".css": "text/css",
        ".html": "text/html",
        ".js": "text/javascript",
        ".wasm": "application/wasm",
      }[extension] ?? "application/octet-stream";
      response.setHeader("content-type", contentType);
      response.end(await readFile(file));
    } catch {
      response.statusCode = 404;
      response.end("not found");
    }
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  return { server, origin: `http://127.0.0.1:${server.address().port}` };
}

class Cdp {
  constructor(socket) {
    this.socket = socket;
    this.nextId = 1;
    this.pending = new Map();
    this.listeners = new Map();
    socket.addEventListener("message", ({ data }) => {
      let message;
      try {
        message = JSON.parse(data);
      } catch (error) {
        this.fail(new Error(`Chromium sent invalid CDP JSON: ${error.message}`));
        return;
      }
      if (message.id) {
        const pending = this.pending.get(message.id);
        this.pending.delete(message.id);
        if (pending) clearTimeout(pending.timer);
        if (message.error) pending?.reject(new Error(message.error.message));
        else pending?.resolve(message.result);
        return;
      }
      for (const listener of this.listeners.get(message.method) ?? []) listener(message.params);
    });
    socket.addEventListener("close", () => this.fail(new Error("Chromium closed the CDP connection")));
    socket.addEventListener("error", () => this.fail(new Error("Chromium CDP connection failed")));
  }

  fail(error) {
    for (const pending of this.pending.values()) {
      clearTimeout(pending.timer);
      pending.reject(error);
    }
    this.pending.clear();
    for (const listeners of this.listeners.values()) {
      for (const listener of listeners) {
        clearTimeout(listener.timer);
        listener.reject?.(error);
      }
    }
    this.listeners.clear();
  }

  send(method, params = {}, timeoutMs = 5_000) {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`CDP ${method} timed out after ${timeoutMs}ms`));
      }, timeoutMs);
      this.pending.set(id, { resolve, reject, timer });
      try {
        this.socket.send(JSON.stringify({ id, method, params }));
      } catch (error) {
        clearTimeout(timer);
        this.pending.delete(id);
        reject(error);
      }
    });
  }

  once(method, timeoutMs = 5_000) {
    return new Promise((resolve, reject) => {
      const listener = (params) => {
        clearTimeout(listener.timer);
        this.listeners.set(method, (this.listeners.get(method) ?? []).filter((candidate) => candidate !== listener));
        resolve(params);
      };
      listener.reject = reject;
      listener.timer = setTimeout(() => {
        this.listeners.set(method, (this.listeners.get(method) ?? []).filter((candidate) => candidate !== listener));
        reject(new Error(`CDP event ${method} timed out after ${timeoutMs}ms`));
      }, timeoutMs);
      this.listeners.set(method, [...(this.listeners.get(method) ?? []), listener]);
    });
  }

  on(method, listener) {
    this.listeners.set(method, [...(this.listeners.get(method) ?? []), listener]);
  }

  async evaluate(expression, timeoutMs = 5_000) {
    const result = await this.send(
      "Runtime.evaluate",
      { expression, awaitPromise: true, returnByValue: true },
      timeoutMs,
    );
    if (result.exceptionDetails) throw new Error(result.exceptionDetails.text);
    return result.result.value;
  }

  async navigate(url, width, height, deviceScaleFactor = 1) {
    await this.send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor, mobile: false });
    const loaded = this.once("Page.loadEventFired");
    await this.send("Page.navigate", { url });
    await loaded;
  }

  /**
   * Clicks where an element actually is, rather than calling its handler.
   *
   * Only a dispatched pointer can tell whether something is reachable: a
   * `.click()` fires just as happily on an element another layer covers, one
   * pushed off screen, or one under `pointer-events: none`. The element's own
   * centre is not always a point a pointer can reach it at — a full-viewport
   * backdrop has a drawer sitting over the middle of it — so this probes the
   * centre and then the edges, and fails loudly if no point on the element
   * belongs to it.
   */
  async clickAt(selector) {
    const point = await this.evaluate(`(() => {
      const element = document.querySelector(${JSON.stringify(selector)});
      if (!element) return { error: 'no element matches' };
      const rect = element.getBoundingClientRect();
      if (rect.width === 0 || rect.height === 0) return { error: 'element has no box' };
      const candidates = [
        [rect.left + rect.width / 2, rect.top + rect.height / 2],
        [rect.right - 4, rect.top + rect.height / 2],
        [rect.left + rect.width / 2, rect.bottom - 4],
        [rect.left + 4, rect.top + rect.height / 2],
        [rect.left + rect.width / 2, rect.top + 4],
      ];
      for (const [x, y] of candidates) {
        const hit = document.elementFromPoint(Math.round(x), Math.round(y));
        if (hit === element || element.contains(hit)) {
          return { x: Math.round(x), y: Math.round(y) };
        }
      }
      const covering = document.elementFromPoint(
        Math.round(rect.left + rect.width / 2),
        Math.round(rect.top + rect.height / 2),
      );
      return { error: 'every point on it belongs to ' + (covering?.className || covering?.tagName) };
    })()`);
    if (point.error) throw new Error(`cannot click ${selector}: ${point.error}`);
    const at = { x: point.x, y: point.y, button: "left", clickCount: 1 };
    await this.send("Input.dispatchMouseEvent", { type: "mousePressed", ...at });
    await this.send("Input.dispatchMouseEvent", { type: "mouseReleased", ...at });
    return point;
  }

  async key(key, code, virtualKeyCode, modifiers = 0) {
    const params = { key, code, windowsVirtualKeyCode: virtualKeyCode, nativeVirtualKeyCode: virtualKeyCode, modifiers };
    await this.send("Input.dispatchKeyEvent", { ...params, type: "keyDown" });
    await this.send("Input.dispatchKeyEvent", { ...params, type: "keyUp" });
  }
}

test("CDP commands and events reject on timeout or connection loss", async () => {
  class FakeSocket extends EventTarget {
    send() {}
  }

  const socket = new FakeSocket();
  const cdp = new Cdp(socket);
  await assert.rejects(cdp.send("Never.responds", {}, 10), /timed out/);
  await assert.rejects(cdp.once("Never.arrives", 10), /timed out/);
  const pendingCommand = cdp.send("Browser.crashes", {}, 1_000);
  const pendingEvent = cdp.once("Page.neverLoads", 1_000);
  socket.dispatchEvent(new Event("close"));
  await assert.rejects(pendingCommand, /closed the CDP connection/);
  await assert.rejects(pendingEvent, /closed the CDP connection/);
});

async function launchBrowser(userDataDir) {
  const child = spawn(browserPath, [
    "--headless=new",
    "--remote-debugging-port=0",
    `--user-data-dir=${userDataDir}`,
    "--no-first-run",
    "--no-default-browser-check",
    "about:blank",
  ], { stdio: "ignore" });
  let socket;
  try {
    const portFile = path.join(userDataDir, "DevToolsActivePort");
    let port;
    // Chromium creates DevToolsActivePort before it writes the port into it, so
    // a successful read is not proof of a usable value. Keep waiting until the
    // first line is non-empty, and allow for a cold start on a loaded machine.
    for (let attempt = 0; attempt < 400; attempt += 1) {
      try {
        const [line] = (await readFile(portFile, "utf8")).trim().split(/\r?\n/);
        if (line) {
          port = line;
          break;
        }
      } catch {
        // The file only appears once Chromium has bound its debugging port.
      }
      await delay(50);
    }
    if (!port) throw new Error("Chromium DevTools port did not become ready within 20s");
    const target = await fetch(`http://127.0.0.1:${port}/json/new?about:blank`, { method: "PUT" }).then((response) => response.json());
    socket = new WebSocket(target.webSocketDebuggerUrl);
    await new Promise((resolve, reject) => {
      socket.addEventListener("open", resolve, { once: true });
      socket.addEventListener("error", reject, { once: true });
    });
    return { child, cdp: new Cdp(socket), socket };
  } catch (error) {
    socket?.close();
    await stopBrowserProcess(child);
    throw error;
  }
}

async function stopBrowserProcess(child) {
  if (!child || child.exitCode !== null) return;
  const exited = new Promise((resolve) => child.once("exit", resolve));
  if (process.platform === "win32") {
    const killer = spawn("taskkill", ["/pid", String(child.pid), "/T", "/F"], { stdio: "ignore" });
    await Promise.race([new Promise((resolve) => killer.once("exit", resolve)), delay(2_000)]);
  } else {
    child.kill("SIGKILL");
  }
  await Promise.race([exited, delay(2_000)]);
}

async function closeBrowser(browserHandle) {
  if (!browserHandle) return;
  await Promise.race([browserHandle.cdp.send("Browser.close"), delay(1_000)]).catch(() => {});
  browserHandle.socket.close();
  await stopBrowserProcess(browserHandle.child);
}

// Waits for a condition inside the page instead of round-tripping CDP every
// 100ms: one Runtime.evaluate replaces a couple of hundred messages, and the
// wait ends on the tick the condition holds rather than on the next poll.
//
// `fatal` names states the condition can never recover from — a WebGPU
// fallback panel, a module that failed to instantiate — so an unrecoverable
// run reports in a moment rather than burning the whole timeout. `describe`
// evaluates only on failure, and its job is to say which part was false: this
// gate previously failed twice in CI with nothing but `last value: false` for
// a three-way conjunction, which took a local bisect to read.
async function waitForValue(
  cdp,
  expression,
  { timeoutMs = 20_000, intervalMs = 50, label = "browser condition", fatal, describe, errors } = {},
) {
  const outcome = await cdp.evaluate(
    `(() => new Promise((resolve) => {
      const test = () => { try { return Boolean(${expression}); } catch { return false; } };
      const doomed = () => { try { return ${fatal ? `(${fatal})` : "false"}; } catch { return false; } };
      // A fatal state must hold twice running before it counts. A frame that
      // is reloading can show the previous document's fallback for an instant,
      // and aborting on that single sample would fail a run that recovers.
      let doomedSamples = 0;
      const settle = () => {
        if (test()) return { ok: true };
        if (!doomed()) {
          doomedSamples = 0;
          return undefined;
        }
        doomedSamples += 1;
        return doomedSamples > 1 ? { ok: false, fatal: doomed() } : undefined;
      };
      const first = settle();
      if (first) return resolve(first);
      const deadline = Date.now() + ${timeoutMs};
      const timer = setInterval(() => {
        const result = settle();
        if (result) { clearInterval(timer); resolve(result); return; }
        if (Date.now() >= deadline) { clearInterval(timer); resolve({ ok: false, timedOut: true }); }
      }, ${intervalMs});
    }))()`,
    // Outlive the in-page deadline so a genuine timeout reports its own
    // diagnosis rather than surfacing as an opaque CDP timeout.
    timeoutMs + 5_000,
  );

  if (outcome?.ok) return true;

  const state = describe
    ? await cdp
        .evaluate(`(() => { try { return ${describe}; } catch (error) { return { describeFailed: String(error) }; } })()`)
        .catch((error) => ({ describeFailed: String(error) }))
    : undefined;

  const lines = [
    outcome?.fatal
      ? `Gave up waiting for ${label}: ${outcome.fatal}`
      : `Timed out after ${timeoutMs}ms waiting for ${label}`,
  ];
  if (state !== undefined) lines.push(`state: ${JSON.stringify(state, null, 2)}`);
  lines.push(errors?.length ? `page errors:\n  ${errors.join("\n  ")}` : "page errors: none");
  throw new Error(lines.join("\n"));
}

// What the release gate needs to see when the embed never starts: which half
// of the condition failed, whether the module was even served, and what the
// fallback said.
const GALLERY_DIAGNOSIS = `(() => ({
  documentReady: document.readyState,
  hasCanvas: Boolean(document.querySelector('canvas')),
  stillLoading: Boolean(document.getElementById('loading')),
  fallbackVisible: (() => { const f = document.getElementById('fallback'); return Boolean(f && !f.hidden); })(),
  fallbackText: document.querySelector('#fallback [data-error]')?.textContent ?? null,
  hostTheme: document.documentElement.dataset.theme ?? null,
  reportedTheme: (() => { try { return window.gpuiAi?.currentTheme() ?? null; } catch { return 'unreachable'; } })(),
  wasmRequests: performance.getEntriesByType('resource')
    .filter((entry) => entry.name.endsWith('.wasm'))
    .map((entry) => ({ name: entry.name.split('/').pop(), transferred: entry.transferSize, duration: Math.round(entry.duration) })),
}))()`;

// An embed that has painted its WebGPU fallback will never produce a canvas.
const GALLERY_GAVE_UP = `(() => {
  const fallback = document.getElementById('fallback');
  return fallback && !fallback.hidden
    ? 'the embed rendered its WebGPU fallback: ' + (fallback.querySelector('[data-error]')?.textContent ?? 'no detail')
    : false;
})()`;

test("a timed-out wait names the condition, its state, and the collected page errors", async () => {
  const evaluated = [];
  const cdp = {
    evaluate: async (expression) => {
      evaluated.push(expression);
      return evaluated.length === 1
        ? { ok: false, timedOut: true }
        : { hasCanvas: false, stillLoading: true, reportedTheme: null };
    },
  };

  await assert.rejects(
    waitForValue(cdp, "false", {
      timeoutMs: 10,
      label: "the loading specimen to start",
      describe: "({})",
      errors: ["ReferenceError: WebSocket is not defined"],
    }),
    (error) => {
      assert.match(error.message, /Timed out after 10ms waiting for the loading specimen to start/);
      // The point of the diagnosis: which part was false, not just "false".
      assert.match(error.message, /"hasCanvas": false/);
      assert.match(error.message, /"stillLoading": true/);
      assert.match(error.message, /ReferenceError: WebSocket is not defined/);
      return true;
    },
  );
  assert.equal(evaluated.length, 2, "the state is described once, only after the wait fails");
});

test("a wait abandons an unrecoverable state instead of burning the whole timeout", async () => {
  const cdp = {
    evaluate: async () => ({ ok: false, fatal: "the specimen rendered its WebGPU fallback: no adapter" }),
  };

  await assert.rejects(
    waitForValue(cdp, "false", { timeoutMs: 20_000, label: "a canvas" }),
    (error) => {
      assert.match(error.message, /Gave up waiting for a canvas/);
      assert.match(error.message, /rendered its WebGPU fallback: no adapter/);
      assert.doesNotMatch(error.message, /Timed out/, "a fatal state is not a timeout");
      return true;
    },
  );
});

test("cleanup runs every step even when one throws, then reports the first failure", async () => {
  const ran = [];
  await assert.rejects(
    settleAll([
      () => {
        ran.push("browser");
        throw new Error("browser would not close");
      },
      () => {
        ran.push("server");
        throw new Error("server would not close");
      },
      () => {
        ran.push("directory");
      },
    ]),
    /browser would not close/,
  );
  assert.deepEqual(ran, ["browser", "server", "directory"], "no step may be skipped");
});

test("a successful wait reports no diagnosis and describes nothing", async () => {
  let describes = 0;
  const cdp = {
    evaluate: async (expression) => {
      if (expression.includes("describeMarker")) describes += 1;
      return { ok: true };
    },
  };

  assert.equal(await waitForValue(cdp, "true", { describe: "({ describeMarker: 1 })" }), true);
  assert.equal(describes, 0, "describing a healthy page is wasted work");
});

test("release WASM owns startup, theme sync, lifecycle, and WebGPU fallback", {
  skip: !browserPath && !releaseGateIsMandatory
    ? "Set CHROME_PATH or install Chrome, Edge, or Chromium to run the browser gate"
    : releaseIntegrationRequested ? false : "Run npm run check:web:release for the built-artifact integration gate",
  timeout: 60_000,
}, async (context) => {
  assert.ok(
    browserPath,
    "CI runs the release gate against a real browser, and none was found on PATH or CHROME_PATH; install one rather than letting this gate skip",
  );
  assert.equal(existsSync(path.join(releaseGalleryDir, "embed.html")), true, "build the release gallery before the site browser gate");
  const temporaryRoot = await mkdtemp(path.join(tmpdir(), "mighty-release-browser-"));
  const outDir = path.join(temporaryRoot, "site");
  const userDataDir = path.join(temporaryRoot, "browser");
  let serverHandle;
  let browserHandle;
  // Each step runs even if an earlier one throws: a browser that will not
  // close must not strand the HTTP server or the temporary directory.
  context.after(async () => {
    await settleAll([
      () => closeBrowser(browserHandle),
      () => (serverHandle ? new Promise((resolve) => serverHandle.server.close(resolve)) : undefined),
      () => rm(temporaryRoot, { force: true, recursive: true, maxRetries: 5, retryDelay: 100 }),
    ]);
  });

  await buildSite({ galleryDir: releaseGalleryDir, outDir });
  serverHandle = await serve(outDir);
  browserHandle = await launchBrowser(userDataDir);
  const { cdp } = browserHandle;
  const baseUrl = `${serverHandle.origin}/manual`;
  const errors = [];
  await Promise.all([cdp.send("Page.enable"), cdp.send("Runtime.enable"), cdp.send("Log.enable")]);
  // The site follows the operating system when nobody has chosen otherwise, so
  // the runner's own preference would decide what every check below sees. Pin
  // it; one step later flips it deliberately to prove the following works.
  await cdp.send("Emulation.setEmulatedMedia", {
    features: [{ name: "prefers-color-scheme", value: "light" }],
  });
  // Granted once, for every clipboard check below. Chrome also refuses the
  // clipboard to a document it does not consider focused, and a headless page
  // never is unless it is told it is.
  await cdp.send("Browser.grantPermissions", {
    origin: serverHandle.origin,
    permissions: ["clipboardReadWrite", "clipboardSanitizedWrite"],
  });
  await cdp.send("Emulation.setFocusEmulationEnabled", { enabled: true });
  await cdp.send("Page.bringToFront");
  cdp.on("Runtime.exceptionThrown", ({ exceptionDetails }) => errors.push(exceptionDetails.text));
  cdp.on("Runtime.consoleAPICalled", ({ type, args }) => {
    if (type === "error") errors.push(args.map((argument) => argument.value ?? argument.description).join(" "));
  });
  cdp.on("Log.entryAdded", ({ entry }) => {
    if (entry.level === "error") errors.push(entry.text);
  });

  // Drive the gallery directly rather than through a component page: this gate
  // is about whether the release artifact boots, syncs themes, and falls back
  // without WebGPU. The page chrome that used to wrap it is S-04, S-06 and
  // S-08 work, and coupling the artifact gate to unbuilt markup is what made
  // it fail for reasons that had nothing to do with the artifact.
  const embed = (story, theme) => `${baseUrl}/gallery/embed.html?story=${story}&theme=${theme}`;

  await cdp.navigate(embed("loading", "light"), 1280, 900);
  await waitForValue(
    cdp,
    `Boolean(document.querySelector('canvas') && !document.getElementById('loading') && window.gpuiAi?.currentTheme() === 'light')`,
    {
      label: "the release artifact to start and report the light theme",
      fatal: GALLERY_GAVE_UP,
      describe: GALLERY_DIAGNOSIS,
      errors,
    },
  );

  // Themes come from the generated registry, so check one of each group: a
  // basic preset, the review theme, one gpui-ai original, and one vendored
  // from upstream.
  for (const theme of ["dark", "contrast", "graphite", "tokyo-night"]) {
    await cdp.navigate(embed("loading", theme), 1280, 900);
    await waitForValue(
      cdp,
      "Boolean(document.querySelector('canvas') && window.gpuiAi?.currentTheme() === '" + theme + "')",
      {
        label: `the release artifact to apply the ${theme} theme`,
        fatal: GALLERY_GAVE_UP,
        describe: GALLERY_DIAGNOSIS,
        errors,
      },
    );
    assert.equal(await cdp.evaluate("window.gpuiAi.currentTheme()"), theme);
  }

  // The site's hero is a story like any other, and it must boot too.
  await cdp.navigate(embed("guided-demo", "dark"), 1280, 900);
  await waitForValue(cdp, "Boolean(document.querySelector('canvas'))", {
    label: "the guided-demo hero to start",
    fatal: GALLERY_GAVE_UP,
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  assert.deepEqual(errors, []);

  // Now through a real component page. The page ships no `src` — the frame
  // carries `data-src` and the observer promotes it — so this is the only
  // check that the demo a visitor actually meets ever starts. A page that
  // renders perfectly and never loads its demo passes every other gate.
  // Served from the project-page base the site was built for, because that is
  // where its own bundle and stylesheet live. Driven at a HiDPI device pixel
  // ratio, which is what most visitors have: GPUI's web backend takes that
  // ratio as its scale factor without scaling the canvas surface, so an
  // unpinned ratio lays the story out at double size and it overflows a frame
  // sized from the catalog. Nothing else here would notice.
  const specimen = components.find((component) => component.slug === "loading") ?? components[0];
  await cdp.navigate(`${serverHandle.origin}/gpui-ai/components/${specimen.slug}/`, 1280, 900, 2);
  await waitForValue(
    cdp,
    "Boolean(document.querySelector('[data-specimen-frame] iframe'))",
    {
      label: `the ${specimen.slug} page to load its demo once the frame is in view`,
      describe: GALLERY_DIAGNOSIS,
      errors,
    },
  );
  assert.equal(
    await cdp.evaluate(
      "document.querySelector('[data-specimen-frame] iframe').getAttribute('src')",
    ),
    `/gpui-ai/gallery/embed.html?story=${specimen.slug}`,
  );
  // The frame is the height the gallery measured, not a guess.
  assert.equal(
    await cdp.evaluate(
      "Math.round(document.querySelector('[data-specimen-frame]').getBoundingClientRect().height)",
    ),
    specimen.height,
  );
  await waitForValue(
    cdp,
    "(() => { const frame = document.querySelector('[data-specimen-frame] iframe'); return Boolean(frame?.contentDocument?.querySelector('canvas')); })()",
    {
      label: `the ${specimen.slug} demo to start inside the page`,
      fatal: GALLERY_GAVE_UP,
      describe: GALLERY_DIAGNOSIS,
      errors,
    },
  );
  assert.deepEqual(errors, []);

  // And the story inside it is laid out at that same scale. Nothing in the DOM
  // reports what the canvas drew, so check the input GPUI reads: unpinned, the
  // ratio the page is running at becomes its scale factor and the demo paints
  // at double size inside a frame that cannot grow. Asserted only once the
  // canvas exists — before that the iframe's window is a fresh about:blank
  // that has not run the embed's script and still reports the parent's ratio.
  assert.equal(await cdp.evaluate("window.devicePixelRatio"), 2, "the page is running HiDPI");
  assert.equal(
    await cdp.evaluate(
      "document.querySelector('[data-specimen-frame] iframe').contentWindow.devicePixelRatio",
    ),
    1,
    "the embed must pin its scale factor or every measured height is wrong",
  );

  // The other half of lazy: a frame that is nowhere near the viewport must not
  // load. Arriving at a deep anchor on a short viewport puts the demo well
  // above the observer's margin. Without this, promoting every frame on
  // hydration would pass every check above — and every visitor reading prose
  // would pay for the shared binary.
  const deep = components.find((component) => component.slug === "chat") ?? specimen;
  await cdp.navigate(`${serverHandle.origin}/gpui-ai/components/${deep.slug}/#limits`, 1280, 400);
  await waitForValue(cdp, "Boolean(document.querySelector('[data-specimen-frame]'))", {
    label: `the ${deep.slug} page to render its frame`,
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  assert.ok(
    await cdp.evaluate(
      "document.querySelector('[data-specimen-frame]').getBoundingClientRect().bottom < -window.innerHeight",
    ),
    "the anchor must leave the demo more than a viewport above the fold",
  );
  await delay(1_000);
  assert.equal(
    await cdp.evaluate("document.querySelectorAll('[data-specimen-frame] iframe').length"),
    0,
    "a demo far outside the viewport must not fetch the shared binary",
  );

  await cdp.evaluate("window.scrollTo(0, 0)");
  await waitForValue(cdp, "Boolean(document.querySelector('[data-specimen-frame] iframe'))", {
    label: "scrolling back to the demo to load it",
    describe: GALLERY_DIAGNOSIS,
    errors,
  });

  // S-03's whole claim: the chrome is painted from the generated tokens, so
  // setting the attribute the registry keys on repaints it. Nothing static can
  // check this — a stylesheet full of var() references looks correct whether or
  // not the properties resolve, and only a browser knows what was painted.
  const repaint = await cdp.evaluate(`(() => {
    const read = () => {
      const body = getComputedStyle(document.body);
      const rail = document.querySelector('.component-reference');
      return {
        background: body.backgroundColor,
        foreground: body.color,
        border: rail ? getComputedStyle(rail).borderTopColor : null,
        radius: rail ? getComputedStyle(rail).borderTopLeftRadius : null,
        face: body.fontFamily,
      };
    };
    const root = document.documentElement;
    const was = root.dataset.theme;
    const before = read();
    root.dataset.theme = 'ember-dusk';
    const after = read();
    // Put back whatever the inline script decided, rather than removing the
    // attribute: with no attribute the page falls to :root, which is a
    // different theme, not the one it was showing.
    root.dataset.theme = was;
    const restored = read();
    return { before, after, restored, was };
  })()`);
  for (const property of ["background", "foreground", "border"]) {
    assert.notEqual(
      repaint.after[property],
      repaint.before[property],
      `switching data-theme left ${property} at ${repaint.before[property]}`,
    );
  }
  assert.deepEqual(repaint.restored, repaint.before, "putting data-theme back must undo the change");
  assert.ok(repaint.was, "the inline script must have painted a theme before anything rendered");
  // The face comes from a token too, so a theme that changed it would move the
  // chrome and the demos together.
  assert.match(repaint.before.face, /IBM Plex Sans/);

  // Every interaction below needs the page hydrated, or it drives inert
  // markup and reports that nothing happened. The shell writes data-theme on
  // mount, so the attribute appearing is this site's own signal that its
  // handlers are attached — and that the theme is applied after render rather
  // than baked into the pre-render, which is what keeps hydration clean.
  // Colours cross-fade over 200ms, so a read taken the instant the attribute
  // changes still sees the old palette. Waiting for the class the fade runs
  // under also proves it is added and then taken away again — a transition
  // left in place would catch every later hover.
  const settleTheme = async (label, previous) => {
    // Waiting for the body to actually be a different colour is the only
    // deterministic way to read the new one: a fixed delay races the
    // transition, and the class the fade runs under comes off on its own timer
    // rather than when the colours have finished moving.
    await waitForValue(
      cdp,
      `getComputedStyle(document.body).backgroundColor !== ${JSON.stringify(previous)}`,
      { label: `the ${label} cross-fade to finish`, describe: GALLERY_DIAGNOSIS, errors },
    );
    // And the transition must not still be in force afterwards, or it catches
    // every later hover and makes the whole page feel slow.
    await waitForValue(cdp, "!document.documentElement.classList.contains('theming')", {
      label: `the ${label} transition to come back off`,
      describe: GALLERY_DIAGNOSIS,
      errors,
    });
  };

  const openPage = async (route, width, height, expected = "light") => {
    await cdp.navigate(`${serverHandle.origin}/gpui-ai${route}`, width, height);
    await waitForValue(
      cdp,
      `document.documentElement.dataset.theme === ${JSON.stringify(expected)}`,
      {
        label: `${route} to hydrate and apply its theme`,
        describe: GALLERY_DIAGNOSIS,
        errors,
      },
    );
  };

  // The drawer, driven the way a keyboard drives it. None of this is visible
  // in the markup: the panel ships hidden and everything below happens after
  // mount, so HTML assertions can only prove the parts exist.
  await openPage(`/components/${specimen.slug}/`, 390, 844);
  await cdp.evaluate(`(() => {
    const toggle = document.querySelector('[data-nav-toggle]');
    toggle.focus();
    toggle.click();
  })()`);
  await waitForValue(cdp, "!document.getElementById('site-nav-panel').hidden", {
    label: "the drawer to open",
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  const opened = await cdp.evaluate(`(() => {
    const toggle = document.querySelector('[data-nav-toggle]');
    const panel = document.getElementById('site-nav-panel');
    return {
      expanded: toggle.getAttribute('aria-expanded'),
      visible: !panel.hidden && panel.getBoundingClientRect().width > 0,
      focused: document.activeElement?.textContent,
      // Read before the focus probe below moves it.
      // Everything beside the panel must be inert, or Tab wanders into a page
      // the visitor cannot see and cannot get back from. The attribute goes on
      // the panel's siblings and inherits, so the property worth checking is
      // the one a keyboard would hit: nothing behind the drawer can take focus.
      inertSiblings: [...panel.parentElement.children]
        .filter((child) => child !== panel)
        .every((child) => child.hasAttribute('inert')),
      contentUnreachable: (() => {
        const behind = document.querySelector('#content a, #content button');
        if (!behind) return 'nothing focusable behind the drawer to test';
        behind.focus();
        return panel.contains(document.activeElement);
      })(),
      current: document.querySelectorAll('#site-nav-panel [aria-current="page"]').length,
    };
  })()`);
  assert.deepEqual(opened, {
    expanded: "true",
    visible: true,
    focused: "Close",
    inertSiblings: true,
    contentUnreachable: true,
    current: 1,
  });

  // A modal is supposed to cycle: Shift+Tab from the first control lands on
  // the last, and Tab from the last comes back to the first. `inert` keeps the
  // page behind out of the sequence but does nothing about its two ends.
  const wrapped = await cdp.evaluate(`(() => {
    const panel = document.getElementById('site-nav-panel');
    const stops = [...panel.querySelectorAll('a[href], button, input, [tabindex]')]
      .filter((element) => element.tabIndex >= 0 && element.offsetParent !== null);
    return { first: stops[0]?.textContent?.trim(), last: stops[stops.length - 1]?.textContent?.trim(), count: stops.length };
  })()`);
  assert.ok(wrapped.count > 2, `the drawer has only ${wrapped.count} tab stops`);

  await cdp.evaluate(
    "document.querySelector('#site-nav-panel [data-nav-close]').focus()",
  );
  await cdp.key("Tab", "Tab", 9, 8);
  assert.equal(
    await cdp.evaluate("document.activeElement?.textContent?.trim()"),
    wrapped.last,
    "Shift+Tab from the first control must wrap to the last, not leave the drawer",
  );
  await cdp.key("Tab", "Tab", 9);
  assert.equal(
    await cdp.evaluate("document.activeElement?.textContent?.trim()"),
    wrapped.first,
    "Tab from the last control must wrap back to the first",
  );

  await cdp.key("Escape", "Escape", 27);
  const closed = await cdp.evaluate(`(() => {
    const panel = document.getElementById('site-nav-panel');
    return {
      expanded: document.querySelector('[data-nav-toggle]').getAttribute('aria-expanded'),
      hidden: panel.hidden,
      // Focus has to come back to something, and the toggle is where the
      // visitor left it.
      focused: document.activeElement?.dataset.navToggle !== undefined,
      anyInert: document.querySelectorAll('[inert]').length,
    };
  })()`);
  assert.deepEqual(closed, { expanded: "false", hidden: true, focused: true, anyInert: 0 });

  // The backdrop is pointer-only by design, so prove the pointer path works
  // too — otherwise it is decoration that traps a mouse user.
  await cdp.evaluate("document.querySelector('[data-nav-toggle]').click()");
  await waitForValue(cdp, "!document.getElementById('site-nav-panel').hidden", {
    label: "the drawer to reopen for the pointer path",
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  // Dispatched at coordinates, not through the element's own click(): the
  // claim is that a pointer reaches the backdrop, and a handler fires just as
  // happily on something buried under another layer.
  // Throws unless a real pointer can land on the backdrop itself, which its
  // own centre cannot do — the drawer covers that.
  await cdp.clickAt(".nav-backdrop");
  await waitForValue(cdp, "document.getElementById('site-nav-panel').hidden", {
    label: "a backdrop click to close the drawer",
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  assert.deepEqual(
    await cdp.evaluate(`(() => ({
      backdropIsButton: Boolean(document.querySelector('button.nav-backdrop')),
      backdropFocusable: document.querySelector('.nav-backdrop').tabIndex >= 0,
    }))()`),
    { backdropIsButton: false, backdropFocusable: false },
  );

  // A drawer open across the desktop breakpoint. The toggle that opened it is
  // display:none up here, so handing focus back to it would drop focus onto
  // nothing — the page would look fine and the keyboard would be lost.
  await cdp.evaluate("document.querySelector('[data-nav-toggle]').click()");
  await waitForValue(cdp, "!document.getElementById('site-nav-panel').hidden", {
    label: "the drawer to open before the resize",
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  await cdp.send("Emulation.setDeviceMetricsOverride", {
    width: 1280,
    height: 900,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await waitForValue(cdp, "document.getElementById('site-nav-panel').hidden", {
    label: "the drawer to close when the rail appears",
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  assert.deepEqual(
    await cdp.evaluate(`(() => {
      const active = document.activeElement;
      return {
        onSomethingVisible: Boolean(active && active !== document.body && active.offsetParent !== null || active?.id === 'content'),
        id: active?.id ?? active?.tagName ?? null,
        anyInert: document.querySelectorAll('[inert]').length,
      };
    })()`),
    { onSomethingVisible: true, id: "content", anyInert: 0 },
    "focus must land somewhere real when the drawer closes itself",
  );
  await cdp.send("Emulation.setDeviceMetricsOverride", {
    width: 390,
    height: 844,
    deviceScaleFactor: 1,
    mobile: false,
  });

  // The mode control has to actually change what the page is painted from.
  const readMode = `(() => ({
    theme: document.documentElement.dataset.theme,
    background: getComputedStyle(document.body).backgroundColor,
    pressed: [...document.querySelectorAll('[data-theme-choice]')]
      .filter((button) => button.getAttribute('aria-pressed') === 'true')
      .map((button) => button.dataset.themeChoice),
  }))()`;
  const beforeMode = await cdp.evaluate(readMode);
  assert.deepEqual(beforeMode.pressed, ["system"], "a visitor who has chosen nothing follows the system");
  await cdp.evaluate("document.querySelector('[data-theme-choice=\"dark\"]').click()");
  await waitForValue(cdp, "document.documentElement.dataset.theme === 'dark'", {
    label: "the dark control to change the mode",
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  await settleTheme("dark", beforeMode.background);
  const afterMode = await cdp.evaluate(readMode);
  assert.deepEqual(afterMode.pressed, ["dark"], "the control must state which mode is current");
  assert.notEqual(afterMode.background, beforeMode.background, "dark repainted nothing");
  // The embed reads this class when the host names no theme, so a demo opened
  // after the switch starts dark instead of contradicting the page around it.
  assert.equal(await cdp.evaluate("document.documentElement.classList.contains('dark')"), true);

  // And a demo that was already running follows too. Without this the page
  // goes dark around a white window, which is worse than not offering the
  // control — and no HTML assertion can see it, because the frame's contents
  // are drawn on a canvas.
  await waitForValue(
    cdp,
    "document.querySelector('[data-specimen-frame] iframe')?.contentWindow?.gpuiAi?.currentTheme() === 'dark'",
    {
      label: "the running demo to follow the page into dark",
      fatal: GALLERY_GAVE_UP,
      describe: GALLERY_DIAGNOSIS,
      errors,
    },
  );

  // The whole theme engine, end to end, in the only place it can be checked.
  // Picking a registry theme has to survive a reload, put itself in the URL so
  // the page can be linked as it looks, and repaint chrome and demo together.
  await cdp.evaluate(`(() => {
    const select = document.getElementById('site-theme');
    select.value = 'ember-dusk';
    select.dispatchEvent(new Event('change', { bubbles: true }));
  })()`);
  await waitForValue(cdp, "document.documentElement.dataset.theme === 'ember-dusk'", {
    label: 'the picker to apply a registry theme',
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  await settleTheme("Ember Dusk", afterMode.background);
  const picked = await cdp.evaluate(`(() => ({
    stored: window.localStorage.getItem('gpui-ai:theme'),
    param: new URLSearchParams(window.location.search).get('theme'),
    background: getComputedStyle(document.body).backgroundColor,
    pressed: [...document.querySelectorAll('[data-theme-choice]')]
      .filter((button) => button.getAttribute('aria-pressed') === 'true')
      .map((button) => button.dataset.themeChoice),
  }))()`);
  assert.equal(picked.stored, 'ember-dusk', 'the choice must survive a reload');
  assert.equal(picked.param, 'ember-dusk', 'the page must be linkable as it looks');
  assert.notEqual(picked.background, afterMode.background, 'the registry theme repainted nothing');
  // None of the three mode buttons is what is showing, and saying otherwise
  // would be a lie a screen reader repeats.
  assert.deepEqual(picked.pressed, []);

  // Back to following the system, and then move the system. Nothing is stored
  // and nothing is in the URL, so the only thing left to follow is the machine.
  await cdp.evaluate(`(() => {
    const select = document.getElementById('site-theme');
    select.value = 'system';
    select.dispatchEvent(new Event('change', { bubbles: true }));
  })()`);
  await waitForValue(cdp, "document.documentElement.dataset.theme === 'light'", {
    label: "returning to the system preference",
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  assert.equal(
    await cdp.evaluate("window.localStorage.getItem('gpui-ai:theme')"),
    null,
    "following the system is the absence of a choice, not a stored one",
  );
  assert.equal(
    await cdp.evaluate("new URLSearchParams(window.location.search).get('theme')"),
    null,
    "the URL must stop naming a theme once the visitor stops choosing one",
  );
  await cdp.send("Emulation.setEmulatedMedia", {
    features: [{ name: "prefers-color-scheme", value: "dark" }],
  });
  await waitForValue(cdp, "document.documentElement.dataset.theme === 'dark'", {
    label: "the page to follow the system flipping to dark",
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  await cdp.send("Emulation.setEmulatedMedia", {
    features: [{ name: "prefers-color-scheme", value: "light" }],
  });
  await waitForValue(cdp, "document.documentElement.dataset.theme === 'light'", {
    label: "the page to follow the system back to light",
    describe: GALLERY_DIAGNOSIS,
    errors,
  });

  // Store a theme again, then reload: the inline script has to paint it
  // before anything else renders, or the page flashes the default first.
  await cdp.evaluate("window.localStorage.setItem('gpui-ai:theme', 'ember-dusk')");

  // Watch for the first time anything sets the attribute, and record what the
  // document was doing at that moment. This is the only way to tell the inline
  // script from the React effect: both leave the same attribute behind, and
  // only one of them beats the stylesheet. `loading` means the head is still
  // being parsed; anything else means the page painted the wrong palette first
  // and then corrected itself, which is the flash the script exists to prevent.
  const { identifier } = await cdp.send("Page.addScriptToEvaluateOnNewDocument", {
    source: `
      window.__firstThemePaint = null;
      var record = function () {
        if (window.__firstThemePaint) return;
        var root = document.documentElement;
        if (!root || !root.getAttribute('data-theme')) return;
        window.__firstThemePaint = {
          theme: root.getAttribute('data-theme'),
          readyState: document.readyState,
        };
      };
      // Observed on the document rather than on documentElement: this runs
      // before the page has any script of its own, and at that point there may
      // be no element to attach to yet.
      new MutationObserver(record).observe(document, {
        attributes: true,
        subtree: true,
        attributeFilter: ['data-theme'],
      });
      record();
    `,
  });
  await openPage(`/components/${specimen.slug}/`, 1280, 900, "ember-dusk");
  await cdp.send("Page.removeScriptToEvaluateOnNewDocument", { identifier });

  assert.deepEqual(
    await cdp.evaluate("window.__firstThemePaint"),
    { theme: "ember-dusk", readyState: "loading" },
    "a stored theme must be painted while the head is still parsing, not after hydration",
  );

  // And a link carrying a theme wins over the stored one for that visit.
  await cdp.navigate(
    `${serverHandle.origin}/gpui-ai/components/${specimen.slug}/?theme=solstice`,
    1280,
    900,
  );
  assert.equal(
    await cdp.evaluate('document.documentElement.dataset.theme'),
    'solstice',
    'a theme in the URL must win for the visit it was linked for',
  );
  await cdp.evaluate("window.localStorage.removeItem('gpui-ai:theme')");
  await cdp.evaluate("window.history.replaceState(null, '', window.location.pathname)");
  // The demo's own toolbar. The override is the interesting one: it has to
  // move this frame without moving the page, and without the frame being torn
  // down and rebuilt, which is why it travels as a message rather than a URL.
  await openPage(`/components/${specimen.slug}/`, 1280, 900);
  const frameTheme = (theme) =>
    `document.querySelector('[data-specimen-frame] iframe')?.contentWindow?.gpuiAi?.currentTheme() === '${theme}'`;
  await waitForValue(cdp, frameTheme("light"), {
    label: "the demo to start out following the site",
    fatal: GALLERY_GAVE_UP,
    describe: GALLERY_DIAGNOSIS,
    errors,
  });

  const readout = () => cdp.evaluate("document.querySelector('.demo-readout').dataset.readout");
  assert.equal(await readout(), "light", "the readout must name what the frame is painted from");

  await cdp.evaluate(`(() => {
    const select = document.querySelector('.demo-toolbar select');
    select.value = 'ember-dusk';
    select.dispatchEvent(new Event('change', { bubbles: true }));
  })()`);
  await waitForValue(cdp, frameTheme("ember-dusk"), {
    label: "the demo to take a theme of its own",
    fatal: GALLERY_GAVE_UP,
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  assert.equal(await readout(), "ember-dusk");
  assert.match(
    await cdp.evaluate("document.querySelector('.demo-readout').textContent"),
    /OVERRIDDEN$/,
    "a frame that has stopped following the page should say so",
  );
  // And the page has not moved. An override that changed the site theme would
  // be a different control wearing the same label.
  assert.equal(await cdp.evaluate("document.documentElement.dataset.theme"), "light");
  assert.match(
    await cdp.evaluate("document.querySelector('[data-specimen-open]').getAttribute('href')"),
    /theme=ember-dusk$/,
    "Pop out must open the demo as it is being shown, not as it started",
  );

  // Reload replaces the frame rather than reaching into it, so the proof is a
  // new document that boots again and comes back to the same theme.
  const wasStartedAt = await cdp.evaluate(
    "document.querySelector('[data-specimen-frame] iframe').contentWindow.performance.timeOrigin",
  );
  await cdp.evaluate("document.querySelector('[data-specimen-reload]').click()");
  await waitForValue(
    cdp,
    `(() => {
      const frame = document.querySelector('[data-specimen-frame] iframe');
      return Boolean(frame?.contentWindow) && frame.contentWindow.performance.timeOrigin !== ${wasStartedAt};
    })()`,
    { label: "Reload to replace the frame", describe: GALLERY_DIAGNOSIS, errors },
  );
  await waitForValue(cdp, frameTheme("ember-dusk"), {
    label: "the reloaded demo to come back overridden",
    fatal: GALLERY_GAVE_UP,
    describe: GALLERY_DIAGNOSIS,
    errors,
  });

  await cdp.evaluate("document.querySelector('[data-specimen-link]').click()");
  await waitForValue(
    cdp,
    "document.querySelector('.demo-toolbar .copy-status').textContent.length > 0",
    { label: "Copy link to report what it did", describe: GALLERY_DIAGNOSIS, errors },
  );
  assert.match(
    await cdp.evaluate("navigator.clipboard.readText()"),
    new RegExp(`/components/${specimen.slug}/\\?theme=ember-dusk$`),
    "Copy link must hand over the page as it is being looked at",
  );

  // Copy, against a real clipboard. Everything short of this checks that the
  // page holds the right string somewhere; only reading the clipboard back
  // shows what a visitor would paste. The panel is highlighted, so the
  // failure this rules out is copying spans, classes, or a partial line.
  await openPage(`/components/${specimen.slug}/`, 1280, 900);
  await cdp.evaluate("document.querySelector('[data-copy]').click()");
  await waitForValue(cdp, "document.querySelector('.code-actions .copy-status').textContent.length > 0", {
    label: 'the copy button to report what it did',
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  // Line endings are normalised on the way back out: the Windows clipboard
  // hands back CRLF whatever was put in. That is the operating system, not the
  // page, and asserting on it would make this test pass on one platform only.
  assert.equal(
    (await cdp.evaluate("navigator.clipboard.readText()")).replaceAll("\r\n", "\n"),
    snippetFile.snippets[specimen.slug].default,
    "the clipboard must hold the snippet, not the markup around it",
  );
  assert.match(
    await cdp.evaluate("document.querySelector('.code-actions .copy-status').textContent"),
    /^Copied /,
    'a copy that says nothing is a copy a visitor cannot trust',
  );
  // The themes page is the one that has to prove the whole claim: the site and
  // the demos are painted from the same numbers. Choosing a card repaints the
  // page and the running trio together, and the file behind Download is the
  // theme itself rather than a picture of it.
  await openPage("/themes/", 1280, 900);
  await waitForValue(
    cdp,
    "document.querySelectorAll('[data-specimen-frame] iframe').length >= 2",
    { label: "the themes page trio to promote", describe: GALLERY_DIAGNOSIS, errors },
  );
  await waitForValue(
    cdp,
    "[...document.querySelectorAll('[data-specimen-frame] iframe')].every((frame) => frame.contentWindow?.gpuiAi?.currentTheme() === 'light')",
    {
      label: "the trio to start on the site theme",
      fatal: GALLERY_GAVE_UP,
      describe: GALLERY_DIAGNOSIS,
      errors,
    },
  );

  const pageBackground = await cdp.evaluate("getComputedStyle(document.body).backgroundColor");
  await cdp.evaluate("document.querySelector('[data-use-theme=\"nord-frost\"]').click()");
  await settleTheme("Nord Frost", pageBackground);
  assert.equal(await cdp.evaluate("document.documentElement.dataset.theme"), "nord-frost");
  await waitForValue(
    cdp,
    "[...document.querySelectorAll('[data-specimen-frame] iframe')].every((frame) => frame.contentWindow?.gpuiAi?.currentTheme() === 'nord-frost')",
    {
      label: "every demo on the page to follow the card that was chosen",
      fatal: GALLERY_GAVE_UP,
      describe: GALLERY_DIAGNOSIS,
      errors,
    },
  );
  assert.equal(
    await cdp.evaluate("document.querySelector('[data-use-theme=\"nord-frost\"]').disabled"),
    true,
    "the card in use should not offer to be used again",
  );

  // Download is a real file at a real URL, not a blob built in the page from
  // values the site derived — those would read back as a theme and not be one.
  const download = await cdp.evaluate(`(async () => {
    const link = document.querySelector('[data-theme-card="nord-frost"] a[download]');
    const response = await fetch(link.href);
    return { status: response.status, body: await response.json(), href: link.getAttribute('href') };
  })()`);
  assert.equal(download.status, 200, `${download.href} does not resolve`);
  assert.equal(download.body.themes.length, 1);
  assert.match(download.body.themes[0].name, /Nord Frost/);
  assert.ok(
    Object.keys(download.body.themes[0].colors).length > 3,
    "the downloaded theme has no colours",
  );

  // Choosing a card is a durable choice, which is the point of it — so put the
  // browser back to a visitor who has chosen nothing before the checks below.
  await cdp.evaluate("window.localStorage.removeItem('gpui-ai:theme')");
  await cdp.evaluate("window.history.replaceState(null, '', window.location.pathname)");

  // The skip link is the first thing Tab reaches, and it moves focus rather
  // than only scrolling — which is the whole reason main carries tabindex.
  await openPage(`/components/${specimen.slug}/`, 1280, 900);
  await cdp.key("Tab", "Tab", 9);
  const skip = await cdp.evaluate("document.activeElement?.className");
  assert.equal(skip, "skip-link", "the skip link must be the first tab stop");
  await cdp.key("Enter", "Enter", 13);
  assert.equal(
    await cdp.evaluate("document.activeElement?.id"),
    "content",
    "following the skip link must move focus into the content",
  );

  // On a phone the rail stops being a sidebar, but it is the only place the
  // page carries the rustdoc link, the source link, and the reference table.
  // Laying it out with `display: none` below a breakpoint would take all of
  // that off every small screen while leaving the markup in place, which every
  // HTML-level assertion would happily match.
  await cdp.navigate(`${serverHandle.origin}/gpui-ai/components/${specimen.slug}/`, 390, 844);
  const railOnMobile = await cdp.evaluate(`(() => {
    const link = document.querySelector('.component-reference a[href*="/api/gpui_ai/"]');
    if (!link) return { rendered: false, reason: 'no rustdoc link' };
    const box = link.getBoundingClientRect();
    return { rendered: box.width > 0 && box.height > 0, href: link.getAttribute('href') };
  })()`);
  assert.equal(
    railOnMobile.rendered,
    true,
    `the API link must stay on the page at 390px wide (${JSON.stringify(railOnMobile)})`,
  );
  assert.match(railOnMobile.href, new RegExp(`struct\\.${specimen.api}\\.html$`));

  // Without WebGPU the *site* must say so, and — the part that matters — must
  // not have fetched anything. Every check above is on a machine that can draw;
  // this one is the promise the card makes to a machine that cannot. The stub
  // defines the property and returns nothing, which is what a browser with the
  // API disabled does, and which an `in` check would have read as yes.
  const { identifier: noGpu } = await cdp.send("Page.addScriptToEvaluateOnNewDocument", {
    source:
      "Object.defineProperty(Navigator.prototype, 'gpu', { configurable: true, get: () => undefined });",
  });
  await openPage(`/components/${specimen.slug}/`, 1280, 900);
  await waitForValue(cdp, "Boolean(document.querySelector('[data-webgpu-fallback]'))", {
    label: "the site's own WebGPU card",
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  await delay(1_000);
  assert.deepEqual(
    await cdp.evaluate(`(() => ({
      frames: document.querySelectorAll('[data-specimen-frame] iframe').length,
      requested: performance
        .getEntriesByType('resource')
        .filter((entry) => /gallery|\\.wasm$/.test(entry.name)).length,
    }))()`),
    { frames: 0, requested: 0 },
    "a browser that cannot draw the demo must not be made to download it",
  );
  await cdp.send("Page.removeScriptToEvaluateOnNewDocument", { identifier: noGpu });

  // Without WebGPU the embed must say so rather than showing an empty frame.
  await cdp.send("Page.addScriptToEvaluateOnNewDocument", {
    source: "Object.defineProperty(Navigator.prototype, 'gpu', { configurable: true, get: () => undefined });",
  });
  await cdp.navigate(embed("diff-table", "contrast"), 1280, 900);
  await waitForValue(
    cdp,
    "(() => { const fallback = document.getElementById('fallback'); return Boolean(fallback && !fallback.hidden); })()",
    { label: "the WebGPU fallback to appear", describe: GALLERY_DIAGNOSIS, errors },
  );
  assert.equal(
    await cdp.evaluate("document.querySelector('#fallback [data-error]').textContent"),
    "This live example requires a browser with WebGPU support.",
  );
  assert.equal(await cdp.evaluate("document.querySelectorAll('canvas').length"), 0);
});

test("every theme the site offers can be read", {
  skip: !browserPath && !releaseGateIsMandatory
    ? "Set CHROME_PATH or install Chrome, Edge, or Chromium to run the browser gate"
    : releaseIntegrationRequested ? false : "Run npm run check:web:release for the built-artifact integration gate",
  timeout: 120_000,
}, async (context) => {
  assert.ok(browserPath, "CI runs this against a real browser, and none was found");

  // Forty-five themes, and the site paints its chrome from all of them. A
  // theme is not a picture here — it decides whether the prose, the code, and
  // the controls can be read at all, and nothing else in the suite would
  // notice a palette that puts grey text on a grey card.
  const temporaryRoot = await mkdtemp(path.join(tmpdir(), "mighty-contrast-"));
  const outDir = path.join(temporaryRoot, "site");
  const galleryDir = path.join(temporaryRoot, "gallery");
  const userDataDir = path.join(temporaryRoot, "browser");
  let serverHandle;
  let browserHandle;
  context.after(async () => {
    await settleAll([
      () => closeBrowser(browserHandle),
      () => (serverHandle ? new Promise((resolve) => serverHandle.server.close(resolve)) : undefined),
      () => rm(temporaryRoot, { force: true, recursive: true, maxRetries: 5, retryDelay: 100 }),
    ]);
  });

  // A fixture gallery, not the release artifact: this is about the chrome the
  // site paints, and booting thirty-four WebGPU canvases would prove nothing
  // about it while taking a hundred times as long.
  await createGalleryFixture(galleryDir);
  await buildSite({ galleryDir, outDir });
  serverHandle = await serve(outDir);
  browserHandle = await launchBrowser(userDataDir);
  const { cdp } = browserHandle;
  await Promise.all([cdp.send("Page.enable"), cdp.send("Runtime.enable")]);

  const own = new Set(
    themeFile.groups.find((group) => group.id === "gpui-ai").themes.map((theme) => theme.slug),
  );
  const slugs = themeFile.groups.flatMap((group) => group.themes.map((theme) => theme.slug));
  assert.ok(slugs.length > 40, `only ${slugs.length} themes were found`);

  const routes = ["/", `/components/${components[0].slug}/`, "/themes/"];
  const findings = [];
  for (const route of routes) {
    await cdp.navigate(`${serverHandle.origin}/gpui-ai${route}`, 1280, 900);
    const audit = await cdp.evaluate(auditExpression(slugs), 90_000);
    assert.ok(audit.elements > 20, `${route} has only ${audit.elements} pieces of text to check`);
    // An audit that writes the attribute and gets ignored reports a clean bill
    // of health for one theme, forty-five times. Distinct backgrounds are the
    // cheapest proof it really visited them.
    assert.ok(
      audit.palettes > slugs.length / 2,
      `${route} painted only ${audit.palettes} distinct backgrounds across ${slugs.length} themes`,
    );
    findings.push(...audit.findings.map((finding) => ({ ...finding, route })));
  }

  const ours = findings.filter((finding) => own.has(finding.theme));
  const vendored = findings.filter((finding) => !own.has(finding.theme));

  // The upstream pack is shown as published and credited, so a palette of
  // theirs that reads poorly is theirs to fix and not ours to silently
  // repaint. It is reported rather than enforced — but it is reported, so
  // nobody has to discover it from a visitor.
  if (vendored.length > 0) {
    const themes = new Set(vendored.map((finding) => finding.theme));
    process.stdout.write(
      `\n${vendored.length} contrast findings across ${themes.size} vendored themes ` +
        `(shown as published, not enforced):\n${report(vendored.slice(0, 20))}\n` +
        (vendored.length > 20 ? `…and ${vendored.length - 20} more\n` : ""),
    );
  }

  assert.deepEqual(
    ours,
    [],
    `the site's own themes must be readable:\n${report(ours)}`,
  );
});
