import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { before, after, test } from "node:test";
import catalog from "../../generated/catalog.json" with { type: "json" };
import { closeBrowser, closeServer, GALLERY_DIAGNOSIS, GALLERY_GAVE_UP, launchBrowser, serve, settleAll, waitForValue } from "../../scripts/cdp.mjs";
import { observeBrowser, readGpuAdapter, saveBrowserEvidence, unexpectedBrowserEvents } from "../../scripts/browser-evidence.mjs";
import { assertDraws } from "./rendered-frame.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../..");
let serverHandle;
before(async () => { serverHandle = await serve(path.join(root, "crates/gallery-web/www/dist")); });
after(async () => closeServer(serverHandle));

// Every catalog entry is tested, not just the two poster fixtures. Rotate
// three review themes across this smoke matrix; exhaustive theme/geometry
// and semantic interaction tests live in the native component suite.
for (const [ix, component] of catalog.components.entries()) {
  test(`catalog ${component.slug}: draws and exposes every declared variant`, { timeout: 120_000 }, async (context) => {
    const temporary = await mkdtemp(path.join(tmpdir(), "gpui-ai-catalog-web-"));
    let browser;
    context.after(async () => settleAll([
      () => saveBrowserEvidence(browser, `catalog-${component.slug}`),
      () => closeBrowser(browser),
      () => rm(temporary, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 }),
    ]));
    browser = await launchBrowser(path.join(temporary, "browser"));
    const { cdp } = browser;
    const events = await observeBrowser(browser);
    await cdp.send("Page.enable");
    const theme = ["light", "dark", "contrast"][ix % 3];
    await cdp.navigate(`${serverHandle.origin}/embed.html?story=${component.slug}&theme=${theme}&motion=reduced`, 640, 300);
    await waitForValue(cdp, `window.gpuiAi?.storyHeight() > 0 && document.body.dataset.ready !== undefined && window.gpuiAi.currentTheme() === '${theme}'`, {
      label: `${component.slug} boots`, fatal: GALLERY_GAVE_UP, describe: GALLERY_DIAGNOSIS,
    });
    assert.ok(await readGpuAdapter(browser), "a WebGPU adapter must be available");
    assert.deepEqual(await cdp.evaluate("window.gpuiAi.variants()"), component.variants.map(({ id }) => id));
    await assertDraws(cdp, component.slug);
    for (const { id } of component.variants) {
      assert.equal(await cdp.evaluate(`window.gpuiAi.setVariant(${JSON.stringify(id)})`), true, `select ${component.slug}/${id}`);
      await waitForValue(cdp, `window.gpuiAi.variant() === ${JSON.stringify(id)}`, { label: `${component.slug}/${id}`, describe: GALLERY_DIAGNOSIS });
      await assertDraws(cdp, `${component.slug}/${id}`);
    }
    assert.deepEqual(unexpectedBrowserEvents(events), [], `${component.slug} has no runtime/asset failures or WebGL fallback`);
  });
}
