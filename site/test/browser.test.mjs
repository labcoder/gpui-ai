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
      const rail = document.querySelector('.component-rail');
      return {
        background: body.backgroundColor,
        foreground: body.color,
        border: rail ? getComputedStyle(rail).borderTopColor : null,
        radius: rail ? getComputedStyle(rail).borderTopLeftRadius : null,
        face: body.fontFamily,
      };
    };
    const before = read();
    document.documentElement.dataset.theme = 'ember-dusk';
    const after = read();
    document.documentElement.removeAttribute('data-theme');
    const restored = read();
    return { before, after, restored };
  })()`);
  for (const property of ["background", "foreground", "border"]) {
    assert.notEqual(
      repaint.after[property],
      repaint.before[property],
      `switching data-theme left ${property} at ${repaint.before[property]}`,
    );
  }
  assert.deepEqual(repaint.restored, repaint.before, "removing data-theme must restore the default");
  // The face comes from a token too, so a theme that changed it would move the
  // chrome and the demos together.
  assert.match(repaint.before.face, /IBM Plex Sans/);

  // On a phone the rail stops being a sidebar, but it is the only place the
  // page carries the rustdoc link, the source link, and the reference table.
  // Laying it out with `display: none` below a breakpoint would take all of
  // that off every small screen while leaving the markup in place, which every
  // HTML-level assertion would happily match.
  await cdp.navigate(`${serverHandle.origin}/gpui-ai/components/${specimen.slug}/`, 390, 844);
  const railOnMobile = await cdp.evaluate(`(() => {
    const link = document.querySelector('.component-rail a[href*="/api/gpui_ai/"]');
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
