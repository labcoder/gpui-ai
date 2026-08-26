import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { buildSite } from "../scripts/build.mjs";
import {
  capturePosters,
  posterFrameLooksReal,
  POSTER_WIDTH,
} from "../scripts/capture-posters.mjs";
import { captureSocialCards } from "../scripts/capture-og.mjs";
import { CARD } from "../scripts/build.mjs";
import { DEFAULT as DEFAULT_THEME } from "../app/theme-resolve.mjs";
import catalog from "../generated/catalog.json" with { type: "json" };
import snippetFile from "../generated/snippets.json" with { type: "json" };
import themeFile from "../generated/themes.json" with { type: "json" };
import {
  browserPath,
  Cdp,
  closeBrowser,
  delay,
  GALLERY_DIAGNOSIS,
  GALLERY_GAVE_UP,
  launchBrowser,
  serve,
  settleAll,
  stopBrowserProcess,
  waitForValue,
} from "../scripts/cdp.mjs";
import { auditExpression, report } from "./contrast.mjs";

const { components } = catalog;

const releaseIntegrationRequested = process.env.GPUI_AI_RELEASE_INTEGRATION === "1";
// Skipping the release gate is a developer convenience, never a CI outcome: a
// runner without a browser would report green while proving nothing.
const releaseGateIsMandatory = releaseIntegrationRequested && process.env.CI === "true";
const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const releaseGalleryDir = path.join(repositoryRoot, "crates/gallery-web/www/dist");
// The two stories the release gate drives: one on its own page, one parked far
// below the fold to prove a demo stays unloaded. Both get a poster captured for
// them, because both are looked at in states where a poster is what shows.
const POSTER_SPECIMEN = "loading";
// A poster element exists as soon as React renders it; the bytes arrive later,
// and `naturalWidth` reads 0 until they do — which is also what it reads for a
// poster that 404s, so nothing may be asserted about one until it is complete.
const POSTER_LOADED =
  "(() => { const p = document.querySelector('[data-specimen-frame] img[data-demo-poster]'); return Boolean(p && p.complete); })()";
const IDLE_SPECIMEN = "chat";
// A story whose prose rewraps hard when the column narrows: 486px at the width
// the catalog measured, over 750 on a phone.
const REFLOWING_SPECIMEN = "attachments";

