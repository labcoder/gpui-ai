// The picture a link to this site turns into.
//
// A shared URL with no card is a line of grey text in a chat window. Every
// page now claims one in its `og:image`, and this is what writes the files
// those tags name: one 1200x630 card per route, rendered by the browser out of
// the site's own stylesheet, so the type, the palette and the radii are the
// ones the page itself is made of rather than a designer's copy of them.
//
// A component's card carries its poster, which is why this runs after
// `generate:posters` — the picture in the card is the same still the page
// shows a reader without WebGPU, and both come from the real gallery.
//
// PNG, not WebP. The cards are read by crawlers and chat clients rather than
// by browsers, and the ones that still refuse WebP fail by showing nothing.
//
// Written into the built site rather than into `site/public`, because they are
// derived from that build: the stylesheet they are rendered against is the
// hashed one Vite just emitted.

import { mkdtemp, mkdir, readdir, readFile, rm, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { browserPath, closeBrowser, closeServer, launchBrowser, serve, settleAll, waitForValue } from "./cdp.mjs";
import { observeBrowser, saveBrowserEvidence } from "./browser-evidence.mjs";
import { CARD } from "./build.mjs";
import { socialCardName } from "../app/route-path.mjs";
import { DEFAULT as DEFAULT_THEME } from "../app/theme-resolve.mjs";
import { docs } from "../app/docs.mjs";
import catalog from "../generated/catalog.json" with { type: "json" };
import buildInfo from "../generated/build.json" with { type: "json" };

const here = path.dirname(fileURLToPath(import.meta.url));
const siteRoot = path.resolve(here, "..");

/**
 * The component whose still stands for the whole library.
 *
 * The pages that are not about one component still need a picture, and it has
 * to be of something: this is the densest story in the catalog and the one a
 * reader is most likely to have come for.
 */
const FLAGSHIP = "chat";

/** The card the build's `og:image` tags point at, for one route. */
const cardFile = (routePath) => `og/${socialCardName(routePath)}.png`;

/**
 * What each route's card says.
 *
 * The same title and description the page's own metadata carries — a card that
 * said something else would be a second, unversioned copy of the page's
 * summary, and the two would drift.
 */
/// The pages that get a card, and what each one says.
///
/// Exported so a test can compare it against the routes the site emits
/// without rendering anything. The full capture already catches a route that
/// claims a card nothing writes, but it catches it at the end of a release
/// build; this is the same fact available in milliseconds.
export function cards() {
  const component = (entry) => ({
    path: `/components/${entry.slug}/`,
    eyebrow: entry.category,
    title: entry.title,
    summary: entry.summary,
    poster: entry.slug,
  });
  return [
    {
      path: "/",
      eyebrow: `v${buildInfo.version}`,
      title: "Components for AI applications",
      summary: "Streaming, tools, approvals, and conversation, built with GPUI.",
      // Not the hero story: it is a guided demo and its resting frame is a
      // prompt bar waiting to be sent, which is an accurate picture of nothing.
      poster: FLAGSHIP,
    },
    {
      path: "/components/",
      eyebrow: "Catalog",
      title: `${catalog.components.length} components`,
      summary: "Grouped by what they are for, each with a demo running the real thing.",
      poster: "tool-calls",
    },
    {
      path: "/extensions/",
      eyebrow: "Extensions",
      title: "Paint into any frame",
      summary: "A decoration slot on every framed component, and a motion channel to drive it.",
      // The decorations story, which is the section's own subject: a card
      // showing a component being decorated says what the page is about in
      // the one way prose cannot.
      poster: "decorations",
    },
    {
      path: "/themes/",
      eyebrow: "Themes",
      title: "One set of numbers",
      summary: "The site, the gallery, and the demos are painted from the same tokens.",
      poster: null,
    },
    {
      path: "/docs/",
      eyebrow: "Documentation",
      title: "How it fits together",
      summary: "Installing it, theming it, and who owns what between it and your application.",
      poster: FLAGSHIP,
    },
    ...docs.map((doc) => ({
      path: `/docs/${doc.slug}/`,
      eyebrow: "Documentation",
      title: doc.title,
      summary: doc.summary,
      // Prose, not a component. A still of Chat beside a page about theming
      // would be decoration pretending to be an illustration.
      poster: null,
    })),
    ...catalog.components.map(component),
  ];
}

/**
 * The card's markup, written into the built site so it can reach its assets.
 *
 * It imports the site's own stylesheet by the hashed name Vite gave it, which
 * is how the card ends up in the site's type and palette without any of it
 * being written down twice. `data-theme` is set the way the site sets it, so
 * the card is painted in the theme a visitor arrives on.
 */
function cardHtml(stylesheet, card, base) {
  const poster = card.poster ? `${base}/posters/${card.poster}-dark.webp` : null;
  return `<!doctype html>
<html lang="en" data-theme="${DEFAULT_THEME}">
<head>
<meta charset="utf-8">
<link rel="stylesheet" href="${base}/assets/${stylesheet}">
<style>
  html, body { margin: 0; padding: 0; }
  body {
    width: ${CARD.width}px;
    height: ${CARD.height}px;
    overflow: hidden;
    display: grid;
    /* Two columns rather than a band across the bottom: an unfurl is small by
       the time anyone sees it, and half a card of empty background is half a
       card wasted. */
    grid-template-columns: ${poster ? "1.15fr 1fr" : "1fr"};
    background: var(--ai-background);
    color: var(--ai-foreground);
  }
  .og-card {
    display: grid;
    gap: 22px;
    align-content: center;
    padding: 0 60px;
    min-width: 0;
  }
  .og-eyebrow {
    color: var(--ai-accent-text);
    font-family: var(--ai-font-mono);
    font-size: 20px;
    letter-spacing: 0.14em;
    text-transform: uppercase;
  }
  .og-title {
    margin: 0;
    font-family: var(--site-font-serif);
    font-weight: 600;
    font-size: 68px;
    line-height: 1.05;
    letter-spacing: -0.02em;
    /* Four lines is the most a title can take before it crowds out the
       summary, and no title here is anywhere near it. */
    overflow: hidden;
  }
  .og-summary {
    margin: 0;
    color: var(--ai-muted-text);
    font-size: 26px;
    line-height: 1.4;
  }
  .og-mark {
    margin-top: 8px;
    color: var(--ai-foreground);
    font-weight: 600;
    font-size: 26px;
    letter-spacing: -0.01em;
  }
  .og-strip {
    position: relative;
    display: grid;
    place-items: center;
    padding: 44px 44px 44px 0;
    min-width: 0;
  }
  /* Fitted rather than filled. These stories are between 52 and 990 pixels tall,
     and filling a 630px panel with a 52px pixel field would enlarge it twelve
     times into an unreadable smear. Fitting it leaves a margin on the short
     ones, which is what a screenshot in a frame looks like anyway. */
  .og-strip img {
    max-width: 100%;
    max-height: 100%;
    border: 1px solid var(--ai-border);
    border-radius: var(--ai-radius-lg);
    background: var(--ai-surface);
    object-fit: contain;
  }
</style>
</head>
<body>
  <div class="og-card">
    <div class="og-eyebrow">${escape(card.eyebrow)}</div>
    <h1 class="og-title">${escape(card.title)}</h1>
    <p class="og-summary">${escape(card.summary)}</p>
    <div class="og-mark">gpui-ai</div>
  </div>
  ${poster ? `<div class="og-strip"><img src="${poster}" alt=""></div>` : ""}
</body>
</html>
`;
}

function escape(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

/**
 * Every card the built site says it has, read out of the pages themselves.
 *
 * The build writes an `og:image` for every route it emits, so this is the
 * complete list of promises made — and the list this script has to keep.
 */
async function claimedCards(siteDir) {
  const found = [];
  const walk = async (directory, page) => {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const here = path.join(directory, entry.name);
      if (entry.isDirectory()) {
        // The gallery is the demo embed and the API is rustdoc; neither is a
        // page this site renders, and neither carries a card.
        if (["gallery", "api", "og", "assets", "posters", "themes"].includes(entry.name)) continue;
        await walk(here, `${page}${entry.name}/`);
        continue;
      }
      if (entry.name !== "index.html") continue;
      const html = await readFile(here, "utf8");
      const claimed = /property="og:image" content="[^"]*?\/(og\/[^"]+)"/.exec(html);
      if (claimed) found.push({ page, file: claimed[1] });
    }
  };
  await walk(siteDir, "/");
  return found;
}

/** A card with nothing on it compresses to almost nothing. */
const SMALLEST_REAL_CARD = 4_000;
const CAPTURE_TIMEOUT_MS = 30_000;

/**
 * Renders one card per route into `<site>/og/`.
 *
 * Takes the built site because the cards are made from it: the stylesheet, the
 * fonts, and the posters all come out of that directory, and the result is
 * written back into it.
 */
export async function captureSocialCards({
  siteDir = path.join(siteRoot, "dist"),
  only,
  log = () => {},
} = {}) {
  if (!browserPath) throw new Error("no Chrome or Edge to render social cards with");
  if (!existsSync(path.join(siteDir, "index.html"))) {
    throw new Error(`build the site first: ${path.join(siteDir, "index.html")} is missing`);
  }

  const assets = await readdir(path.join(siteDir, "assets"));
  const stylesheet = assets.filter((name) => name.endsWith(".css")).sort()[0];
  if (!stylesheet) throw new Error("the built site has no stylesheet to render the cards against");

  // The base every asset in the card resolves against. The site is a project
  // page, so its own pages carry that prefix; the card is served from the same
  // root and has to as well.
  const base = new URL(buildInfo.homepage).pathname.replace(/\/$/, "");

  const temporaryRoot = await mkdtemp(path.join(tmpdir(), "gpui-ai-og-"));
  let serverHandle;
  let browserHandle;
  const written = [];

  try {
    serverHandle = await serve(siteDir);
    browserHandle = await launchBrowser(path.join(temporaryRoot, "browser"));
    await observeBrowser(browserHandle);
    const { cdp } = browserHandle;
    await Promise.all([cdp.send("Page.enable"), cdp.send("Runtime.enable")]);
    await mkdir(path.join(siteDir, "og"), { recursive: true });

    // `only` narrows to a few routes. The release gate renders the two it
    // drives rather than all thirty-seven, so it proves the chain — render,
    // publish, and a tag that names the file — in seconds instead of minutes.
    const wanted = only ? new Set(only) : undefined;
    const chosen = cards().filter((card) => !wanted || wanted.has(card.path));
    if (wanted && chosen.length !== wanted.size) {
      throw new Error(`no card for ${[...wanted].filter((route) => !chosen.some((card) => card.path === route)).join(", ")}`);
    }

    for (const card of chosen) {
      // Written into the site so that the stylesheet, the faces and the poster
      // are all same-origin relative paths, exactly as a page would see them.
      // Named for this process, because the release gate renders two cards
      // into a site another run may be rendering all of at the same time, and
      // one shared scratch file means one of them photographs the other's.
      const scratch = path.join(siteDir, "og", `_card-${process.pid}.html`);
      await writeFile(scratch, cardHtml(stylesheet, card, base));
      await cdp.navigate(
        `${serverHandle.origin}${base}/og/${path.basename(scratch)}`,
        CARD.width,
        CARD.height,
      );
      await waitForValue(
        cdp,
        // `complete` alone is true for an image that 404d, which is exactly the
        // card this must not publish: the poster is half of it.
        "(() => { const img = document.querySelector('.og-strip img'); return document.fonts.status === 'loaded' && (!img || img.naturalWidth > 0); })()",
        { label: `the ${card.path} card to have its type and its poster` },
      );

      const { data } = await cdp.send(
        "Page.captureScreenshot",
        { format: "png", captureBeyondViewport: false },
        CAPTURE_TIMEOUT_MS,
      );
      const bytes = Buffer.from(data, "base64");
      if (bytes.length < SMALLEST_REAL_CARD) {
        throw new Error(`the card for ${card.path} came out blank (${bytes.length} bytes)`);
      }
      const file = cardFile(card.path);
      await writeFile(path.join(siteDir, file), bytes);
      await rm(scratch, { force: true });
      written.push({ route: card.path, file, bytes: bytes.length });
      log(`${file} — ${(bytes.length / 1024).toFixed(1)} kB`);
    }
  } catch (error) {
    await saveBrowserEvidence(browserHandle, "social-card-capture").catch((diagnosticError) => console.error(diagnosticError));
    throw error;
  } finally {
    await settleAll([
      () => closeBrowser(browserHandle),
      () => closeServer(serverHandle),
      () => rm(path.join(siteDir, "og", `_card-${process.pid}.html`), { force: true }),
      () => rm(temporaryRoot, { force: true, recursive: true, maxRetries: 5, retryDelay: 100 }),
    ]);
  }

  // The tags were written by the build and the files by this script, and a tag
  // naming a card nobody rendered breaks an unfurl instead of degrading it.
  for (const { route, file } of written) {
    const html = await readFile(
      path.join(siteDir, ...route.split("/").filter(Boolean), "index.html"),
      "utf8",
    );
    if (!html.includes(file)) throw new Error(`${route} does not name the card written for it`);
  }

  // And the other direction, which is the one that goes wrong: a route added
  // to the site gets a card tag from the build for free, and gets a card from
  // here only if someone remembered. Five documentation pages and their index
  // shipped tags pointing at nothing. Checked against the built pages rather
  // than against a route list, because the tag is what makes the promise.
  if (!only) {
    const rendered = new Set(written.map((entry) => entry.file));
    for (const claimed of await claimedCards(siteDir)) {
      if (!rendered.has(claimed.file)) {
        throw new Error(`${claimed.page} claims ${claimed.file}, which nothing here renders`);
      }
    }
  }
  return written;
}

if (process.argv[1] && path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url))) {
  const written = await captureSocialCards({ log: (line) => console.log(line) });
  const total = written.reduce((sum, entry) => sum + entry.bytes, 0);
  console.log(`${written.length} cards, ${(total / 1024 / 1024).toFixed(2)} MB`);
}
