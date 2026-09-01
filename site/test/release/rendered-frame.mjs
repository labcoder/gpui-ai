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

/**
 * How many pixels inside `clip` differ from its top-left corner.
 *
 * `visiblePixelsInFrame` stops counting at its floor, because all it has to
 * answer is whether a frame is blank. This one counts to the end, so a caller
 * can say how much of something is there rather than that any of it is.
 */
export async function inkInFrame(cdp, clip) {
  const { data } = await cdp.send(
    "Page.captureScreenshot",
    { format: "png", captureBeyondViewport: false, clip },
    30_000,
  );
  const source = `data:image/png;base64,${data}`;
  return cdp.evaluate(
    `new Promise((resolve, reject) => {
      const image = new Image();
      image.onload = () => {
        const canvas = document.createElement('canvas');
        canvas.width = image.naturalWidth;
        canvas.height = image.naturalHeight;
        const context = canvas.getContext('2d', { willReadFrequently: true });
        if (!context) {
          reject(new Error('Chromium could not inspect the captured frame'));
          return;
        }
        context.drawImage(image, 0, 0);
        const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
        const background = [pixels[0], pixels[1], pixels[2]];
        let ink = 0;
        for (let ix = 0; ix < pixels.length; ix += 4) {
          const delta = Math.max(
            Math.abs(pixels[ix] - background[0]),
            Math.abs(pixels[ix + 1] - background[1]),
            Math.abs(pixels[ix + 2] - background[2]),
          );
          if (delta > 8) ink += 1;
        }
        resolve(ink);
      };
      image.onerror = () => reject(new Error('Chromium could not decode the captured frame'));
      image.src = ${JSON.stringify(source)};
    })`,
    30_000,
  );
}
