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
      const mountedPath = requestPath.startsWith("/manual/") ? requestPath.slice("/manual".length) : requestPath;
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

  async navigate(url, width, height) {
    await this.send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false });
    const loaded = this.once("Page.loadEventFired");
    await this.send("Page.navigate", { url });
    await loaded;
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

// What the release gate needs to see when a specimen never starts: which half
// of the condition failed, whether the module was even served, and what the
// fallback said.
const SPECIMEN_DIAGNOSIS = `(() => {
  const frame = document.querySelector('[data-specimen-frame]');
  if (!frame) return { frame: 'missing' };
  const doc = frame.contentDocument;
  const fallback = doc?.getElementById('fallback');
  return {
    frameSrc: frame.getAttribute('src'),
    documentReady: doc?.readyState ?? null,
    hasCanvas: Boolean(doc?.querySelector('canvas')),
    stillLoading: Boolean(doc?.getElementById('loading')),
    fallbackVisible: Boolean(fallback && !fallback.hidden),
    fallbackText: fallback?.querySelector('[data-error]')?.textContent ?? null,
    hostTheme: doc?.documentElement?.dataset?.theme ?? null,
    reportedTheme: (() => { try { return frame.contentWindow.gpuiAi?.currentTheme() ?? null; } catch { return 'unreachable'; } })(),
    wasmRequests: (() => {
      try {
        return frame.contentWindow.performance
          .getEntriesByType('resource')
          .filter((entry) => entry.name.endsWith('.wasm'))
          .map((entry) => ({ name: entry.name.split('/').pop(), transferred: entry.transferSize, duration: Math.round(entry.duration) }));
      } catch { return 'unreachable'; }
    })(),
  };
})()`;

