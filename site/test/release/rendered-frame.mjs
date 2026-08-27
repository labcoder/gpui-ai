import assert from "node:assert/strict";
import { delay } from "../../scripts/cdp.mjs";
import { posterFrameLooksReal, visiblePixelsInFrame } from "../../scripts/capture-posters.mjs";

// Read pixels from the compositor, not a DOM "ready" flag. In particular,
// Linux headless WebGPU can process input while presenting only black frames.
export async function assertDraws(cdp, label, clip) {
  await cdp.evaluate("new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)))", 30_000);
  const deadline = Date.now() + 20_000;
  do {
    const { data } = await cdp.send("Page.captureScreenshot", {
      format: "webp", captureBeyondViewport: false, ...(clip ? { clip } : {}),
    }, 30_000);
    const bytes = Buffer.from(data, "base64");
    const visiblePixels = await visiblePixelsInFrame(cdp, bytes);
    if (posterFrameLooksReal({ encodedBytes: bytes.length, visiblePixels })) return;
    await delay(100);
  } while (Date.now() < deadline);
  assert.fail(`${label} produced only blank frames`);
}
