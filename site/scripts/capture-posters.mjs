// Posters: a still of every story, rendered by the real thing.
//
// A demo is a seventeen-megabyte WASM binary drawing through WebGPU, and there
// are three moments where that is not what a reader has:
//
//   - the browser has no WebGPU at all, so the demo will never run and the
//     card in its place is the only thing that reader will ever see;
//   - the frame is idle, two viewports down, deliberately not loading;
//   - a link to a component page is shared, and the preview card needs a
//     picture (S-14).
//
// So render each story once at build time and keep the frame. Not a mockup and
// not a screenshot taken by hand: the same gallery binary the site ships,
// driven through the same DevTools harness the release gate uses, so a poster
// cannot drift from what the component actually draws.
//
// Two per story, `light` and `dark`. Not one per theme: 45 themes would be
// 1,575 files to make a placeholder marginally more accurate, and the site
// only shows a poster where the colour cannot be wrong — see Demo.tsx.
//
// Deliberately not part of `npm run generate`. CI runs generate and then
// `git diff --exit-code`, and a GPU-rendered WebP is not byte-reproducible:
// two runs of the same story differ in the low bits and the check would fail
// on work that changed nothing. The output is git-ignored and rebuilt.

import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import {
  browserPath,
  closeBrowser,
  closeServer,
  delay,
  GALLERY_DIAGNOSIS,
  GALLERY_GAVE_UP,
  launchBrowser,
  serve,
  settleAll,
  waitForValue,
} from "./cdp.mjs";
import { observeBrowser, saveBrowserEvidence } from "./browser-evidence.mjs";
import catalog from "../generated/catalog.json" with { type: "json" };

const here = path.dirname(fileURLToPath(import.meta.url));
const repositoryRoot = path.resolve(here, "../..");
const galleryDir = path.join(repositoryRoot, "crates/gallery-web/www/dist");
const posterDir = path.join(repositoryRoot, "site/public/posters");

/** The width every story height in `catalog.json` was measured at. */
export const POSTER_WIDTH = 900;

/** The two themes a poster can be shown under without ever being wrong. */
export const POSTER_MODES = ["light", "dark"];

/**
 * The stories that get one, in the order the site lists them.
 *
 * The hero belongs here too: it is the first demo on the site and the one a
 * shared link most often points at, even though it is site-only and so is not
 * in the component catalog.
 */
export function posterStories() {
  const trio = ["loading", "tool-chips", "context"];
  return [
    { slug: catalog.hero.slug, height: catalog.hero.height },
    // The themes page's composed specimen: site-only like the hero, so its
    // height is the sum of the three standalone stories it stacks plus the
    // composition's two gaps.
    {
      slug: "themes-trio",
      height: catalog.components
        .filter(({ slug }) => trio.includes(slug))
        .reduce((total, { height }) => total + height, 48),
    },
    // The Extensions section's own demo: site-only like the two above, and
    // the poster a reader without WebGPU sees in place of it. Its height is
    // written here rather than read from the catalog, because a story that
    // documents no component has no catalog row to read it from.
    { slug: "decorations", height: 420 },
    ...catalog.components.map(({ slug, height }) => ({ slug, height })),
  ];
}

/** Where a story's poster is written, relative to `site/public`. */
export const posterFile = (slug, mode) => `posters/${slug}-${mode}.webp`;

// A real component must put more than compression noise on the page. WebP byte
// length cannot answer that: the sparse Loading State frame is 1,958 bytes in
// Linux Chromium and about 2,100 in Windows Edge, despite showing the same
// content. Decode the captured frame in Chromium and count pixels that visibly
// differ from its background instead.
const MINIMUM_VISIBLE_PIXELS = 16;
const VISIBLE_CHANNEL_DELTA = 8;

/** Whether a captured frame contains visible content rather than a solid fill. */
export function posterFrameLooksReal({ encodedBytes, visiblePixels }) {
  return encodedBytes > 0 && visiblePixels >= MINIMUM_VISIBLE_PIXELS;
}

// Half of these stories are still arriving when they first paint: Streaming
// Text is one glyph, Code Block is a bare cursor, a task list has no rows yet.
// `data-ready` means GPUI has drawn, not that the story has anything to say,
// so waiting a fixed moment and capturing produces a picture of an empty box —
// which is how the floor above first fired.
//
// So sample instead of guess. A frame with more in it compresses larger, so
// the biggest sample is the fullest one; a story that has finished arriving
// stops growing, and one that animates for ever (Orbs breathing, a shimmer)
// holds roughly steady, so both settle on the same rule.
// Elapsed since the story first drew. Widening, because a story that is still
// arriving at three seconds is arriving slowly; the early exit below means
// most stories never reach the later ones.
const SAMPLE_AT_MS = [1_200, 1_800, 2_600, 3_600, 4_800, 6_200, 7_800];
// Two samples in a row that add less than this much are a story that has
// stopped arriving. Loose enough that a blinking cursor does not read as
// growth, tight enough that a list still filling in does.
const STILL_GROWING = 1.02;

// A WebGPU canvas at 900px wide takes appreciably longer to hand back than a
// page of markup, and this runs 70 times.
const CAPTURE_TIMEOUT_MS = 30_000;

/**
 * The fullest frame this story produces and its visible-pixel evidence.
 *
 * Keeps the largest sample rather than the last: a story whose animation
 * ends on an empty frame — a reveal that fades out, a toast that clears —
 * would otherwise be published as the empty one.
 */
