import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { cp, mkdir, mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

const execute = promisify(execFile);
const repository = fileURLToPath(new URL("../../", import.meta.url));

// Exercise the real CLI in an isolated workspace. Only its external build
// commands and browser suites are fixtures; the orchestration is not mocked.
// Staying under ignored target/ also lets the real git provenance calls work.
async function fixture(t) {
  const temporary = path.join(repository, "target");
  await mkdir(temporary, { recursive: true });
  const root = await mkdtemp(path.join(temporary, "web-runner-test-"));
  t.after(() => rm(root, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 }));
  for (const directory of ["script", "site/test/release", "crates/gallery-web/www/dist/assets"]) {
    await mkdir(path.join(root, directory), { recursive: true });
  }
  // Keep helpers alongside the CLI so splitting a script into modules does
  // not require updating a hand-maintained dependency list in this fixture.
  await cp(path.join(repository, "script"), path.join(root, "script"), { recursive: true });
  await writeFile(path.join(root, "trace"), "");
  await writeFile(path.join(root, "crates/gallery-web/www/dist/assets/gallery.wasm"), "previous-release");
  await writeFile(path.join(root, "npm.mjs"), `
    import { appendFile, readFile, writeFile } from 'node:fs/promises';
    const phase = process.argv.at(-1) === 'build:wasm' ? 'compile' : 'host';
    await appendFile('trace', phase + '\\n');
    if (process.env.RUNNER_FIXTURE_FAIL === phase) process.exit(17);
    if (phase === 'compile') await writeFile('compiled.wasm', 'fresh-release');
    else await writeFile('crates/gallery-web/www/dist/assets/gallery.wasm', await readFile('compiled.wasm'));
  `);
  for (const suite of ["catalog", "new-workflow"]) {
    await writeFile(path.join(root, "site/test/release", `${suite}.test.mjs`), `
      import assert from 'node:assert/strict';
      import { appendFile, readFile } from 'node:fs/promises';
      import { test } from 'node:test';
      test('fixture with an arbitrary human title', async () => {
        await appendFile('trace', '${suite}\\n');
        assert.equal(await readFile('crates/gallery-web/www/dist/assets/gallery.wasm', 'utf8'), process.env.RUNNER_FIXTURE_EXPECTED);
        assert.notEqual(process.env.RUNNER_FIXTURE_FAIL, 'browser', 'injected browser failure');
      });
    `);
  }
  return root;
}

async function run(root, { fail = "", browserOnly = false } = {}) {
  const args = ["script/run-web-tests.mjs", "--system-browser", ...(browserOnly ? ["--browser-only"] : [])];
  const env = {
    ...process.env,
    npm_execpath: path.join(root, "npm.mjs"),
    GPUI_AI_WEB_EVIDENCE_ROOT: path.join(root, "evidence"),
    RUNNER_FIXTURE_FAIL: fail,
    RUNNER_FIXTURE_EXPECTED: browserOnly ? "previous-release" : "fresh-release",
  };
  // The CLI starts a separate test runner, not a nested test in this process.
  // Node otherwise inherits our runner context and silently skips its suites.
  delete env.NODE_TEST_CONTEXT;
  const result = await execute(process.execPath, args, {
    cwd: root,
    timeout: 30_000,
    env,
  }).then((output) => ({ ...output, code: 0 }), (error) => error);
  const trace = (await readFile(path.join(root, "trace"), "utf8")).trim().split("\n");
  return { ...result, trace };
}

function assertRun(trace, builds, suites = []) {
  assert.deepEqual(trace.slice(0, builds.length), builds);
  // Both suites must see the built artifact; their order is not a contract.
  assert.deepEqual(trace.slice(builds.length).sort(), [...suites].sort());
}

test("the release CLI builds fresh bytes before running every discovered suite", async (t) => {
  const root = await fixture(t);
  const result = await run(root);
  assert.equal(result.code, 0, result.stderr);
  assertRun(result.trace, ["compile", "host"], ["catalog", "new-workflow"]);
  const [evidence] = await readdir(path.join(root, "evidence"));
  const junit = await readFile(path.join(root, "evidence", evidence, "run-1/results.xml"), "utf8");
  assert.equal([...junit.matchAll(/<testcase\b/g)].length, 2);
});

for (const [phase, builds] of [
  ["compile", ["compile"]],
  ["host", ["compile", "host"]],
  ["browser", ["compile", "host"]],
]) {
  test(`a failing ${phase} command fails the release CLI without testing stale bytes`, async (t) => {
    const root = await fixture(t);
    const result = await run(root, { fail: phase });
    assert.ok(Number.isInteger(result.code) && result.code > 0, result.stderr);
    assertRun(result.trace, builds, phase === "browser" ? ["catalog", "new-workflow"] : []);
  });
}

test("browser-only validation reuses the existing artifact without rebuilding it", async (t) => {
  const root = await fixture(t);
  const result = await run(root, { browserOnly: true });
  assert.equal(result.code, 0, result.stderr);
  assertRun(result.trace, [], ["catalog", "new-workflow"]);
  assert.equal(await readFile(path.join(root, "crates/gallery-web/www/dist/assets/gallery.wasm"), "utf8"), "previous-release");
});