// A specimen that has painted its WebGPU fallback will never produce a canvas.
const SPECIMEN_GAVE_UP = `(() => {
  const fallback = document.querySelector('[data-specimen-frame]')?.contentDocument?.getElementById('fallback');
  return fallback && !fallback.hidden
    ? 'the specimen rendered its WebGPU fallback: ' + (fallback.querySelector('[data-error]')?.textContent ?? 'no detail')
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

test("real browser covers responsive navigation, search, copy, and semantics", {
  skip: browserPath ? false : "Set CHROME_PATH or install Chrome, Edge, or Chromium to run the browser gate",
  timeout: 30_000,
}, async (context) => {
  const temporaryRoot = await mkdtemp(path.join(tmpdir(), "mighty-browser-"));
  const galleryDir = path.join(temporaryRoot, "gallery");
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
  await createGalleryFixture(galleryDir);
  await buildSite({ galleryDir, outDir });
  serverHandle = await serve(outDir);
  browserHandle = await launchBrowser(userDataDir);
  const { cdp } = browserHandle;
  const baseUrl = `${serverHandle.origin}/manual`;

  const errors = [];
  await Promise.all([cdp.send("Page.enable"), cdp.send("Runtime.enable"), cdp.send("Log.enable"), cdp.send("Accessibility.enable")]);
  cdp.on("Runtime.exceptionThrown", ({ exceptionDetails }) => errors.push(exceptionDetails.text));
  cdp.on("Runtime.consoleAPICalled", ({ type, args }) => {
    if (type === "error") errors.push(args.map((argument) => argument.value ?? argument.description).join(" "));
  });
  cdp.on("Log.entryAdded", ({ entry }) => {
    if (entry.level === "error") errors.push(entry.text);
  });

  for (const width of [360, 768, 1280]) {
    await cdp.navigate(`${baseUrl}/components/records-table/?theme=contrast`, width, 900);
    const layout = await cdp.evaluate(`(() => ({
      clientWidth: document.documentElement.clientWidth,
      scrollWidth: document.documentElement.scrollWidth,
      rail: getComputedStyle(document.querySelector('.desktop-rail')).display,
      toggle: getComputedStyle(document.querySelector('[data-nav-toggle]')).display,
      specimenReachable: document.querySelector('.specimen').getBoundingClientRect().width > 0,
      sourceReachable: document.querySelector('.code-panel').getBoundingClientRect().width > 0
    }))()`);
    assert.equal(layout.scrollWidth, layout.clientWidth);
    assert.equal(layout.specimenReachable, true);
    assert.equal(layout.sourceReachable, true);
    assert.equal(layout.rail, width === 1280 ? "block" : "none");
    assert.equal(layout.toggle, width === 1280 ? "none" : "block");
  }

  await cdp.navigate(`${baseUrl}/components/records-table/?theme=contrast`, 768, 900);
  await cdp.evaluate(`document.querySelector('[data-nav-toggle]').click()`);
  assert.deepEqual(await cdp.evaluate(`(() => ({
    expanded: document.querySelector('[data-nav-toggle]').getAttribute('aria-expanded'),
    focus: document.activeElement.textContent.trim(),
    inert: document.querySelector('main').hasAttribute('inert')
  }))()`), { expanded: "true", focus: "Close", inert: true });
  const recordsSequence = String(components.find(({ slug }) => slug === "records-table").sequence).padStart(2, "0");
  assert.equal(await cdp.evaluate(`document.querySelector('#site-nav-panel [aria-current="page"]').textContent.trim()`), `${recordsSequence}Records table`);
  const drawerAccessibility = await cdp.send("Accessibility.getFullAXTree");
  const drawerCurrent = drawerAccessibility.nodes.filter((node) => node.role?.value === "link" && node.name?.value === `${recordsSequence} Records table`);
  assert.equal(drawerCurrent.length, 1);
  await cdp.key("Tab", "Tab", 9, 8);
  assert.match(await cdp.evaluate(`document.activeElement.textContent.trim()`), /Insight card/);
  await cdp.key("Tab", "Tab", 9);
  assert.equal(await cdp.evaluate(`document.activeElement.textContent.trim()`), "Close");
  await cdp.key("Escape", "Escape", 27);
  assert.deepEqual(await cdp.evaluate(`(() => ({
    expanded: document.querySelector('[data-nav-toggle]').getAttribute('aria-expanded'),
    focus: document.activeElement.textContent.trim(),
    inert: document.querySelector('main').hasAttribute('inert')
  }))()`), { expanded: "false", focus: "Index", inert: false });
  await cdp.evaluate(`document.querySelector('[data-nav-toggle]').click(); document.querySelector('.nav-backdrop').click()`);
  assert.equal(await cdp.evaluate(`document.querySelector('[data-nav-toggle]').getAttribute('aria-expanded')`), "false");

  await cdp.navigate(`${baseUrl}/components/records-table/?theme=contrast`, 768, 900);
  await cdp.key("Tab", "Tab", 9);
  assert.equal(await cdp.evaluate(`document.activeElement.className`), "skip-link");
  await cdp.key("Enter", "Enter", 13);
  assert.deepEqual(await cdp.evaluate(`({ id: document.activeElement.id, hash: location.hash })`), { id: "content", hash: "#content" });

  for (const component of components) {
    await cdp.navigate(`${baseUrl}/components/${component.slug}/?theme=contrast`, 768, 900);
    await delay(40);
    assert.equal(await cdp.evaluate(`(() => { const button = document.querySelector('[data-copy]'); button.scrollIntoView({ block: 'center' }); button.focus(); return document.activeElement === button; })()`), true);
    await cdp.key(" ", "Space", 32);
    let status = "";
    for (let attempt = 0; attempt < 20 && !status; attempt += 1) {
      await delay(25);
      status = await cdp.evaluate(`document.querySelector('.copy-status').textContent`);
    }
    assert.match(status, /clipboard|copy it manually/, component.slug);
  }
  assert.equal(await cdp.evaluate(`document.querySelectorAll('iframe[title]').length`), 1);

  await cdp.navigate(`${baseUrl}/components/records-table/?theme=contrast`, 768, 900);
  await cdp.evaluate(`document.querySelector('[rel="next"]').focus()`);
  let navigated = cdp.once("Page.loadEventFired");
  await cdp.key("Enter", "Enter", 13);
  await navigated;
  assert.equal(await cdp.evaluate(`location.pathname`), "/manual/components/diff-table/");
  await cdp.navigate(`${baseUrl}/components/records-table/?theme=contrast`, 768, 900);
  await cdp.evaluate(`document.querySelector('[rel="prev"]').focus()`);
  navigated = cdp.once("Page.loadEventFired");
  await cdp.key("Enter", "Enter", 13);
  await navigated;
  assert.equal(await cdp.evaluate(`location.pathname`), "/manual/components/fine-tune/");

  await cdp.navigate(`${baseUrl}/components/records-table/?theme=contrast`, 768, 900);
  await cdp.evaluate(`document.querySelector('[data-copy]').focus()`);
  await cdp.key("Enter", "Enter", 13);
  await delay(50);
  const mobileAccessibility = await cdp.send("Accessibility.getFullAXTree");
  const axNodes = mobileAccessibility.nodes;
  const byRoleAndName = (role, name) => axNodes.filter((node) => node.role?.value === role && node.name?.value === name);
  for (const [role, name] of [["button", "LIGHT"], ["button", "DARK"], ["button", "CONTRAST"], ["button", "INDEX"], ["button", "RELOAD"], ["button", "Copy"], ["Iframe", "Interactive Records table example"]]) {
    assert.equal(byRoleAndName(role, name).length, 1, `${role} ${name}`);
  }
  const contrastNode = byRoleAndName("button", "CONTRAST")[0];
  assert.equal(contrastNode.properties.find(({ name }) => name === "pressed")?.value.value, "true");
  const indexNode = byRoleAndName("button", "INDEX")[0];
  assert.equal(indexNode.properties.find(({ name }) => name === "expanded")?.value.value, false);
  const liveStatus = axNodes.find((node) => node.role?.value === "status");
  assert.equal(liveStatus?.properties.find(({ name }) => name === "live")?.value.value, "polite");

  await cdp.navigate(`${baseUrl}/components/records-table/?theme=contrast`, 1280, 900);
  const accessibility = await cdp.send("Accessibility.getFullAXTree");
  const roles = accessibility.nodes.map((node) => node.role?.value);
  assert.ok(roles.includes("main"));
  assert.ok(roles.includes("banner"));
  assert.ok(roles.includes("complementary"));
  assert.ok(roles.includes("Iframe"));
  assert.equal(accessibility.nodes.filter((node) => node.role?.value === "complementary" && node.name?.value === "Component catalog").length, 1);

  await cdp.navigate(`${baseUrl}/components/`, 768, 900);
  await cdp.evaluate(`document.querySelector('[data-catalog-search]').focus()`);
  await cdp.send("Input.insertText", { text: "Data tables" });
  assert.deepEqual(await cdp.evaluate(`(() => ({
    count: [...document.querySelectorAll('[data-catalog-item]')].filter((item) => !item.hidden).length,
    status: document.querySelector('[data-catalog-status]').textContent,
    groups: [...document.querySelectorAll('.catalog-group')].filter((group) => !group.hidden).length
  }))()`), { count: 4, status: "4 components", groups: 1 });
  const catalogAccessibility = await cdp.send("Accessibility.getFullAXTree");
  const searchboxes = catalogAccessibility.nodes.filter((node) => node.role?.value === "searchbox");
  assert.equal(searchboxes.filter((node) => node.name?.value === "FIND A PATTERN").length, 1, JSON.stringify(searchboxes.map((node) => node.name?.value)));
  assert.equal(catalogAccessibility.nodes.find((node) => node.role?.value === "status")?.properties.find(({ name }) => name === "live")?.value.value, "polite");

  for (const width of [360, 1280]) {
    await cdp.navigate(`${baseUrl}/`, width, 900);
    const homeLayout = await cdp.evaluate(`(() => ({
      width: innerWidth,
      clientWidth: document.documentElement.clientWidth,
      scrollWidth: document.documentElement.scrollWidth,
      noOverflow: document.documentElement.scrollWidth === document.documentElement.clientWidth,
      metadata: document.querySelectorAll('.build-metadata dd').length,
      architecture: document.querySelector('.architecture-strip').getBoundingClientRect().width > 0,
      repositoryLinks: document.querySelectorAll('footer nav a').length,
      offenders: [...document.querySelectorAll('body *')].filter((item) => item.getBoundingClientRect().right > document.documentElement.clientWidth + 1).slice(0, 5).map((item) => item.className || item.tagName)
    }))()`);
    assert.equal(homeLayout.noOverflow, true, JSON.stringify(homeLayout));
    assert.equal(homeLayout.metadata, 4);
    assert.equal(homeLayout.architecture, true);
    assert.equal(homeLayout.repositoryLinks, 3);
  }
  await cdp.evaluate(`document.querySelector('[data-catalog-search]').focus()`);
  await cdp.send("Input.insertText", { text: "Data tables" });
  assert.deepEqual(await cdp.evaluate(`(() => ({
    count: [...document.querySelectorAll('[data-catalog-item]')].filter((item) => !item.hidden).length,
    status: document.querySelector('[data-catalog-status]').textContent
  }))()`), { count: 4, status: "4 components" });
  assert.equal(await cdp.evaluate(`document.querySelectorAll('[data-featured-specimen] iframe[title]').length`), 3);
  assert.deepEqual(await cdp.evaluate(`(() => [...document.querySelectorAll('[data-featured-specimen] iframe')].map((frame) => frame.dataset.galleryBase))()`), [
    `${baseUrl}/gallery/embed.html`, `${baseUrl}/gallery/embed.html`, `${baseUrl}/gallery/embed.html`,
  ]);
  assert.deepEqual(errors, []);
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
  cdp.on("Runtime.exceptionThrown", ({ exceptionDetails }) => errors.push(exceptionDetails.text));
  cdp.on("Runtime.consoleAPICalled", ({ type, args }) => {
    if (type === "error") errors.push(args.map((argument) => argument.value ?? argument.description).join(" "));
  });
  cdp.on("Log.entryAdded", ({ entry }) => {
    if (entry.level === "error") errors.push(entry.text);
  });

  await cdp.navigate(`${baseUrl}/components/loading/?theme=light`, 1280, 900);
  await waitForValue(cdp, `(() => {
    const frame = document.querySelector('[data-specimen-frame]');
    return Boolean(frame?.contentDocument?.querySelector('canvas') && !frame.contentDocument.getElementById('loading') && frame.contentWindow.gpuiAi?.currentTheme() === 'light');
  })()`, {
    label: "the loading specimen to start and report the light theme",
    fatal: SPECIMEN_GAVE_UP,
    describe: SPECIMEN_DIAGNOSIS,
    errors,
  });
  assert.equal(await cdp.evaluate(`document.querySelector('[data-specimen-frame]').contentDocument.documentElement.dataset.theme`), "light");
  assert.equal(await cdp.evaluate(`document.querySelector('[data-specimen-frame]').contentWindow.gpuiAi.currentTheme()`), "light");

  for (const theme of ["dark", "contrast", "dark"]) {
    assert.equal(await cdp.evaluate(`(() => {
      const button = document.querySelector('[data-theme-choice="${theme}"]');
      button.focus();
      return document.activeElement === button;
    })()`), true);
    await cdp.key(" ", "Space", 32);
    await waitForValue(cdp, `(() => {
      const frame = document.querySelector('[data-specimen-frame]');
      return frame?.contentDocument?.documentElement.dataset.theme === '${theme}' && frame?.contentWindow?.gpuiAi?.currentTheme() === '${theme}';
    })()`, {
      label: `the specimen to follow the ${theme} theme`,
      fatal: SPECIMEN_GAVE_UP,
      describe: SPECIMEN_DIAGNOSIS,
      errors,
    });
    assert.equal(await cdp.evaluate(`document.documentElement.dataset.theme`), theme);
  }

  await cdp.evaluate(`window.scrollTo(0, document.documentElement.scrollHeight)`);
  await waitForValue(cdp, `!document.querySelector('[data-specimen-frame]').hasAttribute('src')`, {
    label: "the offscreen specimen to unload",
    describe: SPECIMEN_DIAGNOSIS,
    errors,
  });
  await cdp.evaluate(`document.querySelector('[data-specimen-frame]').scrollIntoView({ block: 'center' })`);
  await waitForValue(cdp, `(() => {
    const frame = document.querySelector('[data-specimen-frame]');
    return Boolean(frame?.contentDocument?.querySelector('canvas') && frame?.contentWindow?.gpuiAi?.currentTheme() === 'dark');
  })()`, {
    label: "the specimen to restore after scrolling back",
    fatal: SPECIMEN_GAVE_UP,
    describe: SPECIMEN_DIAGNOSIS,
    errors,
  });
  assert.equal(await cdp.evaluate(`document.querySelector('[data-specimen-frame]').contentDocument.documentElement.dataset.theme`), "dark");
  assert.equal(await cdp.evaluate(`document.querySelector('[data-specimen-frame]').contentWindow.gpuiAi.currentTheme()`), "dark");
  assert.deepEqual(errors, []);

  await cdp.send("Page.addScriptToEvaluateOnNewDocument", {
    source: `Object.defineProperty(Navigator.prototype, 'gpu', { configurable: true, get: () => undefined });`,
  });
  await cdp.navigate(`${baseUrl}/components/diff-table/?theme=contrast`, 1280, 900);
  await waitForValue(cdp, `(() => {
    const frame = document.querySelector('[data-specimen-frame]');
    const fallback = frame?.contentDocument?.getElementById('fallback');
    return Boolean(frame?.hasAttribute('src') && fallback && !fallback.hidden);
  })()`, {
    label: "the specimen to show its WebGPU fallback",
    describe: SPECIMEN_DIAGNOSIS,
    errors,
  });
  assert.deepEqual(await cdp.evaluate(`(() => ({
    parentFallbacks: document.querySelectorAll('.webgpu-fallback').length,
    sourceVisible: document.querySelector('.code-panel').getBoundingClientRect().height > 0,
    childMessage: document.querySelector('[data-specimen-frame]').contentDocument.querySelector('#fallback [data-error]').textContent
  }))()`), {
    parentFallbacks: 0,
    sourceVisible: true,
    childMessage: "This live example requires a browser with WebGPU support.",
  });
  assert.deepEqual(errors, []);
});
