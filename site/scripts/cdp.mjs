// Driving a real browser over the DevTools protocol.
//
// Extracted from site/test/browser.test.mjs so that scripts can use it too —
// site/scripts/capture-posters.mjs drives the same launch, serve, navigate, and
// wait-for-a-condition steps the release gate does. The test file imports these
// and keeps its own self-tests for the timeout and connection-loss paths.
//
// No Playwright: everything here already exists, the awkward parts (the
// DevToolsActivePort race, killing a browser tree on Windows, serving the site
// under its project-page base) are already solved and covered, and a second
// browser driver would not make a WebGPU canvas any more deterministic.

import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { readFile, stat } from "node:fs/promises";
import { createServer } from "node:http";
import path from "node:path";
import { browserBuild } from "../../script/web-browser.mjs";

export function browserFlags({ gpu = process.env.GPUI_AI_WEB_GPU ?? "default", deviceScaleFactor, platform = process.platform } = {}) {
  if (!["default", "software"].includes(gpu)) throw new Error(`Unknown GPUI_AI_WEB_GPU: ${gpu}`);
  if (deviceScaleFactor !== undefined && (!Number.isFinite(deviceScaleFactor) || deviceScaleFactor <= 0)) {
    throw new Error("deviceScaleFactor must be a positive finite number");
  }
  return [
    // Linux's headless compositor can acknowledge WebGPU draws but capture a
    // black canvas. The software profile runs headed inside a private Xvfb.
    ...(gpu === "software" && platform === "linux" ? [] : ["--headless=new"]),
    // Software rendering is a functional CI profile, never a frame-budget benchmark.
    // These flags are used only on our local fixtures, not on untrusted websites.
    ...(gpu === "software" ? [
      "--enable-unsafe-webgpu", "--use-webgpu-adapter=swiftshader",
      "--enable-features=Vulkan", "--use-angle=vulkan", "--use-vulkan=swiftshader", "--disable-vulkan-surface",
    ] : []),
    ...(deviceScaleFactor === undefined ? [] : [`--force-device-scale-factor=${deviceScaleFactor}`]),
  ];
}

let pinnedBrowser;
if (process.env.GPUI_AI_WEB_SYSTEM_BROWSER !== "1") {
  try { pinnedBrowser = browserBuild().executable; } catch { /* System browser on unsupported architectures. */ }
}

export const browserCandidates = process.platform === "win32"
  ? [
      process.env.CHROME_PATH,
      pinnedBrowser,
      "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
      "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
      "C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe",
    ]
  : [
      process.env.CHROME_PATH,
      pinnedBrowser,
      "/usr/bin/google-chrome",
      "/usr/bin/chromium",
      "/usr/bin/chromium-browser",
      "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
      "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
    ];
export const browserPath = browserCandidates.find((candidate) => candidate && existsSync(candidate));

export const delay = (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds));

// Runs every cleanup step even when an earlier one throws, then reports the
// first failure. A browser that refuses to close must not strand the HTTP
// server or the temporary directory behind it.
export async function settleAll(steps) {
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

export async function closeServer(handle) {
  if (!handle) return;
  await new Promise((resolve, reject) => {
    handle.server.close((error) => error ? reject(error) : resolve());
    // A crashed/timed-out renderer can leave an active request behind. Stop
    // accepting connections first, then close those requests so teardown
    // cannot hide the original failure behind a hung test process.
    handle.server.closeAllConnections();
  });
}

export async function serve(directory) {
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
        ".json": "application/json",
        ".svg": "image/svg+xml",
        ".wasm": "application/wasm",
        ".webp": "image/webp",
        ".woff2": "font/woff2",
      }[extension] ?? "application/octet-stream";
      response.setHeader("content-type", contentType);
      response.end(await readFile(file));
    } catch {
      // What GitHub Pages does: an address that names nothing is answered with
      // `404.html` if the site ships one, still under a 404 status. Serving a
      // line of plain text instead would mean the site's own not-found page
      // was the one page nothing here ever looked at.
      response.statusCode = 404;
      try {
        const page = await readFile(path.join(directory, "404.html"));
        response.setHeader("content-type", "text/html");
        response.end(page);
      } catch {
        response.end("not found");
      }
    }
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  return { server, origin: `http://127.0.0.1:${server.address().port}` };
}