async function createGalleryFixture(directory) {
  await mkdir(path.join(directory, "assets"), { recursive: true });
  await Promise.all([
    writeFile(path.join(directory, "index.html"), "gallery index"),
    writeFile(path.join(directory, "embed.html"), "<!doctype html><title>Gallery fixture</title>"),
    writeFile(path.join(directory, "assets", "gallery_bg-fixture.wasm"), "wasm"),
  ]);
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

test("a timed-out wait names the condition, its state, and the collected page errors", async () => {
  const evaluated = [];
  const cdp = {
    evaluate: async (expression) => {
      evaluated.push(expression);
      return evaluated.length === 1
        ? { ok: false, timedOut: true }
        : { hasCanvas: false, stillStarting: true, reportedTheme: null };
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
      assert.match(error.message, /"stillStarting": true/);
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

test("poster validation reads visible pixels instead of encoded WebP size", () => {
  assert.equal(
    posterFrameLooksReal({ encodedBytes: 1_958, visiblePixels: 16 }),
    true,
    "the sparse loading poster captured in CI is still a real component",
  );
  assert.equal(
    posterFrameLooksReal({ encodedBytes: 8_192, visiblePixels: 0 }),
    false,
    "a large but uniform image is still blank",
  );
});

test("release WASM owns startup, theme sync, lifecycle, and WebGPU fallback", {
  skip: !browserPath && !releaseGateIsMandatory
    ? "Set CHROME_PATH or install Chrome, Edge, or Chromium to run the browser gate"
    : releaseIntegrationRequested ? false : "Run npm run check:web:release for the built-artifact integration gate",
  // Generous because this gate is one long story about one built artifact, and
  // splitting it would mean building and booting that artifact several times to
  // check things that are all true of the same run.
  //
  // The number is set from CI rather than from this machine: the gate ran in
  // 47s there against 29s here before the poster and card captures were added,
  // so a local minute is closer to two there. It also has to contain up to a
  // minute of waiting for a browser to start — see PORT_READY_TIMEOUT_MS —
  // because a launch that is slow but succeeds should not be reported as a
  // test that hung.
  timeout: 240_000,
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

  // The one story this gate drives, rendered into site/public/posters before
  // Vite copies that directory. Not all seventy: the chain being proved here —
  // capture, publish, serve, render into the fallback — is the same whichever
  // story runs through it, and the full set is a build step (generate:posters),
  // not a test.
  await capturePosters({ only: [POSTER_SPECIMEN, IDLE_SPECIMEN] });

  await buildSite({ galleryDir: releaseGalleryDir, outDir });
  // The social cards are rendered out of the built site — its stylesheet, its
  // faces, its posters — so they come after it. Two of the thirty-seven: the
  // chain being proved is the same whichever route runs through it.
  const cardsWritten = await captureSocialCards({
    siteDir: outDir,
    only: ["/", `/components/${POSTER_SPECIMEN}/`],
  });
  serverHandle = await serve(outDir);
  browserHandle = await launchBrowser(userDataDir);
  const { cdp } = browserHandle;
  const baseUrl = `${serverHandle.origin}/manual`;
  const errors = [];
  await Promise.all([cdp.send("Page.enable"), cdp.send("Runtime.enable"), cdp.send("Log.enable")]);
  // The site follows the operating system when nobody has chosen otherwise, so
  // the runner's own preference would decide what every check below sees. Pin
  // it; one step later flips it deliberately to prove the following works.
  await cdp.send("Emulation.setEmulatedMedia", {
    features: [{ name: "prefers-color-scheme", value: "light" }],
  });
  // Granted once, for every clipboard check below. Chrome also refuses the
  // clipboard to a document it does not consider focused, and a headless page
  // never is unless it is told it is.
  await cdp.send("Browser.grantPermissions", {
    origin: serverHandle.origin,
    permissions: ["clipboardReadWrite", "clipboardSanitizedWrite"],
  });
  await cdp.send("Emulation.setFocusEmulationEnabled", { enabled: true });
  await cdp.send("Page.bringToFront");
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
    `Boolean(document.querySelector('canvas') && window.gpuiAi?.currentTheme() === 'light')`,
    {
      label: "the release artifact to start and report the light theme",
      fatal: GALLERY_GAVE_UP,
      describe: GALLERY_DIAGNOSIS,
      errors,
    },
  );

  // Popped out, with no host to report to. The canvas starts hidden — an
  // unpresented WebGPU surface composites as solid black — and is revealed
  // when GPUI has drawn into it, which is a decision this document makes for
  // itself. Every check above is satisfied by a canvas that exists; this is
  // the one that fails if it exists and is invisible, which is what a
  // reader would get if revealing it depended on a host being there.
  await waitForValue(cdp, "getComputedStyle(document.querySelector('canvas')).opacity === '1'", {
    label: "the popped-out example to reveal its canvas",
    fatal: GALLERY_GAVE_UP,
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  assert.equal(
    await cdp.evaluate("'ready' in document.body.dataset"),
    true,
    "an example with no host must still decide for itself that it is drawing",
  );

  // Popped out there is no page behind this to scroll, so the wheel is the
  // example's from the start — waiting for a click would be waiting for a
  // reason that does not exist here.
  assert.equal(
    await cdp.evaluate("'captured' in document.body.dataset"),
    true,
    "an example with no host must take the wheel without being asked",
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
  const specimen = components.find((component) => component.slug === POSTER_SPECIMEN) ?? components[0];
  // Records every size the demo reports and whether the embed had drawn by
  // then, for the ordering assertion below. Installed before the page exists;
  // no-ops inside the frames themselves.
  await cdp.send("Page.addScriptToEvaluateOnNewDocument", {
    source: `if (window.top === window && !window.__demoSizes) {
      window.__demoSizes = [];
      window.addEventListener('message', (event) => {
        const message = event.data;
        if (!message || message.type !== 'gpui-ai-size') return;
        let ready = false;
        try {
          const frame = document.querySelector('[data-specimen-frame] iframe');
          ready = frame?.contentDocument?.body?.dataset?.ready !== undefined;
        } catch {}
        window.__demoSizes.push({ height: message.height, ready });
      });
    }`,
  });
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

  // The height the host is told is measured at the width the story is really
  // shown at, which means after the first drawn frame and never before: a
  // report taken while the canvas still had the parser's default width wraps
  // the story into a tall column, and the frame's tallest-wins reservation
  // keeps it forever — the published voice page reserved 1,381 px for a 90 px
  // story exactly this way.
  await waitForValue(
    cdp,
    "(() => { try { const frame = document.querySelector('[data-specimen-frame] iframe'); return frame?.contentDocument?.body?.dataset?.ready !== undefined; } catch { return false; } })()",
    {
      label: `the ${specimen.slug} demo to report its first drawn frame`,
      fatal: GALLERY_GAVE_UP,
      describe: GALLERY_DIAGNOSIS,
      errors,
    },
  );
  await delay(600);
  for (const sizeReport of JSON.parse(await cdp.evaluate("JSON.stringify(window.__demoSizes ?? [])"))) {
    assert.equal(
      sizeReport.ready,
      true,
      `a height of ${sizeReport.height} was reported before the first drawn frame`,
    );
  }
  const settledHeight = await cdp.evaluate(
    "Math.round(document.querySelector('[data-specimen-frame]').getBoundingClientRect().height)",
  );
  assert.ok(
    settledHeight < specimen.height * 4,
    `the frame settled at ${settledHeight}px for a ${specimen.height}px story — a pre-draw width has been ratcheted in`,
  );

  // The embed's first paint must composite transparent over the host, which
  // means agreeing on a colour scheme before its module arrives: the pin is
  // inline in the built embed's head, ahead of the module that later owns it —
  // a scheme-mismatched transparent iframe composites opaque white over a
  // dark host for the whole of the module's network round-trip.
  const embedMarkup = await cdp.evaluate(
    "fetch('/gpui-ai/gallery/embed.html').then((response) => response.text())",
  );
  const schemePinAt = embedMarkup.indexOf("colorScheme");
  const moduleAt = embedMarkup.indexOf('type="module"');
  assert.ok(
    schemePinAt !== -1 && moduleAt !== -1 && schemePinAt < moduleAt,
    "the built embed must pin its colour scheme before its module script",
  );
  assert.equal(
    await cdp.evaluate(
      "(() => { const frame = document.querySelector('[data-specimen-frame] iframe'); return frame.contentDocument.documentElement.classList.contains('dark') === document.documentElement.classList.contains('dark'); })()",
    ),
    true,
    "the running embed's scheme must match its host's",
  );

  // The page's code now arrives as a per-route chunk awaited before hydration.
  // A demo that has started proves React ran, so if the block below does not
  // reassemble the snippet, hydration blanked the pre-rendered code while the
  // chunk was still loading — the regression the await exists to prevent.
  const hydratedCode = await cdp.evaluate(
    "document.querySelector('pre.code code')?.textContent ?? ''",
  );
  assert.equal(
    hydratedCode.replace(/\n$/, ""),
    snippetFile.snippets[specimen.slug].default,
    "the hydrated page must still show the snippet its chunk carries",
  );

  // The other half of lazy: a frame that is nowhere near the viewport must not
  // load. Arriving at a deep anchor on a short viewport puts the demo well
  // above the observer's margin. Without this, promoting every frame on
  // hydration would pass every check above — and every visitor reading prose
  // would pay for the shared binary.
  const deep = components.find((component) => component.slug === IDLE_SPECIMEN) ?? specimen;
  // Under Dark, one of the two themes a poster is captured in, so the idle
  // frame shows its poster rather than the site's own words.
  await cdp.navigate(
    `${serverHandle.origin}/gpui-ai/components/${deep.slug}/?theme=dark#limits`,
    1280,
    400,
  );
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

  // D-09. An idle frame under Light or Dark shows the still captured from the
  // gallery, and `naturalWidth` is what proves the file was really served and
  // decoded rather than 404ing into an empty box — the failure a markup
  // assertion cannot tell apart from success.
  await waitForValue(cdp, POSTER_LOADED, {
    label: `the idle ${deep.slug} poster to load`,
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  const idlePoster = await cdp.evaluate(`(() => {
    const poster = document.querySelector('[data-specimen-frame] img[data-demo-poster]');
    if (!poster) return null;
    return {
      story: poster.dataset.demoPoster,
      src: poster.getAttribute('src'),
      width: Number(poster.getAttribute('width')),
      height: Number(poster.getAttribute('height')),
      naturalWidth: poster.naturalWidth,
      alt: poster.getAttribute('alt'),
    };
  })()`);
  assert.ok(idlePoster, `the idle ${deep.slug} frame shows no poster`);
  assert.equal(idlePoster.story, deep.slug);
  assert.equal(idlePoster.src, `/gpui-ai/posters/${deep.slug}-dark.webp`);
  assert.equal(idlePoster.naturalWidth, POSTER_WIDTH, "the poster did not load");
  // Both dimensions are declared, so the picture reserves exactly the space the
  // demo will take and swapping one for the other moves nothing.
  assert.equal(idlePoster.width, POSTER_WIDTH);
  assert.equal(idlePoster.height, deep.height);
  // Decoration: the live demo is about to replace it, and a screen reader
  // describing a placeholder describes something already gone.
  assert.equal(idlePoster.alt, "");
  // Asking for a theme in the URL records it, the same as clicking for one, and
  // every check below this expects a reader who has chosen nothing yet.
  await cdp.evaluate("window.localStorage.removeItem('gpui-ai:theme')");

  await cdp.evaluate("window.scrollTo(0, 0)");
  await waitForValue(cdp, "Boolean(document.querySelector('[data-specimen-frame] iframe'))", {
    label: "scrolling back to the demo to load it",
    describe: GALLERY_DIAGNOSIS,
    errors,
  });

  // And it is gone once the demo draws. A decoded poster is megabytes of RGBA,
  // so one left behind per demo would cost more than the frames it spared.
  await waitForValue(
    cdp,
    "(() => { const frame = document.querySelector('[data-specimen-frame] iframe'); return Boolean(frame?.contentDocument?.querySelector('canvas')); })()",
    {
      label: `the ${deep.slug} demo to draw`,
      fatal: GALLERY_GAVE_UP,
      describe: GALLERY_DIAGNOSIS,
      errors,
    },
  );
  await waitForValue(cdp, "!document.querySelector('[data-specimen-frame] img[data-demo-poster]')", {
    label: "the poster to be dropped once the demo is running",
    describe: GALLERY_DIAGNOSIS,
    errors,
  });

  // D-10. Running is not a one-way door. Every live demo is one instance of
  // the shared binary, its own WASM heap, and a WebGPU surface; a reader going
  // down a long page would collect one of each per demo passed, and nothing
  // above would notice — every check so far is satisfied by a frame that
  // starts and never stops.
  await cdp.evaluate("document.querySelector('#limits').scrollIntoView()");
  await waitForValue(cdp, "document.querySelectorAll('[data-specimen-frame] iframe').length === 0", {
    label: `the ${deep.slug} demo to be torn down once it is a viewport behind`,
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  // And it comes back. A demo that only ever stopped would be worse than one
  // that never stopped: the reader who scrolls back is looking straight at it.
  await cdp.evaluate("window.scrollTo(0, 0)");
  await waitForValue(
    cdp,
    "(() => { const frame = document.querySelector('[data-specimen-frame] iframe'); return Boolean(frame?.contentDocument?.querySelector('canvas')); })()",
    {
      label: `the ${deep.slug} demo to start again on the way back`,
      fatal: GALLERY_GAVE_UP,
      describe: GALLERY_DIAGNOSIS,
      errors,
    },
  );

  // S-03's whole claim: the chrome is painted from the generated tokens, so
  // setting the attribute the registry keys on repaints it. Nothing static can
  // check this — a stylesheet full of var() references looks correct whether or
  // not the properties resolve, and only a browser knows what was painted.
  const repaint = await cdp.evaluate(`(() => {
    const read = () => {
      const body = getComputedStyle(document.body);
      const rail = document.querySelector('.component-reference');
      return {
        background: body.backgroundColor,
        foreground: body.color,
        border: rail ? getComputedStyle(rail).borderTopColor : null,
        radius: rail ? getComputedStyle(rail).borderTopLeftRadius : null,
        face: body.fontFamily,
      };
    };
    const root = document.documentElement;
    const was = root.dataset.theme;
    const before = read();
    root.dataset.theme = 'ember-dusk';
    const after = read();
    // Put back whatever the inline script decided, rather than removing the
    // attribute: with no attribute the page falls to :root, which is a
    // different theme, not the one it was showing.
    root.dataset.theme = was;
    const restored = read();
    return { before, after, restored, was };
  })()`);
  for (const property of ["background", "foreground", "border"]) {
    assert.notEqual(
      repaint.after[property],
      repaint.before[property],
      `switching data-theme left ${property} at ${repaint.before[property]}`,
    );
  }
  assert.deepEqual(repaint.restored, repaint.before, "putting data-theme back must undo the change");
  assert.ok(repaint.was, "the inline script must have painted a theme before anything rendered");
  // The face comes from a token too, so a theme that changed it would move the
  // chrome and the demos together.
  assert.match(repaint.before.face, /IBM Plex Sans/);

  // Every interaction below needs the page hydrated, or it drives inert
  // markup and reports that nothing happened. The shell writes data-theme on
  // mount, so the attribute appearing is this site's own signal that its
  // handlers are attached — and that the theme is applied after render rather
  // than baked into the pre-render, which is what keeps hydration clean.
  // Colours cross-fade over 200ms, so a read taken the instant the attribute
  // changes still sees the old palette. Waiting for the class the fade runs
  // under also proves it is added and then taken away again — a transition
  // left in place would catch every later hover.
  const settleTheme = async (label, previous) => {
    // Waiting for the body to actually be a different colour is the only
    // deterministic way to read the new one: a fixed delay races the
    // transition, and the class the fade runs under comes off on its own timer
    // rather than when the colours have finished moving.
    await waitForValue(
      cdp,
      `getComputedStyle(document.body).backgroundColor !== ${JSON.stringify(previous)}`,
      { label: `the ${label} cross-fade to finish`, describe: GALLERY_DIAGNOSIS, errors },
    );
    // And the transition must not still be in force afterwards, or it catches
    // every later hover and makes the whole page feel slow.
    await waitForValue(cdp, "!document.documentElement.classList.contains('theming')", {
      label: `the ${label} transition to come back off`,
      describe: GALLERY_DIAGNOSIS,
      errors,
    });
  };

  // `expected` defaults to what a visitor who has chosen nothing gets, which
  // is the site's default theme rather than the machine's light or dark.
  const openPage = async (route, width, height, expected = DEFAULT_THEME) => {
    await cdp.navigate(`${serverHandle.origin}/gpui-ai${route}`, width, height);
    await waitForValue(
      cdp,
      `document.documentElement.dataset.theme === ${JSON.stringify(expected)}`,
      {
        label: `${route} to hydrate and apply its theme`,
        describe: GALLERY_DIAGNOSIS,
        errors,
      },
    );
  };

  // The drawer, driven the way a keyboard drives it. None of this is visible
  // in the markup: the panel ships hidden and everything below happens after
  // mount, so HTML assertions can only prove the parts exist.
  await openPage(`/components/${specimen.slug}/`, 390, 844);
  await cdp.evaluate(`(() => {
    const toggle = document.querySelector('[data-nav-toggle]');
    toggle.focus();
    toggle.click();
  })()`);
  await waitForValue(cdp, "!document.getElementById('site-nav-panel').hidden", {
    label: "the drawer to open",
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  const opened = await cdp.evaluate(`(() => {
    const toggle = document.querySelector('[data-nav-toggle]');
    const panel = document.getElementById('site-nav-panel');
    return {
      expanded: toggle.getAttribute('aria-expanded'),
      visible: !panel.hidden && panel.getBoundingClientRect().width > 0,
      focused: document.activeElement?.textContent,
      // Read before the focus probe below moves it.
      // Everything beside the panel must be inert, or Tab wanders into a page
      // the visitor cannot see and cannot get back from. The attribute goes on
      // the panel's siblings and inherits, so the property worth checking is
      // the one a keyboard would hit: nothing behind the drawer can take focus.
      inertSiblings: [...panel.parentElement.children]
        .filter((child) => child !== panel)
        .every((child) => child.hasAttribute('inert')),
      contentUnreachable: (() => {
        const behind = document.querySelector('#content a, #content button');
        if (!behind) return 'nothing focusable behind the drawer to test';
        behind.focus();
        return panel.contains(document.activeElement);
      })(),
      current: document.querySelectorAll('#site-nav-panel [aria-current="page"]').length,
    };
  })()`);
  assert.deepEqual(opened, {
    expanded: "true",
    visible: true,
    focused: "Close",
    inertSiblings: true,
    contentUnreachable: true,
    current: 1,
  });

  // A modal is supposed to cycle: Shift+Tab from the first control lands on
  // the last, and Tab from the last comes back to the first. `inert` keeps the
  // page behind out of the sequence but does nothing about its two ends.
  const wrapped = await cdp.evaluate(`(() => {
    const panel = document.getElementById('site-nav-panel');
    const stops = [...panel.querySelectorAll('a[href], button, input, [tabindex]')]
      .filter((element) => element.tabIndex >= 0 && element.offsetParent !== null);
    return { first: stops[0]?.textContent?.trim(), last: stops[stops.length - 1]?.textContent?.trim(), count: stops.length };
  })()`);
  assert.ok(wrapped.count > 2, `the drawer has only ${wrapped.count} tab stops`);

  await cdp.evaluate(
    "document.querySelector('#site-nav-panel [data-nav-close]').focus()",
  );
  await cdp.key("Tab", "Tab", 9, 8);
  assert.equal(
    await cdp.evaluate("document.activeElement?.textContent?.trim()"),
    wrapped.last,
    "Shift+Tab from the first control must wrap to the last, not leave the drawer",
  );
  await cdp.key("Tab", "Tab", 9);
  assert.equal(
    await cdp.evaluate("document.activeElement?.textContent?.trim()"),
    wrapped.first,
    "Tab from the last control must wrap back to the first",
  );

  await cdp.key("Escape", "Escape", 27);
  const closed = await cdp.evaluate(`(() => {
    const panel = document.getElementById('site-nav-panel');
    return {
      expanded: document.querySelector('[data-nav-toggle]').getAttribute('aria-expanded'),
      hidden: panel.hidden,
      // Focus has to come back to something, and the toggle is where the
      // visitor left it.
      focused: document.activeElement?.dataset.navToggle !== undefined,
      anyInert: document.querySelectorAll('[inert]').length,
    };
  })()`);
  assert.deepEqual(closed, { expanded: "false", hidden: true, focused: true, anyInert: 0 });

  // The backdrop is pointer-only by design, so prove the pointer path works
  // too — otherwise it is decoration that traps a mouse user.
  await cdp.evaluate("document.querySelector('[data-nav-toggle]').click()");
  await waitForValue(cdp, "!document.getElementById('site-nav-panel').hidden", {
    label: "the drawer to reopen for the pointer path",
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  // Dispatched at coordinates, not through the element's own click(): the
  // claim is that a pointer reaches the backdrop, and a handler fires just as
  // happily on something buried under another layer.
  // Throws unless a real pointer can land on the backdrop itself, which its
  // own centre cannot do — the drawer covers that.
  await cdp.clickAt(".nav-backdrop");
  await waitForValue(cdp, "document.getElementById('site-nav-panel').hidden", {
    label: "a backdrop click to close the drawer",
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  assert.deepEqual(
    await cdp.evaluate(`(() => ({
      backdropIsButton: Boolean(document.querySelector('button.nav-backdrop')),
      backdropFocusable: document.querySelector('.nav-backdrop').tabIndex >= 0,
    }))()`),
    { backdropIsButton: false, backdropFocusable: false },
  );

  // A drawer open across the desktop breakpoint. The toggle that opened it is
  // display:none up here, so handing focus back to it would drop focus onto
  // nothing — the page would look fine and the keyboard would be lost.
  await cdp.evaluate("document.querySelector('[data-nav-toggle]').click()");
  await waitForValue(cdp, "!document.getElementById('site-nav-panel').hidden", {
    label: "the drawer to open before the resize",
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  await cdp.send("Emulation.setDeviceMetricsOverride", {
    width: 1280,
    height: 900,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await waitForValue(cdp, "document.getElementById('site-nav-panel').hidden", {
    label: "the drawer to close when the rail appears",
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  assert.deepEqual(
    await cdp.evaluate(`(() => {
      const active = document.activeElement;
      return {
        onSomethingVisible: Boolean(active && active !== document.body && active.offsetParent !== null || active?.id === 'content'),
        id: active?.id ?? active?.tagName ?? null,
        anyInert: document.querySelectorAll('[inert]').length,
      };
    })()`),
    { onSomethingVisible: true, id: "content", anyInert: 0 },
    "focus must land somewhere real when the drawer closes itself",
  );
  await cdp.send("Emulation.setDeviceMetricsOverride", {
    width: 390,
    height: 844,
    deviceScaleFactor: 1,
    mobile: false,
  });

  // The mode control has to actually change what the page is painted from.
  const readMode = `(() => ({
    theme: document.documentElement.dataset.theme,
    background: getComputedStyle(document.body).backgroundColor,
    pressed: [...document.querySelectorAll('[data-theme-choice]')]
      .filter((button) => button.getAttribute('aria-pressed') === 'true')
      .map((button) => button.dataset.themeChoice),
  }))()`;
  const beforeMode = await cdp.evaluate(readMode);
  assert.equal(
    beforeMode.theme,
    "nord-frost",
    "a visitor who has chosen nothing must land on the default theme",
  );
  assert.deepEqual(
    beforeMode.pressed,
    [],
    "Nord Frost is none of the three modes, so none of them may claim to be current",
  );
  await cdp.evaluate("document.querySelector('[data-theme-choice=\"dark\"]').click()");
  await waitForValue(cdp, "document.documentElement.dataset.theme === 'dark'", {
    label: "the dark control to change the mode",
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  await settleTheme("dark", beforeMode.background);
  const afterMode = await cdp.evaluate(readMode);
  assert.deepEqual(afterMode.pressed, ["dark"], "the control must state which mode is current");
  assert.notEqual(afterMode.background, beforeMode.background, "dark repainted nothing");
  // The embed reads this class when the host names no theme, so a demo opened
  // after the switch starts dark instead of contradicting the page around it.
  assert.equal(await cdp.evaluate("document.documentElement.classList.contains('dark')"), true);

  // And a demo that was already running follows too. Without this the page
  // goes dark around a white window, which is worse than not offering the
  // control — and no HTML assertion can see it, because the frame's contents
  // are drawn on a canvas.
  await waitForValue(
    cdp,
    "document.querySelector('[data-specimen-frame] iframe')?.contentWindow?.gpuiAi?.currentTheme() === 'dark'",
    {
      label: "the running demo to follow the page into dark",
      fatal: GALLERY_GAVE_UP,
      describe: GALLERY_DIAGNOSIS,
      errors,
    },
  );

  // The whole theme engine, end to end, in the only place it can be checked.
  // Picking a registry theme has to survive a reload, put itself in the URL so
  // the page can be linked as it looks, and repaint chrome and demo together.
  await cdp.evaluate(`(() => {
    const select = document.getElementById('site-theme');
    select.value = 'ember-dusk';
    select.dispatchEvent(new Event('change', { bubbles: true }));
  })()`);
  await waitForValue(cdp, "document.documentElement.dataset.theme === 'ember-dusk'", {
    label: 'the picker to apply a registry theme',
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  await settleTheme("Ember Dusk", afterMode.background);
  const picked = await cdp.evaluate(`(() => ({
    stored: window.localStorage.getItem('gpui-ai:theme'),
    param: new URLSearchParams(window.location.search).get('theme'),
    background: getComputedStyle(document.body).backgroundColor,
    pressed: [...document.querySelectorAll('[data-theme-choice]')]
      .filter((button) => button.getAttribute('aria-pressed') === 'true')
      .map((button) => button.dataset.themeChoice),
  }))()`);
  assert.equal(picked.stored, 'ember-dusk', 'the choice must survive a reload');
  assert.equal(picked.param, 'ember-dusk', 'the page must be linkable as it looks');
  assert.notEqual(picked.background, afterMode.background, 'the registry theme repainted nothing');
  // None of the three mode buttons is what is showing, and saying otherwise
  // would be a lie a screen reader repeats.
  assert.deepEqual(picked.pressed, []);

  // Ask to follow the system, and then move the system. Following the machine
  // is now a choice like any other — the site opens on Nord Frost, so it has
  // to be recorded, or a reload would quietly overrule it.
  await cdp.evaluate(`(() => {
    const select = document.getElementById('site-theme');
    select.value = 'system';
    select.dispatchEvent(new Event('change', { bubbles: true }));
  })()`);
  await waitForValue(cdp, "document.documentElement.dataset.theme === 'light'", {
    label: "returning to the system preference",
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  assert.equal(
    await cdp.evaluate("window.localStorage.getItem('gpui-ai:theme')"),
    "system",
    "asking to follow the system must survive a reload",
  );
  assert.equal(
    await cdp.evaluate("new URLSearchParams(window.location.search).get('theme')"),
    "system",
    "the URL must carry any choice that is not the default",
  );
  await cdp.send("Emulation.setEmulatedMedia", {
    features: [{ name: "prefers-color-scheme", value: "dark" }],
  });
  await waitForValue(cdp, "document.documentElement.dataset.theme === 'dark'", {
    label: "the page to follow the system flipping to dark",
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  await cdp.send("Emulation.setEmulatedMedia", {
    features: [{ name: "prefers-color-scheme", value: "light" }],
  });
  await waitForValue(cdp, "document.documentElement.dataset.theme === 'light'", {
    label: "the page to follow the system back to light",
    describe: GALLERY_DIAGNOSIS,
    errors,
  });

  // Choosing the default is still a choice, and it goes in the URL like any
  // other: a plain address means "the default", which for the person opening
  // the link is whatever they have already chosen for themselves, not what the
  // sender was looking at.
  await cdp.evaluate(`(() => {
    const select = document.getElementById('site-theme');
    select.value = 'nord-frost';
    select.dispatchEvent(new Event('change', { bubbles: true }));
  })()`);
  await waitForValue(cdp, "document.documentElement.dataset.theme === 'nord-frost'", {
    label: "the picker to return to the default theme",
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  assert.deepEqual(
    await cdp.evaluate(`(() => ({
      stored: window.localStorage.getItem('gpui-ai:theme'),
      param: new URLSearchParams(window.location.search).get('theme'),
    }))()`),
    { stored: "nord-frost", param: "nord-frost" },
    "choosing the default must be remembered and linkable like any other choice",
  );

  // Close off both places a choice is normally recorded — a storage that
  // reads but refuses to write, and a frame that refuses history writes — and
  // the choice must still apply. Neither is hypothetical: a full quota does
  // the first and a sandboxed iframe does the second, and in both cases the
  // stale value stays readable, which is what used to overrule the visitor.
  await cdp.evaluate(`(() => {
    window.localStorage.setItem('gpui-ai:theme', 'ember-dusk');
    const storage = Object.getPrototypeOf(window.localStorage);
    window.__setItem = storage.setItem;
    storage.setItem = () => { throw new Error('quota exceeded'); };
    window.__replaceState = window.history.replaceState;
    window.history.replaceState = () => { throw new Error('sandboxed'); };
  })()`);
  await cdp.evaluate(`(() => {
    const select = document.getElementById('site-theme');
    select.value = 'solstice';
    select.dispatchEvent(new Event('change', { bubbles: true }));
  })()`);
  await waitForValue(cdp, "document.documentElement.dataset.theme === 'solstice'", {
    label: "a choice to apply when nothing can record it",
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  assert.deepEqual(
    await cdp.evaluate(`(() => ({
      stored: window.localStorage.getItem('gpui-ai:theme'),
      param: new URLSearchParams(window.location.search).get('theme'),
    }))()`),
    { stored: "ember-dusk", param: "nord-frost" },
    "the test must really have blocked both records, or it proves nothing",
  );
  await cdp.evaluate(`(() => {
    Object.getPrototypeOf(window.localStorage).setItem = window.__setItem;
    window.history.replaceState = window.__replaceState;
    window.localStorage.removeItem('gpui-ai:theme');
  })()`);

  // Store a theme again, then reload: the inline script has to paint it
  // before anything else renders, or the page flashes the default first.
  await cdp.evaluate("window.localStorage.setItem('gpui-ai:theme', 'ember-dusk')");

  // Watch for the first time anything sets the attribute, and record what the
  // document was doing at that moment. This is the only way to tell the inline
  // script from the React effect: both leave the same attribute behind, and
  // only one of them beats the stylesheet. `loading` means the head is still
  // being parsed; anything else means the page painted the wrong palette first
  // and then corrected itself, which is the flash the script exists to prevent.
  const { identifier } = await cdp.send("Page.addScriptToEvaluateOnNewDocument", {
    source: `
      window.__firstThemePaint = null;
      window.__everyThemePaint = [];
      // Reads the records, not just the live attribute. MutationObserver
      // batches everything from one task into a single callback, so a paint to
      // the wrong theme and back within one task would leave the attribute
      // already corrected by the time this ran — which is exactly the flash
      // the sequence below exists to catch.
      var record = function (records) {
        var root = document.documentElement;
        if (!root || !root.getAttribute('data-theme')) return;
        var seen = window.__everyThemePaint;
        (records || []).forEach(function (entry) {
          var was = entry.oldValue;
          if (was && seen[seen.length - 1] !== was) seen.push(was);
        });
        var theme = root.getAttribute('data-theme');
        if (seen[seen.length - 1] !== theme) seen.push(theme);
        if (window.__firstThemePaint) return;
        window.__firstThemePaint = {
          theme: theme,
          readyState: document.readyState,
        };
      };
      // Observed on the document rather than on documentElement: this runs
      // before the page has any script of its own, and at that point there may
      // be no element to attach to yet.
      new MutationObserver(record).observe(document, {
        attributes: true,
        subtree: true,
        // Needed for the oldValue read above: without it every record reports
        // null, and a paint that was already corrected leaves no trace.
        attributeOldValue: true,
        attributeFilter: ['data-theme'],
      });
      record();
    `,
  });
  await openPage(`/components/${specimen.slug}/`, 1280, 900, "ember-dusk");
  await cdp.send("Page.removeScriptToEvaluateOnNewDocument", { identifier });

  assert.deepEqual(
    await cdp.evaluate("window.__firstThemePaint"),
    { theme: "ember-dusk", readyState: "loading" },
    "a stored theme must be painted while the head is still parsing, not after hydration",
  );
  // Not just the first paint: every value the attribute ever took. Hydration
  // renders the default snapshot before the store reports the stored choice,
  // so a shell that painted on that first pass would repaint the page to the
  // default and back, and the check above would not notice.
  assert.deepEqual(
    await cdp.evaluate("window.__everyThemePaint"),
    ["ember-dusk"],
    "the page must never be painted a theme the visitor did not ask for",
  );

  // And a link carrying a theme wins over the stored one for that visit.
  await cdp.navigate(
    `${serverHandle.origin}/gpui-ai/components/${specimen.slug}/?theme=solstice`,
    1280,
    900,
  );
  assert.equal(
    await cdp.evaluate('document.documentElement.dataset.theme'),
    'solstice',
    'a theme in the URL must win for the visit it was linked for',
  );
  await cdp.evaluate("window.localStorage.removeItem('gpui-ai:theme')");
  await cdp.evaluate("window.history.replaceState(null, '', window.location.pathname)");
  // The demo's own toolbar. The override is the interesting one: it has to
  // move this frame without moving the page, and without the frame being torn
  // down and rebuilt, which is why it travels as a message rather than a URL.
  // Watch the whole of a demo's arrival, not just where it ends up. Every
  // other check here reads the settled state, which is the same whether the
  // canvas was hidden on the way or shown black the entire time.
  const { identifier: watcher } = await cdp.send("Page.addScriptToEvaluateOnNewDocument", {
    source: `
      window.__demoWatch = { samples: 0, sawStarting: false, shownUndrawn: 0 };
      (function watch() {
        var state = window.__demoWatch;
        state.samples += 1;
        if (document.querySelector('[data-demo-starting]')) state.sawStarting = true;
        try {
          var frame = document.querySelector('[data-specimen-frame] iframe');
          var canvas = frame && frame.contentDocument && frame.contentDocument.querySelector('canvas');
          // 300x150 is the backing store a canvas nothing has drawn into
          // carries. Visible at that size is the black rectangle itself.
          if (canvas && getComputedStyle(canvas).opacity === '1' &&
              canvas.width === 300 && canvas.height === 150) {
            state.shownUndrawn += 1;
          }
        } catch (error) {
          // A frame mid-navigation is not readable, and is not evidence.
        }
        requestAnimationFrame(watch);
      })();
    `,
  });

  await openPage(`/components/${specimen.slug}/`, 1280, 900);
  const frameTheme = (theme) =>
    `document.querySelector('[data-specimen-frame] iframe')?.contentWindow?.gpuiAi?.currentTheme() === '${theme}'`;
  await waitForValue(cdp, frameTheme(DEFAULT_THEME), {
    label: "the demo to start out following the site",
    fatal: GALLERY_GAVE_UP,
    describe: GALLERY_DIAGNOSIS,
    errors,
  });

  // How a demo arrives. The embed used to paint its own page and put a card
  // in the middle of this window while it started, so opening a page meant
  // watching the window go black, show a card, and take it away. It paints
  // nothing now, and the canvas — which composites as solid black until GPUI
  // presents a frame, because gpui_web configures the surface opaque — stays
  // at zero opacity until it has been drawn at the window's real size.
  //
  // Waited for rather than read straight away: the reveal is a 140ms fade, and
  // reading mid-fade is a race that says nothing about the mechanism.
  await waitForValue(
    cdp,
    `getComputedStyle(document.querySelector('[data-specimen-frame] iframe').contentDocument.querySelector('canvas')).opacity === '1'`,
    { label: "the demo to finish appearing", describe: GALLERY_DIAGNOSIS, errors },
  );
  assert.deepEqual(
    await cdp.evaluate(`(() => {
      const frame = document.querySelector('[data-specimen-frame] iframe');
      const inner = frame.contentDocument;
      const canvas = inner.querySelector('canvas');
      return {
        ready: 'ready' in inner.body.dataset,
        htmlBackground: getComputedStyle(inner.documentElement).backgroundColor,
        bodyBackground: getComputedStyle(inner.body).backgroundColor,
        canvasOpacity: getComputedStyle(canvas).opacity,
        drawnAtFullSize: canvas.width >= canvas.clientWidth && canvas.clientWidth > 0,
        loadingCard: Boolean(inner.getElementById('loading')),
        windowBackground: getComputedStyle(document.querySelector('[data-specimen-frame]')).backgroundColor,
      };
    })()`),
    {
      ready: true,
      htmlBackground: "rgba(0, 0, 0, 0)",
      bodyBackground: "rgba(0, 0, 0, 0)",
      canvasOpacity: "1",
      drawnAtFullSize: true,
      loadingCard: false,
      // Nord Frost's own --ai-background, which is what the canvas paints too,
      // so the window fills in and the component appears inside it without
      // anything changing colour.
      windowBackground: "rgb(46, 52, 64)",
    },
    "a demo must arrive by appearing in its window, not by flashing black first",
  );

  // What the watcher saw on the way here. This is the half the settled reading
  // cannot cover: delete the rule that hides the canvas and `shownUndrawn`
  // climbs; delete the title-bar hint and `sawStarting` never becomes true.
  const watched = await cdp.evaluate("window.__demoWatch");
  await cdp.send("Page.removeScriptToEvaluateOnNewDocument", { identifier: watcher });
  // Enough frames that a flash lasting a few of them would have been caught.
  // Headless throttles requestAnimationFrame, so this settles around thirty
  // across the load rather than sixty a second.
  assert.ok(watched.samples > 10, `only ${watched.samples} frames were sampled`);
  assert.equal(
    watched.shownUndrawn,
    0,
    "the canvas was visible before anything had been drawn into it",
  );
  assert.equal(watched.sawStarting, true, "the window never said the demo was starting");

  // Who the wheel belongs to. GPUI's web platform calls preventDefault on
  // every wheel event before looking at it, so a canvas swallows the wheel
  // whether or not the story has anything to scroll — which left a reader
  // unable to move the page in either direction while the pointer was over a
  // demo. The page has it by default now, and the demo takes it on a click.
  const wheelAt = async (x, y, deltaY, times = 8) => {
    for (let index = 0; index < times; index += 1) {
      await cdp.send("Input.dispatchMouseEvent", {
        type: "mouseWheel", x, y, deltaX: 0, deltaY, pointerType: "mouse",
      });
    }
    await delay(400);
  };
  const scrollY = () => cdp.evaluate("window.scrollY");
  const saysItScrolls = () => cdp.evaluate("Boolean(document.querySelector('[data-demo-scrolls]'))");

  await cdp.evaluate("document.querySelector('[data-specimen-frame]').scrollIntoView({ block: 'center' })");
  await delay(400);
  const overDemo = await cdp.evaluate(`(() => {
    const rect = document.querySelector('[data-specimen-frame]').getBoundingClientRect();
    return { x: Math.round(rect.left + rect.width / 2), y: Math.round(rect.top + rect.height / 2) };
  })()`);

  const parked = await scrollY();
  assert.equal(await saysItScrolls(), false, "a demo nobody has touched must not claim the wheel");
  await wheelAt(overDemo.x, overDemo.y, 400);
  assert.notEqual(
    await scrollY(),
    parked,
    "the page must scroll while the pointer is over a demo nobody has clicked into",
  );

  // Clicking in hands it over, and the window says so rather than swallowing
  // the wheel silently — which is what made this feel like a trap.
  await cdp.evaluate("document.querySelector('[data-specimen-frame]').scrollIntoView({ block: 'center' })");
  await delay(400);
  for (const type of ["mousePressed", "mouseReleased"]) {
    await cdp.send("Input.dispatchMouseEvent", {
      type, x: overDemo.x, y: overDemo.y, button: "left", clickCount: 1,
    });
  }
  await waitForValue(cdp, "Boolean(document.querySelector('[data-demo-scrolls]'))", {
    label: "the window to say the demo has the wheel",
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  const engagedAt = await scrollY();
  await wheelAt(overDemo.x, overDemo.y, 400);
  assert.equal(await scrollY(), engagedAt, "a demo that has been clicked into must keep the wheel");

  // And moving away gives it straight back, so nothing is held that was not
  // asked for.
  await cdp.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: overDemo.x, y: 60 });
  await waitForValue(cdp, "!document.querySelector('[data-demo-scrolls]')", {
    label: "the demo to give the wheel back when the pointer leaves",
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  const leftAt = await scrollY();
  await wheelAt(overDemo.x, overDemo.y, 400);
  assert.notEqual(await scrollY(), leftAt, "the page must have the wheel again once the pointer leaves");
  await cdp.evaluate("window.scrollTo(0, 0)");

  // The same question with a finger. The canvas ships with an inline
  // `touch-action: none`, which stopped a drag over a demo scrolling anything
  // at all. A touch pointer stops existing when the finger lifts, so the
  // capture is released with it and vertical panning stays with the page —
  // being unable to scroll a page is worse than being unable to pan a
  // transcript, and this is the assertion that keeps it that way.
  const touchAction = () =>
    cdp.evaluate(
      "getComputedStyle(document.querySelector('[data-specimen-frame] iframe').contentDocument.querySelector('canvas')).touchAction",
    );
  await cdp.send("Emulation.setTouchEmulationEnabled", { enabled: true, maxTouchPoints: 5 });
  assert.equal(await touchAction(), "pan-y", "a finger must be able to scroll the page over a demo");
  await cdp.send("Input.dispatchTouchEvent", {
    type: "touchStart",
    touchPoints: [{ x: overDemo.x, y: overDemo.y }],
  });
  await waitForValue(cdp, "Boolean(document.querySelector('[data-demo-scrolls]'))", {
    label: "a demo to take the gesture while the finger is down",
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  await cdp.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
  await waitForValue(cdp, "!document.querySelector('[data-demo-scrolls]')", {
    label: "the demo to let go when the finger lifts",
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  assert.equal(await touchAction(), "pan-y", "a tap must not leave a demo holding the page");
  await cdp.send("Emulation.setTouchEmulationEnabled", { enabled: false });

  const readout = () => cdp.evaluate("document.querySelector('.demo-readout').dataset.readout");
  assert.equal(
    await readout(),
    DEFAULT_THEME,
    "the readout must name what the frame is painted from",
  );

  await cdp.evaluate(`(() => {
    const select = document.querySelector('.demo-toolbar select');
    select.value = 'ember-dusk';
    select.dispatchEvent(new Event('change', { bubbles: true }));
  })()`);
  await waitForValue(cdp, frameTheme("ember-dusk"), {
    label: "the demo to take a theme of its own",
    fatal: GALLERY_GAVE_UP,
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  assert.equal(await readout(), "ember-dusk");
  assert.match(
    await cdp.evaluate("document.querySelector('.demo-readout').textContent"),
    /OVERRIDDEN$/,
    "a frame that has stopped following the page should say so",
  );
  // And the page has not moved. An override that changed the site theme would
  // be a different control wearing the same label.
  assert.equal(await cdp.evaluate("document.documentElement.dataset.theme"), DEFAULT_THEME);
  assert.match(
    await cdp.evaluate("document.querySelector('[data-specimen-open]').getAttribute('href')"),
    /theme=ember-dusk$/,
    "Pop out must open the demo as it is being shown, not as it started",
  );

  // D-04. Reset reaches into the running demo instead of replacing it. The
  // whole point is what does *not* happen: replacing the frame tears down a
  // seventeen-megabyte WebAssembly instance and downloads nothing, to reach a
  // state the story gets back to in one frame. A document that survived the
  // press is the proof, and it is only visible from out here.
  const wasStartedAt = await cdp.evaluate(
    "document.querySelector('[data-specimen-frame] iframe').contentWindow.performance.timeOrigin",
  );
  await cdp.evaluate("document.querySelector('[data-specimen-reload]').click()");
  await delay(1_000);
  assert.equal(
    await cdp.evaluate(
      "document.querySelector('[data-specimen-frame] iframe').contentWindow.performance.timeOrigin",
    ),
    wasStartedAt,
    "Reset must not throw the WebAssembly instance away to get back to the start",
  );
  // And the demo is still the demo: still drawing, still overridden. That the
  // story itself went back to its opening state is a claim about the gallery,
  // and is tested where the state lives.
  assert.equal(
    await cdp.evaluate(
      "Boolean(document.querySelector('[data-specimen-frame] iframe').contentDocument.querySelector('canvas'))",
    ),
    true,
    "the demo must still be running after a reset",
  );
  await waitForValue(cdp, frameTheme("ember-dusk"), {
    label: "the reset demo to still be overridden",
    fatal: GALLERY_GAVE_UP,
    describe: GALLERY_DIAGNOSIS,
    errors,
  });

  await cdp.evaluate("document.querySelector('[data-specimen-link]').click()");
  await waitForValue(
    cdp,
    "document.querySelector('.demo-toolbar .copy-status').textContent.length > 0",
    { label: "Copy link to report what it did", describe: GALLERY_DIAGNOSIS, errors },
  );
  assert.match(
    await cdp.evaluate("navigator.clipboard.readText()"),
    new RegExp(`/components/${specimen.slug}/\\?theme=ember-dusk$`),
    "Copy link must hand over the page as it is being looked at",
  );

  // Copy, against a real clipboard. Everything short of this checks that the
  // page holds the right string somewhere; only reading the clipboard back
  // shows what a visitor would paste. The panel is highlighted, so the
  // failure this rules out is copying spans, classes, or a partial line.
  await openPage(`/components/${specimen.slug}/`, 1280, 900);
  await cdp.evaluate("document.querySelector('[data-copy]').click()");
  await waitForValue(cdp, "document.querySelector('.code-actions .copy-status').textContent.length > 0", {
    label: 'the copy button to report what it did',
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  // Line endings are normalised on the way back out: the Windows clipboard
  // hands back CRLF whatever was put in. That is the operating system, not the
  // page, and asserting on it would make this test pass on one platform only.
  assert.equal(
    (await cdp.evaluate("navigator.clipboard.readText()")).replaceAll("\r\n", "\n"),
    snippetFile.snippets[specimen.slug].default,
    "the clipboard must hold the snippet, not the markup around it",
  );
  assert.match(
    await cdp.evaluate("document.querySelector('.code-actions .copy-status').textContent"),
    /^Copied /,
    'a copy that says nothing is a copy a visitor cannot trust',
  );

  // And the other way anyone copies code: dragging across it. A line number in
  // the middle of the result would be a snippet that does not compile.
  //
  // This engine leaves generated content out of a selection on its own, so
  // passing here is not proof that the `user-select: none` on the gutter is
  // doing anything — that rule is for the ones that do not. What this does
  // check is the whole rendered path, which the markup assertions cannot: what
  // a reader ends up holding.
  assert.equal(
    (
      await cdp.evaluate(`(() => {
        const code = document.querySelector('pre.code code');
        const range = document.createRange();
        range.selectNodeContents(code);
        const selection = window.getSelection();
        selection.removeAllRanges();
        selection.addRange(range);
        const text = selection.toString();
        selection.removeAllRanges();
        return text;
      })()`)
    ).replaceAll("\r\n", "\n").replace(/\n$/, ""),
    snippetFile.snippets[specimen.slug].default,
    "a selection dragged across the code must give back the code, and nothing else",
  );
  // The themes page is the one that has to prove the whole claim: the site and
  // the demos are painted from the same numbers. Choosing a card repaints the
  // page and the running trio together, and the file behind Download is the
  // theme itself rather than a picture of it.
  await openPage("/themes/", 1280, 900);
  await waitForValue(
    cdp,
    // One, exactly: the trio is one composed story in one runtime now — three
    // separate demos cost a WebAssembly instance and a WebGPU context each
    // for pixels the composition shows together. "At least" would let a
    // failed promotion pass, because the `.every()` below only visits frames
    // that exist.
    "document.querySelectorAll('[data-specimen-frame] iframe').length === 1",
    { label: "the composed trio demo on the themes page to promote", describe: GALLERY_DIAGNOSIS, errors },
  );
  await waitForValue(
    cdp,
    `[...document.querySelectorAll('[data-specimen-frame] iframe')].every((frame) => frame.contentWindow?.gpuiAi?.currentTheme() === '${DEFAULT_THEME}')`,
    {
      label: "the trio to start on the site theme",
      fatal: GALLERY_GAVE_UP,
      describe: GALLERY_DIAGNOSIS,
      errors,
    },
  );

  // The card for the theme already showing is the one case the control has to
  // refuse, and the site opens on a named theme, so it is true before anything
  // is clicked.
  assert.equal(
    await cdp.evaluate(
      `document.querySelector('[data-use-theme="${DEFAULT_THEME}"]').disabled`,
    ),
    true,
    "the card for the theme already showing should not offer to be used",
  );

  const pageBackground = await cdp.evaluate("getComputedStyle(document.body).backgroundColor");
  await cdp.evaluate("document.querySelector('[data-use-theme=\"ember-dusk\"]').click()");
  await settleTheme("Ember Dusk", pageBackground);
  assert.equal(await cdp.evaluate("document.documentElement.dataset.theme"), "ember-dusk");
  await waitForValue(
    cdp,
    "[...document.querySelectorAll('[data-specimen-frame] iframe')].every((frame) => frame.contentWindow?.gpuiAi?.currentTheme() === 'ember-dusk')",
    {
      label: "every demo on the page to follow the card that was chosen",
      fatal: GALLERY_GAVE_UP,
      describe: GALLERY_DIAGNOSIS,
      errors,
    },
  );
  assert.equal(
    await cdp.evaluate("document.querySelector('[data-use-theme=\"ember-dusk\"]').disabled"),
    true,
    "the card in use should not offer to be used again",
  );
  assert.equal(
    await cdp.evaluate(
      `document.querySelector('[data-use-theme="${DEFAULT_THEME}"]').disabled`,
    ),
    false,
    "the card that was left should offer itself again",
  );

  // Download is a real file at a real URL, not a blob built in the page from
  // values the site derived — those would read back as a theme and not be one.
  const download = await cdp.evaluate(`(async () => {
    const link = document.querySelector('[data-theme-card="ember-dusk"] a[download]');
    const response = await fetch(link.href);
    return { status: response.status, body: await response.json(), href: link.getAttribute('href') };
  })()`);
  assert.equal(download.status, 200, `${download.href} does not resolve`);
  assert.equal(download.body.themes.length, 1);
  assert.match(download.body.themes[0].name, /Ember Dusk/);
  assert.ok(
    Object.keys(download.body.themes[0].colors).length > 3,
    "the downloaded theme has no colours",
  );

  // Choosing a card is a durable choice, which is the point of it — so put the
  // browser back to a visitor who has chosen nothing before the checks below.
  await cdp.evaluate("window.localStorage.removeItem('gpui-ai:theme')");
  await cdp.evaluate("window.history.replaceState(null, '', window.location.pathname)");

  // The skip link is the first thing Tab reaches, and it moves focus rather
  // than only scrolling — which is the whole reason main carries tabindex.
  await openPage(`/components/${specimen.slug}/`, 1280, 900);
  await cdp.key("Tab", "Tab", 9);
  const skip = await cdp.evaluate("document.activeElement?.className");
  assert.equal(skip, "skip-link", "the skip link must be the first tab stop");
  await cdp.key("Enter", "Enter", 13);
  assert.equal(
    await cdp.evaluate("document.activeElement?.id"),
    "content",
    "following the skip link must move focus into the content",
  );

  // On a phone the rail stops being a sidebar, but it is the only place the
  // page carries the rustdoc link, the source link, and the reference table.
  // Laying it out with `display: none` below a breakpoint would take all of
  // that off every small screen while leaving the markup in place, which every
  // HTML-level assertion would happily match.
  await cdp.navigate(`${serverHandle.origin}/gpui-ai/components/${specimen.slug}/`, 390, 844);
  const railOnMobile = await cdp.evaluate(`(() => {
    const link = document.querySelector('.component-reference a[href*="/api/gpui_ai/"]');
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

  // Without WebGPU the *site* must say so, and — the part that matters — must
  // not have fetched anything. Every check above is on a machine that can draw;
  // this one is the promise the card makes to a machine that cannot. The stub
  // defines the property and returns nothing, which is what a browser with the
  // API disabled does, and which an `in` check would have read as yes.
  const { identifier: noGpu } = await cdp.send("Page.addScriptToEvaluateOnNewDocument", {
    source:
      "Object.defineProperty(Navigator.prototype, 'gpu', { configurable: true, get: () => undefined });",
  });
  await openPage(`/components/${specimen.slug}/`, 1280, 900);
  await waitForValue(cdp, "Boolean(document.querySelector('[data-webgpu-fallback]'))", {
    label: "the site's own WebGPU card",
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  await delay(1_000);
  assert.deepEqual(
    await cdp.evaluate(`(() => ({
      frames: document.querySelectorAll('[data-specimen-frame] iframe').length,
      requested: performance
        .getEntriesByType('resource')
        .filter((entry) => /gallery|\\.wasm$/.test(entry.name)).length,
    }))()`),
    { frames: 0, requested: 0 },
    "a browser that cannot draw the demo must not be made to download it",
  );

  // And the window is still there, with the card inside it. A machine that
  // cannot draw the component should still see where it would have been, what
  // it is called, and the controls that belong to it — not a gap in the page.
  // Nothing is starting, so nothing may say it is.
  assert.deepEqual(
    await cdp.evaluate(`(() => {
      const window_ = document.querySelector('.demo-window');
      const bar = window_?.querySelector('.demo-titlebar');
      const card = window_?.querySelector('[data-webgpu-fallback]');
      const box = window_?.getBoundingClientRect();
      return {
        windowShown: Boolean(box && box.width > 0 && box.height > 0),
        titled: bar?.querySelector('.demo-title')?.textContent?.length > 0,
        cardInsideWindow: Boolean(card),
        starting: Boolean(document.querySelector('[data-demo-starting]')),
      };
    })()`),
    { windowShown: true, titled: true, cardInsideWindow: true, starting: false },
    "a demo that cannot run must still be shown in its window, and must not claim to be starting",
  );

  // D-09, the case a poster exists for. This reader will never see the
  // component run, so the still is the only picture of it there will ever be —
  // and it carries a description, because nothing is coming to replace it.
  await waitForValue(cdp, POSTER_LOADED, {
    label: `the ${specimen.slug} fallback poster to load`,
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  const fallbackPoster = await cdp.evaluate(`(() => {
    const poster = document.querySelector('[data-specimen-frame] img[data-demo-poster]');
    const body = document.querySelector('[data-specimen-frame]');
    return {
      src: poster?.getAttribute('src') ?? null,
      naturalWidth: poster?.naturalWidth ?? 0,
      alt: poster?.getAttribute('alt') ?? null,
      hidden: poster?.getAttribute('aria-hidden') ?? null,
      frameHeight: body ? Math.round(body.getBoundingClientRect().height) : null,
    };
  })()`);
  // Nord Frost is the site's default and a dark theme; the poster follows the
  // mode rather than the theme, because there are 45 themes and two posters.
  assert.equal(fallbackPoster.src, `/gpui-ai/posters/${specimen.slug}-dark.webp`);
  assert.equal(fallbackPoster.naturalWidth, POSTER_WIDTH, "the fallback poster did not load");
  assert.equal(fallbackPoster.alt, `${specimen.windowTitle}, rendered`);
  assert.equal(fallbackPoster.hidden, null, "the only picture of the component must not be hidden");
  // Whether this browser can draw is only known after the page has mounted, so
  // the card arrives late. It must not resize the window when it does: the CLS
  // check further down would catch it on one route, this catches it here.
  assert.equal(
    fallbackPoster.frameHeight,
    specimen.height,
    "the WebGPU card must not change the height of the window it appears in",
  );

  await cdp.send("Page.removeScriptToEvaluateOnNewDocument", { identifier: noGpu });

  // D-13. A demo on a phone is not the demo on a desktop: the story is given a
  // third of the width and its prose rewraps, so the height the catalog
  // measured is far too short. The story reports what it actually laid out at
  // and the frame takes that number — without it the demo is clipped and has
  // to be scrolled inside its own canvas, which is the one gesture this site
  // has already had to fight for.
  const reflows = components.find((component) => component.slug === REFLOWING_SPECIMEN);
  assert.ok(reflows, `${REFLOWING_SPECIMEN} must be in the catalog`);
  await cdp.navigate(`${serverHandle.origin}/gpui-ai/components/${reflows.slug}/`, 390, 844);
  await waitForValue(
    cdp,
    "(() => { const frame = document.querySelector('[data-specimen-frame] iframe'); return Boolean(frame?.contentDocument?.querySelector('canvas')); })()",
    {
      label: `the ${reflows.slug} demo to start on a phone-sized window`,
      fatal: GALLERY_GAVE_UP,
      describe: GALLERY_DIAGNOSIS,
      errors,
    },
  );
  // Reserved first, so nothing on the page moves before the demo has an
  // opinion. This is the number the catalog carries.
  const narrow = await waitForValue(
    cdp,
    `Math.round(document.querySelector('[data-specimen-frame]').getBoundingClientRect().height) > ${reflows.height + 50}`,
    {
      label: `the ${reflows.slug} frame to grow to what the story reports`,
      describe: `(() => ({
        declared: ${reflows.height},
        frame: Math.round(document.querySelector('[data-specimen-frame]')?.getBoundingClientRect().height ?? 0),
        reported: document.querySelector('[data-specimen-frame] iframe')?.contentWindow?.gpuiAi?.storyHeight() ?? null,
      }))()`,
      errors,
    },
  ).then(() =>
    cdp.evaluate(`(() => ({
      frame: Math.round(document.querySelector('[data-specimen-frame]').getBoundingClientRect().height),
      reported: document.querySelector('[data-specimen-frame] iframe').contentWindow.gpuiAi.storyHeight(),
    }))()`),
  );
  assert.ok(
    narrow.frame >= narrow.reported,
    `the frame (${narrow.frame}) must be at least the tallest the story reported (${narrow.reported})`,
  );

  // And on a wide window the reported height is the catalog's, so the number
  // the page reserved was right and nothing moved.
  await cdp.navigate(`${serverHandle.origin}/gpui-ai/components/${reflows.slug}/`, 1280, 900);
  await waitForValue(
    cdp,
    `Math.round(document.querySelector('[data-specimen-frame]').getBoundingClientRect().height) === ${reflows.height}`,
    {
      // The story arrives over a few seconds and grows as it does, so this is
      // the frame settling on the tallest state — which is how the catalog's
      // own number was measured. The two agreeing is the claim: a story that
      // changed shape would move this number and the catalog would be stale.
      label: `the ${reflows.slug} frame to settle on the height the catalog declares`,
      timeoutMs: 30_000,
      fatal: GALLERY_GAVE_UP,
      describe: `(() => ({
        declared: ${reflows.height},
        frame: Math.round(document.querySelector('[data-specimen-frame]')?.getBoundingClientRect().height ?? 0),
        reported: document.querySelector('[data-specimen-frame] iframe')?.contentWindow?.gpuiAi?.storyHeight() ?? null,
      }))()`,
      errors,
    },
  );

  // D-04. A story draws its own switcher inside the canvas, so a reader can
  // change what they are looking at without the page knowing — and Copy link
  // handed over the state the story opens in rather than the one they were on.
  // The address can name a state now, and the demo says which it is showing.
  const switchable = components.find((component) => component.variants.length > 1);
  assert.ok(switchable, "the catalog must have a story with states to switch between");
  const wanted = switchable.variants[1];

  await cdp.navigate(
    `${serverHandle.origin}/gpui-ai/components/${switchable.slug}/?variant=${wanted.id}`,
    1280,
    900,
  );
  await waitForValue(
    cdp,
    "(() => { const frame = document.querySelector('[data-specimen-frame] iframe'); return Boolean(frame?.contentDocument?.querySelector('canvas')); })()",
    {
      label: `the ${switchable.slug} demo to start`,
      fatal: GALLERY_GAVE_UP,
      describe: GALLERY_DIAGNOSIS,
      errors,
    },
  );
  // The frame was pointed at the state before it started, rather than being
  // switched afterwards: a reader following a link should not watch the demo
  // open on one state and jump to another.
  assert.match(
    await cdp.evaluate(
      "document.querySelector('[data-specimen-frame] iframe').getAttribute('src')",
    ),
    new RegExp(`variant=${wanted.id}$`),
    "a link naming a state must open the frame on it",
  );
  await waitForValue(
    cdp,
    `document.querySelector('[data-specimen-frame] iframe').contentWindow.gpuiAi?.variant() === ${JSON.stringify(wanted.id)}`,
    {
      label: `the ${switchable.slug} story to be showing ${wanted.id}`,
      fatal: GALLERY_GAVE_UP,
      describe: `(() => ({
        asked: ${JSON.stringify(wanted.id)},
        showing: document.querySelector('[data-specimen-frame] iframe')?.contentWindow?.gpuiAi?.variant() ?? null,
        offers: document.querySelector('[data-specimen-frame] iframe')?.contentWindow?.gpuiAi?.variants() ?? null,
      }))()`,
      errors,
    },
  );
  // The states the gallery says it has are the states the catalog published.
  assert.deepEqual(
    await cdp.evaluate(
      "document.querySelector('[data-specimen-frame] iframe').contentWindow.gpuiAi.variants()",
    ),
    switchable.variants.map((entry) => entry.id),
    "the catalog and the running story must agree about what states exist",
  );

  // And the page hears about a change it did not make, which is what Copy link
  // needs: the switcher is inside the canvas.
  const first = switchable.variants[0];
  await cdp.evaluate(
    `document.querySelector('[data-specimen-frame] iframe').contentWindow.gpuiAi.setVariant(${JSON.stringify(first.id)})`,
  );
  await waitForValue(
    cdp,
    `document.querySelector('[data-specimen-open]').getAttribute('href').includes('variant=${first.id}')`,
    {
      label: "the page to be told which state the story switched to",
      describe: `(() => ({
        popOut: document.querySelector('[data-specimen-open]')?.getAttribute('href'),
      }))()`,
      errors,
    },
  );

  // D-04. The library's claim about reduced motion is that a run with it on
  // lands on a useful static frame rather than an empty one, and until now
  // nothing on the web could ask for it — GPUI reads the preference from the
  // platform and the web platform has none. Two pictures a second apart is the
  // only way to check "static" from out here, and it is a real check: the same
  // story with motion on fails it.
  // Orbs, because reduced motion stops animation and not the story: a
  // streaming demo keeps changing either way, because its content is still
  // arriving. This one is an ambient signal whose only movement is the
  // breathing, so a still picture of it means exactly what it looks like.
  const shimmering = "orbs";
  const twoFrames = async (motion) => {
    await cdp.navigate(
      `${serverHandle.origin}/gpui-ai/gallery/embed.html?story=${shimmering}&theme=dark${motion}`,
      900,
      500,
    );
    await waitForValue(cdp, "'ready' in document.body.dataset", {
      label: `the ${shimmering} embed to draw with motion${motion || " unpinned"}`,
      fatal: GALLERY_GAVE_UP,
      describe: GALLERY_DIAGNOSIS,
    });
    // Long enough to be past the opening reveal, which is a one-shot and
    // settles at its end state under reduced motion.
    await delay(3_000);
    const shot = () =>
      cdp
        .send("Page.captureScreenshot", { format: "png" }, 30_000)
        .then((result) => result.data);
    const first = await shot();
    await delay(1_000);
    return [first, await shot()];
  };

  const [stillA, stillB] = await twoFrames("&motion=reduced");
  assert.equal(
    await cdp.evaluate("window.gpuiAi.reducedMotion()"),
    true,
    "motion=reduced must reach the gallery, not just the address bar",
  );
  assert.equal(
    stillA,
    stillB,
    "with reduced motion asked for, a settled story must stop moving",
  );

  const [movingA, movingB] = await twoFrames("&motion=full");
  assert.equal(await cdp.evaluate("window.gpuiAi.reducedMotion()"), false);
  assert.notEqual(
    movingA,
    movingB,
    "this check is worthless unless the same story does move with motion on",
  );

  // S-12. The search is only worth having if it ranks, and only worth reaching
  // for if it is one key away. Both are browser facts: the ranking has unit
  // tests, but nothing outside a browser can say whether the box the shortcut
  // lands in is the one on screen.
  await cdp.navigate(`${serverHandle.origin}/gpui-ai/components/`, 1280, 900);
  await waitForValue(cdp, "Boolean(document.querySelector('#component-filter'))", {
    label: "the catalog to render its search box",
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  // Typed as a reader types it, rather than by setting `value` — React listens
  // for input events, and an assignment fires none.
  await cdp.evaluate(`(() => {
    const box = document.querySelector('#component-filter');
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
    setter.call(box, 'approv');
    box.dispatchEvent(new Event('input', { bubbles: true }));
  })()`);
  await waitForValue(
    cdp,
    "document.querySelectorAll('main [data-component]').length > 0 && document.querySelectorAll('main [data-component]').length < 10",
    { label: "the catalog to narrow", describe: GALLERY_DIAGNOSIS, errors },
  );
  assert.equal(
    await cdp.evaluate(
      "document.querySelector('main [data-component]').getAttribute('data-component')",
    ),
    "approval",
    "the component named for the word must come before the ones that only mention it",
  );

  // The shortcut, from the page rather than from the box. On a wide window the
  // rail is showing, so that is where the cursor belongs — a hidden input can
  // still take focus, which would put the cursor somewhere nobody can see.
  await cdp.evaluate("document.activeElement?.blur()");
  await cdp.key("/", "Slash", 191);
  assert.equal(
    await cdp.evaluate("document.activeElement?.id"),
    "rail-component-search",
    "slash must land in the search box the reader can actually see",
  );

  // And a slash typed into a field is a slash. Without this the shortcut would
  // make it impossible to search for one.
  await cdp.evaluate(`(() => {
    const box = document.querySelector('#component-filter');
    box.focus();
  })()`);
  await cdp.key("/", "Slash", 191);
  assert.equal(
    await cdp.evaluate("document.activeElement?.id"),
    "component-filter",
    "the shortcut must not steal the cursor from a field being typed into",
  );

  // S-14. A mistyped address gets the site's own page, and — the part only a
  // browser can check — hydrates as that page. The client used to fall back to
  // the first route for a path it did not recognise, so every 404 would have
  // been served as "page not found" and then quietly rebuilt as the home page
  // the moment React took over.
  const before = errors.length;
  await cdp.navigate(`${serverHandle.origin}/gpui-ai/nothing-is-here/`, 1280, 900);
  await waitForValue(cdp, "Boolean(document.querySelector('.missing-ways'))", {
    label: "an address that names nothing to get the site's own page",
    describe: GALLERY_DIAGNOSIS,
    errors,
  });
  assert.deepEqual(
    await cdp.evaluate(`(() => ({
      heading: document.querySelector('h1')?.textContent,
      ways: document.querySelectorAll('.missing-ways a').length,
      masthead: Boolean(document.querySelector('.masthead')),
      // Still the 404 after hydration. A mismatch here would have React throw
      // the server's markup away and rebuild, which is invisible in a
      // screenshot and obvious in the DOM a moment later.
      settled: document.querySelector('h1')?.textContent,
    }))()`),
    { heading: "Page not found", ways: 3, masthead: true, settled: "Page not found" },
  );
  // The 404 status is itself a console error, and is the point. Anything else
  // on this page is React saying the markup it was given was not the markup it
  // would have drawn.
  const complaints = errors
    .splice(before)
    .filter((text) => !/Failed to load resource.*404/.test(text));
  assert.deepEqual(
    complaints,
    [],
    "the 404 page hydrated with complaints, which means the client drew a different page",
  );

  // S-14. A shared link is the one part of this site nobody visiting it can
  // check, so the card behind every og:image is read off disk: the right
  // format, the size the tags claim, and named by the page that points at it.
  // A tag pointing at a card nobody rendered breaks an unfurl rather than
  // degrading it, which is worse than having no tag at all.
  assert.equal(cardsWritten.length, 2, "both cards asked for must have been written");
  for (const card of cardsWritten) {
    const bytes = await readFile(path.join(outDir, card.file));
    assert.deepEqual(
      [...bytes.subarray(0, 8)],
      [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a],
      `${card.file} is not a PNG, whatever its name says`,
    );
    // IHDR is the first chunk in every PNG, and its width and height are the
    // four bytes each at 16 and 20. Reading them needs no image library and
    // cannot be satisfied by a file that merely exists.
    assert.equal(bytes.readUInt32BE(16), CARD.width, `${card.file} is the wrong width`);
    assert.equal(bytes.readUInt32BE(20), CARD.height, `${card.file} is the wrong height`);

    const served = await cdp.evaluate(
      `new Promise((resolve) => {
        const image = new Image();
        image.onload = () => resolve(image.naturalWidth);
        image.onerror = () => resolve(0);
        image.src = ${JSON.stringify(`/gpui-ai/${card.file}`)};
      })`,
    );
    assert.equal(served, CARD.width, `${card.file} is not served where its tag says it is`);
  }

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

  // Nothing on the page may move as the faces arrive.
  //
  // The four faces come in through `@import "@fontsource/…"` inside site.css,
  // and Vite does not preload what an @import pulled in — so the chrome
  // painted in the system fallback and shifted about a second later when Plex
  // and Lilex landed. Measured cold and throttled, that was 0.0029 on the home
  // page; the build now emits a preload per face and it is zero.
  //
  // Cold and slow on purpose: with a warm cache and a local server the faces
  // arrive before first paint whether or not anything preloaded them, and this
  // would be a check that cannot fail. The themes page is left out — its three
  // demo frames race each other and the number moves run to run, which is not
  // the fonts and not something to gate.
  await cdp.send("Page.addScriptToEvaluateOnNewDocument", {
    source: `
      window.__shift = 0;
      new PerformanceObserver((list) => {
        for (const entry of list.getEntries()) {
          if (!entry.hadRecentInput) window.__shift += entry.value;
        }
      }).observe({ type: 'layout-shift', buffered: true });
    `,
  });
  await cdp.send("Network.enable");
  await cdp.send("Network.setCacheDisabled", { cacheDisabled: true });
  await cdp.send("Emulation.setEmulatedMedia", { features: [] });
  await cdp.send("Network.emulateNetworkConditions", {
    offline: false,
    latency: 120,
    downloadThroughput: 400 * 1024,
    uploadThroughput: 400 * 1024,
  });

  for (const route of ["/", `/components/${specimen.slug}/`]) {
    await cdp.navigate(`${serverHandle.origin}/gpui-ai${route}`, 1280, 900);
    await delay(5_000);
    assert.equal(
      await cdp.evaluate("window.__shift"),
      0,
      `${route} moved under the reader as the webfonts arrived`,
    );
    assert.ok(
      (await cdp.evaluate("document.querySelectorAll('link[rel=preload][as=font]').length")) > 0,
      `${route} ships no font preloads, which is what keeps that at zero`,
    );
  }

  await cdp.send("Network.emulateNetworkConditions", {
    offline: false,
    latency: 0,
    downloadThroughput: -1,
    uploadThroughput: -1,
  });
  await cdp.send("Network.setCacheDisabled", { cacheDisabled: false });
});

test("every theme the site offers can be read", {
  skip: !browserPath && !releaseGateIsMandatory
    ? "Set CHROME_PATH or install Chrome, Edge, or Chromium to run the browser gate"
    : releaseIntegrationRequested ? false : "Run npm run check:web:release for the built-artifact integration gate",
  timeout: 120_000,
}, async (context) => {
  assert.ok(browserPath, "CI runs this against a real browser, and none was found");

  // Forty-five themes, and the site paints its chrome from all of them. A
  // theme is not a picture here — it decides whether the prose, the code, and
  // the controls can be read at all, and nothing else in the suite would
  // notice a palette that puts grey text on a grey card.
  const temporaryRoot = await mkdtemp(path.join(tmpdir(), "mighty-contrast-"));
  const outDir = path.join(temporaryRoot, "site");
  const galleryDir = path.join(temporaryRoot, "gallery");
  const userDataDir = path.join(temporaryRoot, "browser");
  let serverHandle;
  let browserHandle;
  context.after(async () => {
    await settleAll([
      () => closeBrowser(browserHandle),
      () => (serverHandle ? new Promise((resolve) => serverHandle.server.close(resolve)) : undefined),
      () => rm(temporaryRoot, { force: true, recursive: true, maxRetries: 5, retryDelay: 100 }),
    ]);
  });

  // A fixture gallery, not the release artifact: this is about the chrome the
  // site paints, and booting thirty-four WebGPU canvases would prove nothing
  // about it while taking a hundred times as long.
  await createGalleryFixture(galleryDir);
  await buildSite({ galleryDir, outDir });
  serverHandle = await serve(outDir);
  browserHandle = await launchBrowser(userDataDir);
  const { cdp } = browserHandle;
  await Promise.all([cdp.send("Page.enable"), cdp.send("Runtime.enable")]);

  const own = new Set(
    themeFile.groups.find((group) => group.id === "gpui-ai").themes.map((theme) => theme.slug),
  );
  const slugs = themeFile.groups.flatMap((group) => group.themes.map((theme) => theme.slug));
  assert.ok(slugs.length > 40, `only ${slugs.length} themes were found`);

  const routes = ["/", `/components/${components[0].slug}/`, "/themes/"];
  const findings = [];
  for (const route of routes) {
    await cdp.navigate(`${serverHandle.origin}/gpui-ai${route}`, 1280, 900);
    const audit = await cdp.evaluate(auditExpression(slugs), 90_000);
    assert.ok(audit.elements > 20, `${route} has only ${audit.elements} pieces of text to check`);
    // An audit that writes the attribute and gets ignored reports a clean bill
    // of health for one theme, forty-five times. Distinct backgrounds are the
    // cheapest proof it really visited them.
    assert.ok(
      audit.palettes > slugs.length / 2,
      `${route} painted only ${audit.palettes} distinct backgrounds across ${slugs.length} themes`,
    );
    findings.push(...audit.findings.map((finding) => ({ ...finding, route })));
  }

  const ours = findings.filter((finding) => own.has(finding.theme));
  const vendored = findings.filter((finding) => !own.has(finding.theme));

  // The upstream pack is shown as published and credited, so a palette of
  // theirs that reads poorly is theirs to fix and not ours to silently
  // repaint. It is reported rather than enforced — but it is reported, so
  // nobody has to discover it from a visitor.
  if (vendored.length > 0) {
    const themes = new Set(vendored.map((finding) => finding.theme));
    process.stdout.write(
      `\n${vendored.length} contrast findings across ${themes.size} vendored themes ` +
        `(shown as published, not enforced):\n${report(vendored.slice(0, 20))}\n` +
        (vendored.length > 20 ? `…and ${vendored.length - 20} more\n` : ""),
    );
  }

  assert.deepEqual(
    ours,
    [],
    `the site's own themes must be readable:\n${report(ours)}`,
  );
});
