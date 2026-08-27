import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";

test("root workflows expose and compose site checks and builds", async () => {
  const packageJson = JSON.parse(
    await readFile(new URL("../../package.json", import.meta.url), "utf8"),
  );
  const { scripts } = packageJson;

  // The site is TypeScript now, so the gate has to typecheck it, not only run
  // the node tests.
  assert.match(scripts["check:site"], /npm --prefix site run typecheck/);
  assert.match(scripts["check:site"], /npm --prefix site test/);
  assert.equal(scripts["build:site"], "npm --prefix site run build");
  assert.equal(scripts["check:web:release"], "node script/check-web-release.mjs");
  assert.match(scripts["check:web"], /check:web:release/);
  assert.equal(scripts["check:prepush"], "npm run check && npm run check:web");
  assert.match(scripts["test:web:browser"], /--browser-only/);
  assert.match(scripts["build:web"], /build:site/);
});

test("CI and Pages test the built artifact with the same pinned browser and retain evidence", async () => {
  for (const workflow of ["ci", "pages"]) {
    const source = await readFile(new URL(`../../.github/workflows/${workflow}.yml`, import.meta.url), "utf8");
    assert.match(source, /GPUI_AI_WEB_GPU: software/);
    assert.match(source, /npm run setup:web-browser/);
    assert.match(source, /xvfb xauth libvulkan1 mesa-vulkan-drivers/);
    assert.match(source, /CHROME_DEVEL_SANDBOX: \/opt\/google\/chrome\/chrome-sandbox/);
    assert.match(source, /test -u "\$CHROME_DEVEL_SANDBOX"/);
    assert.doesNotMatch(source.replace(/^\s*#.*$/gm, ""), /--no-sandbox|sysctl\s+-w/, "the gate must not disable the browser sandbox or host restrictions");
    assert.match(source, /if: always\(\)\s+uses: actions\/upload-artifact@/);
    assert.match(source, /path: \$\{\{ runner.temp \}\}\/gpui-ai-web-evidence\//, "evidence must not come from the Cargo cache");
    const build = source.indexOf("npm run build:wasm");
    const gate = source.indexOf("npm run test:web:browser");
    assert.ok(build > 0 && gate > build, `${workflow} must build before its browser gate`);
    assert.ok(!source.slice(gate).includes("npm run build:wasm"), "do not replace the tested WASM artifact");
    if (workflow === "pages") assert.ok(source.indexOf("uses: actions/upload-pages-artifact@") > gate);
  }
  const build = await readFile(new URL("../../crates/gallery-web/scripts/build-wasm.sh", import.meta.url), "utf8");
  assert.match(build, /cargo build --locked/);
  assert.doesNotMatch(build, /^\s*(?:if command -v )?wasm-opt\b/m, "PATH must not silently change the tested build pipeline");
});
