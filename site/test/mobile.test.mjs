import assert from "node:assert/strict";
import { cp, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import {
  browserPath, closeBrowser, delay, launchBrowser, serve, settleAll, waitForValue,
} from "../scripts/cdp.mjs";

const requested = process.env.GPUI_AI_RELEASE_INTEGRATION === "1";
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

test("release WASM mobile preserves device pixels and touch intent", {
  skip: !requested ? "Run npm run check:web:release for the built-artifact gate"
    : !browserPath && process.env.CI !== "true" ? "No Chromium browser installed" : false,
  timeout: 120_000,
}, async (context) => {
  assert.ok(browserPath, "the mobile release gate needs a real browser");
  const temporaryRoot = await mkdtemp(path.join(tmpdir(), "gpui-ai-mobile-"));
  let serverHandle;
  let browserHandle;
  context.after(async () => settleAll([
    () => closeBrowser(browserHandle),
    () => serverHandle && new Promise((resolve) => serverHandle.server.close(resolve)),
    () => rm(temporaryRoot, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 }),
  ]));
  await cp(path.join(root, "crates/gallery-web/www/dist"), path.join(temporaryRoot, "gallery"), { recursive: true });
  await writeFile(path.join(temporaryRoot, "index.html"), `<!doctype html>
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <style>body { margin:0 } iframe { display:block; width:100%; height:844px; border:0 }</style>
    <iframe src="./gallery/embed.html?story=approval&theme=sunday-panel"></iframe>`);
  serverHandle = await serve(temporaryRoot);

  // CDP-only DPR emulation does not scale ResizeObserver's device-pixel box.
  // Launch at the real scale too, rather than testing mismatched browser APIs.
  // The last case covers gpui_web's Safari path (no device-pixel box API).
  for (const [ratio, fallback] of [[2, false], [3, false], [3, true]]) {
    const name = `${ratio}x${fallback ? " CSS-box fallback" : " device-pixel box"}`;
    browserHandle = await launchBrowser(path.join(temporaryRoot, `browser-${ratio}-${fallback}`), { deviceScaleFactor: ratio });
    const { cdp } = browserHandle;
    await Promise.all([cdp.send("Page.enable"), cdp.send("Runtime.enable")]);
    await cdp.send("Emulation.setTouchEmulationEnabled", { enabled: true, maxTouchPoints: 5 });
    await cdp.send("Page.addScriptToEvaluateOnNewDocument", { source: `
      ${fallback ? "delete ResizeObserverEntry.prototype.devicePixelContentBoxSize;" : ""}
      window.__editableFocus = [];
      document.addEventListener('focus', e => {
        if (e.target.matches?.('textarea,input')) window.__editableFocus.push(!e.target.readOnly);
      }, true);` });
    const frame = "document.querySelector('iframe').contentWindow";
    const inFrame = (expression) => cdp.evaluate(`(() => { const w = ${frame}; const d = w.document; ${expression} })()`);
    const inputState = () => inFrame(`const i = d.querySelector('textarea,input'); return {
      readOnly: i.readOnly, focused: d.activeElement === i, value: i.value, editableFocus: w.__editableFocus
    };`);
    const tap = async (x, y) => {
      await cdp.send("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [{ x, y }] });
      // A real tap spans a draw, allowing GPUI to install the tapped input handler.
      await delay(80);
      await cdp.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
      await delay(120);
    };
    const ready = (story) => waitForValue(cdp,
      `Boolean(${frame}?.location.search.includes('story=${story}') && ${frame}.document.body?.dataset.ready !== undefined)`,
      { label: `${name} ${story} first frame` });
    const geometry = () => inFrame(`const c = d.querySelector('canvas'); return {
      ratio: w.devicePixelRatio, width: c.width, height: c.height, cssWidth: c.clientWidth, cssHeight: c.clientHeight
    };`);
    await cdp.navigate(`${serverHandle.origin}/`, 390, 844, ratio);
    await cdp.send("Emulation.setDeviceMetricsOverride", { width: 390, height: 844, deviceScaleFactor: ratio, mobile: true });
    await ready("approval");
    await context.test(`${name}: sharp backing store and logical layout`, async () => {
      const size = await geometry();
      assert.equal(size.ratio, ratio, "the embed must not falsify devicePixelRatio");
      assert.equal(size.width, size.cssWidth * ratio);
      assert.equal(size.height, size.cssHeight * ratio);
      // Wrong physical/logical conversion wraps this to 742px at 2x, 1235px at 3x.
      const height = await inFrame("return w.gpuiAi.storyHeight();");
      assert.ok(height > 400 && height < 500, `CSS layout height was ${height}`);
    });
    await context.test(`${name}: non-input taps never take editable focus`, async () => {
      await tap(195, 700); // Blank canvas below both approval cards.
      const input = await inputState();
      assert.equal(input.readOnly, true);
      assert.equal(input.focused, false);
      assert.deepEqual(input.editableFocus, [], "no transient editable focus on a non-input tap");
    });
    await context.test(`${name}: touch activates approval at its CSS coordinates`, async () => {
      const before = await inFrame("return w.gpuiAi.storyHeight();");
      await tap(67, 217); // Approve in the pinned 390px, 14px-rem fixture.
      const after = await inFrame("return w.gpuiAi.storyHeight();");
      assert.notEqual(after, before, "the decision must replace its action row with a note");
      assert.deepEqual((await inputState()).editableFocus, [], "buttons must not summon the keyboard");
      await inFrame("w.gpuiAi.reset();");
      await waitForValue(cdp, `${frame}.gpuiAi.storyHeight() === ${before}`, { label: "approval reset restores the pending layout" });
    });
    await context.test(`${name}: editable taps still focus and accept text`, async () => {
      await cdp.evaluate(`document.querySelector('iframe').src = './gallery/embed.html?story=prompt-bar&theme=sunday-panel'`);
      await ready("prompt-bar");
      await tap(100, 73); // First composer input, not its Send button.
      assert.equal((await inputState()).focused, true);
      assert.equal((await inputState()).readOnly, false);
      await cdp.send("Input.insertText", { text: "Mobile gyjpq ﬁ" });
      await waitForValue(cdp, `${frame}.document.querySelector('textarea').value === 'Mobile gyjpq ﬁ'`, { label: "mobile text reaches the real composer" });
      await tap(8, 200); // Outside the composer, in the canvas margin.
      assert.equal((await inputState()).readOnly, true);
      assert.equal((await inputState()).focused, false);
      await tap(100, 73);
      assert.equal((await inputState()).focused, true, "the next edit must reopen the IME session");
      assert.equal((await inputState()).value, "Mobile gyjpq ﬁ", "blur/re-entry preserves the draft");
    });
    await context.test(`${name}: resize preserves device pixels`, async () => {
      await cdp.send("Emulation.setDeviceMetricsOverride", { width: 320, height: 844, deviceScaleFactor: ratio, mobile: true });
      await waitForValue(cdp, `${frame}.document.querySelector('canvas').width === ${320 * ratio}`, { label: "resized high-density canvas" });
      assert.equal((await geometry()).cssWidth, 320);
    });
    await closeBrowser(browserHandle);
    browserHandle = undefined;
  }
});
