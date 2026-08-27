import assert from "node:assert/strict";
import { cp, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { closeBrowser, closeServer, launchBrowser, serve, settleAll, waitForValue } from "../../scripts/cdp.mjs";
import { observeBrowser, readGpuAdapter, saveBrowserEvidence, unexpectedBrowserEvents } from "../../scripts/browser-evidence.mjs";
import { assertDraws } from "./rendered-frame.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../..");
const INPUT_TIMEOUT_MS = 30_000;

// Independent tests: a stalled case must not cancel the other densities or
// close their browser underneath a still-running touch command.
for (const [ratio, fallback] of [[2, false], [3, false], [3, true]]) {
  const name = `${ratio}x${fallback ? " CSS-box fallback" : " device-pixel box"}`;
  test(`mobile ${name}: device pixels, touch activation, and editing`, { timeout: 120_000 }, async (context) => {
    const temporaryRoot = await mkdtemp(path.join(tmpdir(), "gpui-ai-mobile-"));
    let serverHandle, browserHandle, phase = "boot";
    context.after(async () => settleAll([
      () => saveBrowserEvidence(browserHandle, `mobile-${name}-${phase}`),
      () => closeBrowser(browserHandle),
      () => closeServer(serverHandle),
      () => rm(temporaryRoot, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 }),
    ]));
    await cp(path.join(root, "crates/gallery-web/www/dist"), path.join(temporaryRoot, "gallery"), { recursive: true });
    await writeFile(path.join(temporaryRoot, "index.html"), `<!doctype html>
      <meta name="viewport" content="width=device-width, initial-scale=1">
      <style>body { margin:0 } iframe { display:block; width:100%; height:844px; border:0 }</style>
      <iframe src="./gallery/embed.html?story=approval&theme=sunday-panel"></iframe>`);
    serverHandle = await serve(temporaryRoot);

    // CDP-only DPR emulation does not scale ResizeObserver's device-pixel box.
    // Both APIs must agree. The fallback covers the API path used by Safari,
    // not Safari's event dispatch or its physical on-screen keyboard.
    browserHandle = await launchBrowser(path.join(temporaryRoot, "browser"), { deviceScaleFactor: ratio });
    const { cdp } = browserHandle;
    const events = await observeBrowser(browserHandle);
    await cdp.send("Page.enable");
    await cdp.send("Emulation.setTouchEmulationEnabled", { enabled: true, maxTouchPoints: 5 });
    await cdp.send("Page.addScriptToEvaluateOnNewDocument", { source: `
      ${fallback ? "delete ResizeObserverEntry.prototype.devicePixelContentBoxSize;" : ""}
      window.__editableFocus = [];
      document.addEventListener('focus', e => {
        if (e.target.matches?.('textarea,input')) window.__editableFocus.push(!e.target.readOnly);
      }, true);` });
    const frame = "document.querySelector('iframe').contentWindow";
    const inFrame = (expression, timeoutMs) => cdp.evaluate(`(() => { const w = ${frame}; const d = w.document; ${expression} })()`, timeoutMs);
    const inputExpression = `(() => { const w = ${frame}, d = w.document, i = d.querySelector('textarea,input'); return {
      readOnly: i?.readOnly, focused: d.activeElement === i, value: i?.value, editableFocus: w.__editableFocus
    }; })()`;
    const wait = (expression, label) => waitForValue(cdp, expression, {
      label: `${name}: ${label}`,
      fatal: `${frame}.document.body?.dataset.failed !== undefined && 'gallery fallback'`,
      describe: `({ url: ${frame}.location.href, input: ${inputExpression}, errors: ${JSON.stringify(events)} })`,
    });
    const ready = (story) => wait(
      `Boolean(${frame}.location.search.includes('story=${story}') && ${frame}.document.body?.dataset.ready !== undefined && ${frame}.gpuiAi?.storyHeight() > 0)`,
      `${story} ready with measured content`,
    );
    const geometry = () => inFrame(`const c = d.querySelector('canvas'); return {
      ratio: w.devicePixelRatio, width: c.width, height: c.height, cssWidth: c.clientWidth, cssHeight: c.clientHeight
    };`);
    const tap = async (x, y) => {
      // Input commands wait for Chromium's renderer acknowledgement. At 3x on
      // a software adapter that can exceed the generic 5s CDP control budget.
      await cdp.send("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [{ x, y }] }, INPUT_TIMEOUT_MS);
      // Cross a paint boundary, not an assumed 80ms frame. GPUI installs the
      // focused input handler during paint after processing pointer-down.
      await inFrame("return new Promise(resolve => w.requestAnimationFrame(() => w.requestAnimationFrame(resolve)));", INPUT_TIMEOUT_MS);
      await cdp.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] }, INPUT_TIMEOUT_MS);
    };

    await cdp.navigate(`${serverHandle.origin}/`, 390, 844, ratio);
    await cdp.send("Emulation.setDeviceMetricsOverride", { width: 390, height: 844, deviceScaleFactor: ratio, mobile: true });
    await ready("approval");
    const adapter = await readGpuAdapter(browserHandle);
    assert.ok(adapter, "the release test requires a working WebGPU adapter");
    if (process.env.GPUI_AI_WEB_GPU === "software") {
      assert.match(`${adapter.architecture} ${adapter.description}`, /swiftshader/i, "the software profile must select a software adapter");
    }

    phase = "portrait-geometry";
    const size = await geometry();
    assert.equal(size.ratio, ratio, "the embed must not falsify devicePixelRatio");
    assert.equal(size.width, size.cssWidth * ratio);
    assert.equal(size.height, size.cssHeight * ratio);
    const height = await inFrame("return w.gpuiAi.storyHeight();");
    assert.ok(height > 400 && height < 500, `390px Sunday Panel fixture CSS height was ${height}`);

    // The full portrait backing store is checked above. Input needs only the
    // first card/composer: don't shade three million pixels on every tap just
    // to prove focus. This is still the real story, at the real 2x/3x DPR.
    await cdp.evaluate("document.querySelector('iframe').style.height = '300px'");
    await wait(`${frame}.document.querySelector('canvas').height === ${300 * ratio}`, "compact input viewport");
    // Clip to the embed: the surrounding white page cannot make a black
    // WebGPU surface look like a successful, non-uniform screenshot.
    await assertDraws(cdp, name, { x: 0, y: 0, width: 390, height: 300, scale: 1 });
    phase = "non-input";
    await tap(8, 280); // Blank canvas margin, outside either card.
    await wait(`(${inputExpression}).readOnly && !(${inputExpression}).focused`, "non-input tap releases IME");
    assert.deepEqual((await cdp.evaluate(inputExpression)).editableFocus, [], "no transient editable focus on a non-input tap");

    phase = "approval";
    assert.equal(await inFrame("return w.gpuiAi.isApprovalGranted('gate');"), false);
    // GPUI's pinned web backend does not expose an accessibility tree. These
    // CSS coordinates target the fixed, measured fixture, not a handler call.
    await tap(67, 217);
    await wait(`${frame}.gpuiAi.isApprovalGranted('gate')`, "real pointer approves the gate");
    assert.equal(await inFrame("return w.gpuiAi.isApprovalGranted('purge');"), false, "the other gate is untouched");
    assert.deepEqual((await cdp.evaluate(inputExpression)).editableFocus, [], "buttons must not summon the keyboard");
    assert.equal(await inFrame("return w.gpuiAi.reset();"), true);
    await wait(`!${frame}.gpuiAi.isApprovalGranted('gate')`, "reset restores the pending decision");

    phase = "editing";
    await cdp.evaluate(`document.querySelector('iframe').src = './gallery/embed.html?story=prompt-bar&theme=sunday-panel'`);
    await ready("prompt-bar");
    await tap(100, 73);
    await wait(`(${inputExpression}).focused && !(${inputExpression}).readOnly`, "composer opens an editable IME session");
    await cdp.send("Input.insertText", { text: "Mobile gyjpq ﬁ" }, INPUT_TIMEOUT_MS);
    await wait(`(${inputExpression}).value === 'Mobile gyjpq ﬁ'`, "mobile text reaches the real composer");
    await tap(8, 200);
    await wait(`(${inputExpression}).readOnly && !(${inputExpression}).focused`, "leaving the composer closes the IME session");
    await tap(100, 73);
    await wait(`(${inputExpression}).focused && !(${inputExpression}).readOnly`, "re-entry reopens the IME session");
    assert.equal((await cdp.evaluate(inputExpression)).value, "Mobile gyjpq ﬁ", "blur/re-entry preserves the draft");

    phase = "resize";
    await cdp.send("Emulation.setDeviceMetricsOverride", { width: 320, height: 844, deviceScaleFactor: ratio, mobile: true });
    await wait(`${frame}.document.querySelector('canvas').width === ${320 * ratio}`, "resized high-density canvas");
    assert.equal((await geometry()).cssWidth, 320);
    assert.deepEqual(unexpectedBrowserEvents(events), [], "mobile runs without browser errors or a WebGL fallback");
    phase = "passed";
  });
}
