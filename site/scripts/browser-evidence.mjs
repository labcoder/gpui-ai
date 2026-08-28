import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

export function unexpectedBrowserEvents(events) {
  return events.filter(({ kind, detail }) =>
    ["exception", "error", "crash", "network", "http"].includes(kind)
    // A WebGL fallback must not pass as evidence for the WebGPU backend.
    || (kind === "warning" && /WebGPU initialization failed; falling back to WebGL2/.test(detail)),
  );
}

/** Collect browser failures from every frame, including errors a readiness poll cannot see. */
export async function observeBrowser(handle) {
  const { cdp } = handle;
  const events = [];
  const record = (kind, detail) => {
    // Upstream's wasm_assets.rs returns this error while an icon's first
    // request is pending. Keep it in evidence, but it is not a failed fetch;
    // HTTP/network failures are recorded separately and remain fatal.
    // Native-hosted and Linux-built WASM loggers differ in whether the Rust
    // target is included. Match only this known SVG pending diagnostic.
    if (kind === "error" && /^\[ERROR\] (?:gpui::elements::svg)?: Wasm assets loading, will be available soon\.\.\.$/.test(detail)) kind = "asset-pending";
    const previous = events.find((e) => e.kind === kind && JSON.stringify(e.detail) === JSON.stringify(detail));
    if (previous) previous.count += 1;
    else if (events.length < 100) events.push({ kind, detail, count: 1 });
    else if (events.length === 100) events.push({ kind: "error", detail: "Browser event limit exceeded; evidence was truncated", count: 1 });
  };
  cdp.on("Runtime.exceptionThrown", ({ exceptionDetails: e }) => record("exception", e.exception?.description ?? e.text));
  cdp.on("Runtime.consoleAPICalled", ({ type, args }) => {
    if (type === "error" || type === "warning") record(type, args.map((a) => a.value ?? a.description).join(" "));
  });
  cdp.on("Network.loadingFailed", ({ errorText, canceled, type }) => {
    if (!canceled) record("network", { errorText, type });
  });
  cdp.on("Network.responseReceived", ({ response }) => {
    if (response.status >= 400) record("http", { url: response.url, status: response.status });
  });
  cdp.on("Inspector.targetCrashed", () => record("crash", "Renderer crashed"));
  await Promise.all([cdp.send("Runtime.enable"), cdp.send("Network.enable"), cdp.send("Inspector.enable")]);
  handle.events = events;
  return events;
}

// Runs before teardown, on success as well as failure. Each probe is bounded;
// an unresponsive renderer must not prevent saving stderr and command timings.
export async function saveBrowserEvidence(handle, label) {
  if (!handle) return;
  const directory = path.resolve(process.env.GPUI_AI_WEB_ARTIFACTS ?? path.join(root, "target/web-evidence/manual"));
  await mkdir(directory, { recursive: true });
  const name = `${label.replace(/[^a-z0-9-]/gi, "-")}-${Date.now()}`;
  const { cdp } = handle;
  const commands = cdp.commands.map((entry) => ({ ...entry }));
  const probe = (method, params) => cdp.send(method, params, 5_000).catch((error) => ({ error: error.message }));
  const state = await probe("Runtime.evaluate", {
    returnByValue: true,
    expression: `(() => {
      const describe = (w) => {
        try {
          const d = w.document, c = d.querySelector('canvas'), i = d.querySelector('textarea,input');
          return {
            url: w.location.href, ready: d.body?.dataset.ready, failed: d.body?.dataset.failed,
            fallback: d.querySelector('#fallback [data-error]')?.textContent,
            ratio: w.devicePixelRatio,
            canvas: c && { width: c.width, height: c.height, cssWidth: c.clientWidth, cssHeight: c.clientHeight },
            input: i && { focused: d.activeElement === i, readOnly: i.readOnly, length: i.value.length },
            theme: w.gpuiAi?.currentTheme(), storyHeight: w.gpuiAi?.storyHeight(),
            editableFocus: w.__editableFocus,
          };
        } catch (e) { return { error: String(e) }; }
      };
      return [describe(window), ...[...document.querySelectorAll('iframe')].map(f => describe(f.contentWindow))];
    })()`,
  });
  const screenshot = await probe("Page.captureScreenshot", { format: "png", captureBeyondViewport: false });
  if (screenshot.data) await writeFile(path.join(directory, `${name}.png`), Buffer.from(screenshot.data, "base64"));
  await writeFile(path.join(directory, `${name}.json`), JSON.stringify({
    label, node: process.version, platform: process.platform, browser: handle.version,
    executable: handle.browserPath, flags: handle.flags, virtualDisplay: handle.virtualDisplay, adapter: handle.adapter,
    events: handle.events, commands, state: state.result?.value ?? state,
    screenshotError: screenshot.error, stderr: handle.stderr(),
  }, null, 2));
}

/** Inspect the default adapter; also reject GPUI's fallback warning to verify its WebGPU path. */
export async function readGpuAdapter(handle) {
  handle.adapter = await handle.cdp.evaluate(`(async () => {
    const adapter = await navigator.gpu?.requestAdapter();
    if (!adapter) return null;
    const i = adapter.info;
    return { vendor: i.vendor, architecture: i.architecture, device: i.device,
      description: i.description, isFallbackAdapter: i.isFallbackAdapter ?? adapter.isFallbackAdapter };
  })()`);
  return handle.adapter;
}