export class Cdp {
  constructor(socket) {
    this.socket = socket;
    this.nextId = 1;
    this.pending = new Map();
    this.listeners = new Map();
    this.commands = [];
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
        if (pending) {
          pending.record.durationMs = Date.now() - pending.record.startedAt;
          pending.record.error = message.error?.message;
        }
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
    const record = { method, startedAt: Date.now() };
    this.commands.push(record);
    if (this.commands.length > 100) this.commands.shift();
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        record.durationMs = Date.now() - record.startedAt;
        record.error = "timeout";
        reject(new Error(`CDP ${method} timed out after ${timeoutMs}ms`));
      }, timeoutMs);
      this.pending.set(id, { resolve, reject, timer, record });
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
    if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description ?? result.exceptionDetails.text);
    return result.result.value;
  }

  async navigate(url, width, height, deviceScaleFactor = 1) {
    await this.send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor, mobile: false });
    // Listening before asking, because a page can finish loading before
    // `Page.navigate` resolves. The catch is not optional: if the navigate
    // rejects first, this promise has nobody waiting on it, and an unhandled
    // rejection takes the whole test runner down with an error about a page
    // rather than about the navigation that actually failed.
    const loaded = this.once("Page.loadEventFired");
    loaded.catch(() => {});
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

/**
 * How long to wait for Chromium to write its debugging port.
 *
 * Sixty seconds, which is absurd for a browser starting and is not really
 * about the browser. This runs after a fat-LTO WebAssembly build and a Vite
 * build on a two-core CI runner, against a cold page cache, and twenty seconds
 * was not enough: run 32809933880 timed out on the first launch and then
 * launched a second browser successfully seventeen seconds later, once the
 * binary was warm. A wait that is too short turns a slow machine into a red
 * build; a wait that is too long costs nothing, because the two ways this
 * really fails — the browser exiting, or there being no browser — are both
 * detected immediately below.
 */
const PORT_READY_TIMEOUT_MS = 60_000;

/**
 * How long a clean launcher exit may precede the port file. Chromium's
 * launcher process can hand off to a re-execed browser and exit 0 first; the
 * real browser then writes `DevToolsActivePort` into the same profile a beat
 * later (measured ~250ms; the allowance covers a loaded machine). A launch
 * that truly produced no browser still fails, just this much later.
 */
const LAUNCHER_HANDOFF_GRACE_MS = 15_000;

/** How much of Chromium's own complaint to quote when it will not start. */
const STDERR_KEPT = 4_000;

export async function launchBrowser(userDataDir, { deviceScaleFactor } = {}) {
  if (!browserPath) throw new Error("No browser found; run npm run setup:web-browser");
  if (process.env.CHROME_PATH && !existsSync(process.env.CHROME_PATH)) throw new Error("CHROME_PATH does not exist");
  const flags = [
    ...browserFlags({ deviceScaleFactor }),
    "--remote-debugging-port=0",
    `--user-data-dir=${userDataDir}`,
    "--no-first-run",
    "--no-default-browser-check",
    "about:blank",
  ];
  const virtualDisplay = process.platform === "linux" && process.env.GPUI_AI_WEB_GPU === "software";
  const child = virtualDisplay
    ? spawn("xvfb-run", ["-a", "--server-args=-screen 0 1920x1080x24 -nolisten tcp", browserPath, ...flags], {
        stdio: ["ignore", "ignore", "pipe"], detached: true,
      })
    : spawn(browserPath, flags, { stdio: ["ignore", "ignore", "pipe"] });
  child.browserProcessGroup = virtualDisplay;

  // Kept, rather than discarded. A browser that will not start says why — a
  // missing library, a sandbox it cannot enter, a profile it cannot write —
  // and throwing that away leaves "the port did not become ready", which is
  // the one thing every failure here has in common and the least useful thing
  // to be told.
  let complaint = "";
  child.stderr?.on("data", (chunk) => {
    complaint = `${complaint}${chunk}`.slice(-STDERR_KEPT);
  });
  let exit;
  let exitedAt = 0;
  child.once("error", (error) => {
    exit = `launch error: ${error.message}${virtualDisplay ? "; install xvfb and xauth" : ""}`;
    exitedAt = Date.now();
  });
  child.once("exit", (code, signal) => {
    exit = signal ? `signal ${signal}` : `exit code ${code}`;
    exitedAt = Date.now();
  });

  const reason = () => {
    const said = complaint.trim().split(/\r?\n/).slice(-6).join("\n");
    return [
      exit ? `it stopped with ${exit}` : `it is still running`,
      `browser: ${browserPath}`,
      said ? `it said:\n${said}` : "it said nothing",
    ].join("; ");
  };

  let socket;
  try {
    const portFile = path.join(userDataDir, "DevToolsActivePort");
    let port;
    // Chromium creates DevToolsActivePort before it writes the port into it, so
    // a successful read is not proof of a usable value. Keep waiting until the
    // first line is non-empty, and allow for a cold start on a loaded machine.
    const startedAt = Date.now();
    while (Date.now() - startedAt < PORT_READY_TIMEOUT_MS) {
      try {
        const [line] = (await readFile(portFile, "utf8")).trim().split(/\r?\n/);
        if (line) {
          port = line;
          break;
        }
      } catch {
        // The file only appears once Chromium has bound its debugging port.
      }
      // A browser that dies is never going to write the file, and waiting
      // the rest of the minute to say so helps nobody. But a clean exit is
      // not yet proof there is no browser: Edge's launcher process re-execs
      // the real browser and exits 0 before the grandchild has written the
      // port into our profile, so a zero exit gets a bounded grace instead
      // of an immediate verdict.
      if (exit && exit !== "exit code 0") break;
      if (exit && Date.now() - exitedAt > LAUNCHER_HANDOFF_GRACE_MS) break;
      await delay(50);
    }
    if (!port) {
      const waited = Math.round((Date.now() - startedAt) / 1000);
      throw new Error(`Chromium wrote no DevTools port in ${waited}s: ${reason()}`);
    }
    const target = await fetch(`http://127.0.0.1:${port}/json/new?about:blank`, { method: "PUT" }).then((response) => response.json());
    socket = new WebSocket(target.webSocketDebuggerUrl);
    await new Promise((resolve, reject) => {
      socket.addEventListener("open", resolve, { once: true });
      socket.addEventListener("error", reject, { once: true });
    });
    const cdp = new Cdp(socket);
    if (process.env.GPUI_AI_WEB_CPU_RATE) {
      const rate = Number(process.env.GPUI_AI_WEB_CPU_RATE);
      if (!Number.isFinite(rate) || rate < 1) throw new Error("GPUI_AI_WEB_CPU_RATE must be at least 1");
      await cdp.send("Emulation.setCPUThrottlingRate", { rate });
    }
    const version = await cdp.send("Browser.getVersion");
    if (process.env.GPUI_AI_WEB_CHROME_VERSION && version.product.split("/")[1] !== process.env.GPUI_AI_WEB_CHROME_VERSION) {
      throw new Error(`Browser version mismatch: ${version.product}, expected ${process.env.GPUI_AI_WEB_CHROME_VERSION}`);
    }
    return { child, cdp, socket, version, browserPath, flags, virtualDisplay, stderr: () => complaint };
  } catch (error) {
    socket?.close();
    await stopBrowserProcess(child);
    throw error;
  }
}