async function fullestFrame(cdp) {
  let best = Buffer.alloc(0);
  let settled = 0;
  let waited = 0;
  for (const at of SAMPLE_AT_MS) {
    await delay(at - waited);
    waited = at;
    const { data } = await cdp.send(
      "Page.captureScreenshot",
      { format: "webp", quality: 82, captureBeyondViewport: false },
      CAPTURE_TIMEOUT_MS,
    );
    const bytes = Buffer.from(data, "base64");
    const grew = bytes.length > best.length * STILL_GROWING;
    if (bytes.length > best.length) best = bytes;
    settled = grew ? 0 : settled + 1;
    if (settled >= 2) {
      const visiblePixels = await visiblePixelsInFrame(cdp, best);
      if (posterFrameLooksReal({ encodedBytes: best.length, visiblePixels })) {
        return { bytes: best, visiblePixels };
      }
    }
  }
  return { bytes: best, visiblePixels: await visiblePixelsInFrame(cdp, best) };
}

/** Count enough pixels unlike the top-left background to classify the frame. */
export async function visiblePixelsInFrame(cdp, bytes) {
  const source = `data:image/webp;base64,${bytes.toString("base64")}`;
  return cdp.evaluate(
    `new Promise((resolve, reject) => {
      const image = new Image();
      image.onload = () => {
        const canvas = document.createElement('canvas');
        canvas.width = image.naturalWidth;
        canvas.height = image.naturalHeight;
        const context = canvas.getContext('2d', { willReadFrequently: true });
        if (!context) {
          reject(new Error('Chromium could not inspect the captured poster'));
          return;
        }
        context.drawImage(image, 0, 0);
        const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
        const background = [pixels[0], pixels[1], pixels[2], pixels[3]];
        let visiblePixels = 0;
        for (let ix = 0; ix < pixels.length; ix += 4) {
          const delta = Math.max(
            Math.abs(pixels[ix] - background[0]),
            Math.abs(pixels[ix + 1] - background[1]),
            Math.abs(pixels[ix + 2] - background[2]),
            Math.abs(pixels[ix + 3] - background[3]),
          );
          if (delta > ${VISIBLE_CHANNEL_DELTA}) visiblePixels += 1;
          if (visiblePixels >= ${MINIMUM_VISIBLE_PIXELS}) break;
        }
        resolve(visiblePixels);
      };
      image.onerror = () => reject(new Error('Chromium could not decode the captured poster'));
      image.src = ${JSON.stringify(source)};
    })`,
    CAPTURE_TIMEOUT_MS,
  );
}

/**
 * Renders every story twice and writes the posters.
 *
 * Returns one record per file so a caller — a test, or the build log — can see
 * what was written and how big it was without reading the directory back.
 */
export async function capturePosters({ outDir = posterDir, only, log = () => {} } = {}) {
  if (!browserPath) throw new Error("no Chrome or Edge to capture posters with");
  if (!existsSync(path.join(galleryDir, "embed.html"))) {
    throw new Error(`build the web gallery first: ${path.join(galleryDir, "embed.html")} is missing`);
  }

  // `only` narrows to a few slugs. The release gate captures the one story it
  // drives rather than all seventy, so it proves the whole chain — capture,
  // publish, serve, render — in seconds instead of minutes.
  const wanted = only ? new Set(only) : undefined;
  const stories = posterStories().filter(({ slug }) => !wanted || wanted.has(slug));
  if (wanted && stories.length !== wanted.size) {
    throw new Error(`no story called ${[...wanted].filter((slug) => !stories.some((s) => s.slug === slug)).join(", ")}`);
  }
  const temporaryRoot = await mkdtemp(path.join(tmpdir(), "gpui-ai-posters-"));
  let serverHandle;
  let browserHandle;
  const written = [];

  try {
    serverHandle = await serve(galleryDir);
    browserHandle = await launchBrowser(path.join(temporaryRoot, "browser"));
    await observeBrowser(browserHandle);
    const { cdp } = browserHandle;
    await Promise.all([cdp.send("Page.enable"), cdp.send("Runtime.enable")]);
    await mkdir(outDir, { recursive: true });

    for (const { slug, height } of stories) {
      for (const mode of POSTER_MODES) {
        const url = `${serverHandle.origin}/embed.html?story=${slug}&theme=${mode}`;
        await cdp.navigate(url, POSTER_WIDTH, height);
        await waitForValue(
          cdp,
          `'ready' in document.body.dataset && window.gpuiAi?.currentTheme() === ${JSON.stringify(mode)}`,
          {
            label: `${slug} to draw in ${mode}`,
            fatal: GALLERY_GAVE_UP,
            describe: GALLERY_DIAGNOSIS,
          },
        );
        const { bytes, visiblePixels } = await fullestFrame(cdp);
        if (!posterFrameLooksReal({ encodedBytes: bytes.length, visiblePixels })) {
          throw new Error(
            `${slug} in ${mode} captured ${bytes.length} bytes but fewer than ${MINIMUM_VISIBLE_PIXELS} visible pixels, which is a blank frame, not a component`,
          );
        }
        const file = posterFile(slug, mode);
        await writeFile(path.join(outDir, path.basename(file)), bytes);
        written.push({ slug, mode, file, bytes: bytes.length });
        log(`${file} — ${(bytes.length / 1024).toFixed(1)} kB`);
      }
    }
  } catch (error) {
    await saveBrowserEvidence(browserHandle, "poster-capture").catch((diagnosticError) => console.error(diagnosticError));
    throw error;
  } finally {
    await settleAll([
      () => closeBrowser(browserHandle),
      () => closeServer(serverHandle),
      () => rm(temporaryRoot, { force: true, recursive: true, maxRetries: 5, retryDelay: 100 }),
    ]);
  }

  return written;
}

if (process.argv[1] && path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url))) {
  const written = await capturePosters({ log: (line) => console.log(line) });
  const total = written.reduce((sum, entry) => sum + entry.bytes, 0);
  console.log(`${written.length} posters, ${(total / 1024 / 1024).toFixed(2)} MB`);
}
