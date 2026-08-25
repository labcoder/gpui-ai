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

export const browserCandidates = process.platform === "win32"
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

export async function launchBrowser(userDataDir) {
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

export async function stopBrowserProcess(child) {
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

export async function closeBrowser(browserHandle) {
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
