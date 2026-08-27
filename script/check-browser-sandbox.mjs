// Use the same launcher/profile/cleanup as the release gate, before spending
// time compiling WASM. This checks a live renderer, not just a file mode.
import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { closeBrowser, launchBrowser, settleAll } from "../site/scripts/cdp.mjs";

const profile = await mkdtemp(path.join(tmpdir(), "gpui-ai-sandbox-probe-"));
let browser;
try {
  browser = await launchBrowser(profile);
  await browser.cdp.send("Page.enable");
  await browser.cdp.send("Runtime.enable");
  await browser.cdp.navigate("data:text/html,<p>gpui-ai%20sandbox%20ready</p>", 320, 240);
  assert.equal(await browser.cdp.evaluate("document.body.textContent"), "gpui-ai sandbox ready");
  assert.ok(!browser.flags.includes("--no-sandbox"));
} finally {
  await settleAll([
    () => closeBrowser(browser),
    () => rm(profile, { recursive: true, force: true }),
  ]);
}