export async function stopBrowserProcess(child) {
  if (!child?.pid || child.exitCode !== null || child.signalCode !== null) return;
  const exited = new Promise((resolve) => child.once("exit", resolve));
  if (process.platform === "win32") {
    const killer = spawn("taskkill", ["/pid", String(child.pid), "/T", "/F"], { stdio: "ignore" });
    await Promise.race([new Promise((resolve) => killer.once("exit", resolve)), delay(2_000)]);
  } else {
    // xvfb-run owns both the display and Chrome. Kill its private process
    // group on a failed close; killing only the wrapper strands both children.
    try {
      if (child.browserProcessGroup) process.kill(-child.pid, "SIGKILL");
      else child.kill("SIGKILL");
    } catch (error) {
      if (error.code !== "ESRCH") throw error;
    }
  }
  await Promise.race([exited, delay(2_000)]);
}

export async function closeBrowser(browserHandle) {
  if (!browserHandle) return;
  const { child } = browserHandle;
  const exited = child.exitCode !== null || child.signalCode !== null
    ? Promise.resolve()
    : new Promise((resolve) => child.once("exit", resolve));
  await Promise.race([browserHandle.cdp.send("Browser.close"), delay(1_000)]).catch(() => {});
  // Browser.close acknowledges before the process exits. Give xvfb-run time
  // to remove its display lock and authority directory on a normal shutdown.
  await Promise.race([exited, delay(2_000)]);
  browserHandle.socket.close();
  await stopBrowserProcess(child);
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
export async function waitForValue(
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
export const GALLERY_DIAGNOSIS = `(() => ({
  documentReady: document.readyState,
  siteHydrated: document.documentElement.dataset.siteHydrated !== undefined,
  hasCanvas: Boolean(document.querySelector('canvas')),
  stillStarting: Boolean(document.querySelector('[data-demo-starting]')),
  fallbackVisible: (() => { const f = document.getElementById('fallback'); return Boolean(f && !f.hidden); })(),
  fallbackText: document.querySelector('#fallback [data-error]')?.textContent ?? null,
  hostTheme: document.documentElement.dataset.theme ?? null,
  reportedTheme: (() => { try { return window.gpuiAi?.currentTheme() ?? null; } catch { return 'unreachable'; } })(),
  wasmRequests: performance.getEntriesByType('resource')
    .filter((entry) => entry.name.endsWith('.wasm'))
    .map((entry) => ({ name: entry.name.split('/').pop(), transferred: entry.transferSize, duration: Math.round(entry.duration) })),
}))()`;

// An embed that has painted its WebGPU fallback will never produce a canvas.
export const GALLERY_GAVE_UP = `(() => {
  const fallback = document.getElementById('fallback');
  return fallback && !fallback.hidden
    ? 'the embed rendered its WebGPU fallback: ' + (fallback.querySelector('[data-error]')?.textContent ?? 'no detail')
    : false;
})()`;
