import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";
import { parse } from "yaml";
import { Evaluator, Lexer, Parser, data } from "@actions/expressions";
import { reviver } from "@actions/expressions/data/reviver";

const workflow = async (name) => parse(await readFile(new URL(`../../.github/workflows/${name}.yml`, import.meta.url), "utf8"));
const expression = (value, context) => {
  if (typeof value === "boolean") return value;
  const source = String(value ?? "true").trim().replace(/^\$\{\{\s*|\s*\}\}$/g, "");
  const functions = ["always", "cancelled"].map((name) => ({ name, minArgs: 0, maxArgs: 0 }));
  const tree = new Parser(new Lexer(source).lex().tokens, Object.keys(context), functions).parse();
  const status = new Map(functions.map((fn) => [fn.name, { ...fn, call: () => new data.BooleanData(fn.name === "always") }]));
  return new Evaluator(tree, JSON.parse(JSON.stringify(context), reviver), status).evaluate().value;
};

const github = {
  event_name: "workflow_run", ref: "refs/heads/main", repository: "labcoder/gpui-ai",
  event: { workflow_run: { conclusion: "success", event: "push", head_branch: "main", head_repository: { full_name: "labcoder/gpui-ai" } } },
};

test("automatic Pages follows CI completion, never a concurrent push build", async () => {
  const pages = await workflow("pages");
  assert.equal(pages.on.push, undefined);
  assert.deepEqual(pages.on.workflow_run, { workflows: ["CI"], types: ["completed"], branches: ["main"] });
  assert.ok(Object.hasOwn(pages.on, "workflow_dispatch"));
});

test("Pages accepts only successful same-repository main pushes, or an explicit main dispatch", async () => {
  const source = (await workflow("pages")).jobs.source;
  assert.ok(source, "Pages must select a trusted source before any build or download");
  assert.equal(expression(source.if, { github }), true);
  for (const conclusion of ["failure", "cancelled", "skipped", "timed_out"]) {
    const candidate = structuredClone(github);
    candidate.event.workflow_run.conclusion = conclusion;
    assert.equal(expression(source.if, { github: candidate }), false, conclusion);
  }
  for (const mutation of [
    (g) => { g.event.workflow_run.event = "pull_request"; },
    (g) => { g.event.workflow_run.head_branch = "feature"; },
    (g) => { g.event.workflow_run.head_repository.full_name = "someone/gpui-ai"; },
  ]) {
    const candidate = structuredClone(github);
    mutation(candidate);
    assert.equal(expression(source.if, { github: candidate }), false);
  }
  assert.equal(expression(source.if, { github: { ...github, event_name: "workflow_dispatch", event: {} } }), true);
  assert.equal(expression(source.if, { github: { ...github, event_name: "workflow_dispatch", ref: "refs/heads/feature", event: {} } }), false);
});

test("publication assembly waits for every CI gate and reuses the tested gallery", async () => {
  const ci = await workflow("ci");
  const publish = ci.jobs["pages-build"];
  assert.ok(publish, "CI must assemble the publication only after its check jobs");
  assert.deepEqual([...publish.needs].sort(), ["native", "quality", "wasm"]);
  assert.equal(publish.uses, "./.github/workflows/build-site.yml");
  assert.equal(publish.with["gallery-artifact"], "tested-gallery");
  assert.equal(expression(publish.if, { github: { event_name: "push", ref: "refs/heads/main" } }), true);
  assert.equal(expression(publish.if, { github: { event_name: "pull_request", ref: "refs/pull/1/merge" } }), false);
  // Without a status override GitHub's implicit success() honors all needs.
  assert.doesNotMatch(publish.if, /always\(|failure\(|cancelled\(/);
});

test("the WASM gate stays active and uploads only after testing", async () => {
  const ci = await workflow("ci");
  const steps = ci.jobs.wasm.steps;
  const gate = steps.findIndex((step) => /npm\s+run\s+test:web:browser/.test(step.run ?? ""));
  assert.ok(gate >= 0);
  assert.equal(expression(steps[gate].if, {}), true);
  assert.ok(!steps[gate]["continue-on-error"]);
  const upload = steps.findIndex((step) => step.with?.name === "tested-gallery");
  assert.ok(upload > gate, "a compile alone must not produce a reusable gallery");
  assert.ok(steps.some((step) => step.uses === "./.github/actions/setup-web-browser"));
});

test("the shared builder compiles and tests only on its standalone path", async () => {
  const build = await workflow("build-site");
  const steps = build.jobs.build.steps;
  const standalone = steps.find((step) => /npm\s+run\s+build:wasm/.test(step.run ?? ""));
  const download = steps.find((step) => step.uses?.startsWith("actions/download-artifact@"));
  assert.ok(standalone && download);
  assert.equal(expression(standalone.if, { inputs: { "gallery-artifact": "tested-gallery" } }), false);
  assert.equal(expression(download.if, { inputs: { "gallery-artifact": "tested-gallery" } }), true);
  assert.equal(expression(standalone.if, { inputs: { "gallery-artifact": "" } }), true);
  assert.equal(expression(download.if, { inputs: { "gallery-artifact": "" } }), false);
  assert.match(standalone.run, /test:web:browser/);
  assert.ok(!standalone["continue-on-error"]);
  const assembly = steps.find((step) => /pages-artifact\.mjs seal/.test(step.run ?? ""));
  assert.ok(assembly);
  assert.doesNotMatch(assembly.run, /build:wasm|build:web/);
  assert.ok(steps.indexOf(assembly) > steps.indexOf(standalone));
});

test("manual failure and missing source block import; automatic import uses the selected run", async () => {
  const pages = await workflow("pages");
  const needs = { source: { outputs: { sha: "a".repeat(40), "run-id": "123" } }, manual: { result: "skipped" } };
  assert.equal(expression(pages.jobs.prepare.if, { github, needs }), true);
  for (const result of ["failure", "cancelled", "skipped"]) {
    assert.equal(expression(pages.jobs.prepare.if, { github: { ...github, event_name: "workflow_dispatch" }, needs: { ...needs, manual: { result } } }), false);
  }
  assert.equal(expression(pages.jobs.prepare.if, { github: { ...github, event_name: "workflow_dispatch" }, needs: { ...needs, manual: { result: "success" } } }), true);
  assert.equal(expression(pages.jobs.prepare.if, { github, needs: { ...needs, source: { outputs: { sha: "" } } } }), false);
  const download = pages.jobs.prepare.steps.find((step) => step.uses?.startsWith("actions/download-artifact@"));
  assert.equal(expression(download.with["run-id"], { needs }), "123");
  assert.ok(download.with["github-token"]);
  assert.equal(download.with.name, "pages-site");
  assert.ok(pages.jobs.deploy.needs.includes("prepare"));
  const deploy = pages.jobs.deploy.steps.find((step) => step.uses?.startsWith("actions/deploy-pages@"));
  for (const current of ["false", ""]) {
    assert.equal(expression(deploy.if, { steps: { current: { outputs: { current } } } }), false);
  }
  assert.equal(expression(deploy.if, { steps: { current: { outputs: { current: "true" } } } }), true);
});

test("source selection binds the triggering commit and skips superseded runs", async () => {
  const pages = await workflow("pages");
  const script = pages.jobs.source.steps.find((step) => step.id === "source").with.script;
  const execute = new (Object.getPrototypeOf(async function () {}).constructor)("context", "github", "core", script);
  const sha = "a".repeat(40);
  for (const [event, current, expected] of [
    [{ workflow_run: { head_sha: sha, id: 123 } }, sha, { sha, "run-id": "123" }],
    [{ workflow_run: { head_sha: sha, id: 123 } }, "b".repeat(40), {}],
    [{}, sha, { sha, "run-id": "456" }],
  ]) {
    const outputs = {};
    await execute({ payload: event, sha, runId: 456, repo: { owner: "labcoder", repo: "gpui-ai" } },
      { rest: { repos: { getBranch: async () => ({ data: { commit: { sha: current } } }) } } },
      { notice() {}, setOutput: (name, value) => { outputs[name] = value; } });
    assert.deepEqual(outputs, expected);
  }
});

test("deployment rechecks main after the environment approval wait", async () => {
  const pages = await workflow("pages");
  const script = pages.jobs.deploy.steps.find((step) => step.id === "current").with.script;
  const execute = new (Object.getPrototypeOf(async function () {}).constructor)("github", "context", "core", "process", script);
  const sha = "a".repeat(40);
  for (const current of [sha, "b".repeat(40)]) {
    const outputs = {};
    await execute(
      { rest: { repos: { getBranch: async () => ({ data: { commit: { sha: current } } }) } } },
      { repo: { owner: "labcoder", repo: "gpui-ai" } },
      { setOutput: (name, value) => { outputs[name] = value; } },
      { env: { SITE_SHA: sha } },
    );
    assert.equal(outputs.current, String(current === sha));
  }
});

test("deployment requires successful preparation even when the manual ancestor was skipped", async () => {
  const deploy = (await workflow("pages")).jobs.deploy;
  // A status function prevents the skipped optional job from poisoning the
  // dependency chain, but it must not allow a failed import to deploy.
  assert.match(deploy.if ?? "", /cancelled\(/);
  const needs = { source: { result: "success" }, prepare: { result: "success" } };
  assert.equal(expression(deploy.if, { needs }), true);
  for (const job of ["source", "prepare"]) {
    for (const result of ["failure", "cancelled", "skipped"]) {
      assert.equal(expression(deploy.if, { needs: { ...needs, [job]: { result } } }), false);
    }
  }
});

test("repackaging uses a new artifact name, but a deploy-only retry uses the prepared name", async () => {
  const pages = await workflow("pages");
  const prepare = pages.jobs.prepare;
  const upload = prepare.steps.find((step) => step.uses?.startsWith("actions/upload-pages-artifact@"));
  const deploy = pages.jobs.deploy.steps.find((step) => step.uses?.startsWith("actions/deploy-pages@"));
  assert.ok(upload.with.name, "immutable Pages artifacts need a unique name per packaging attempt");
  const first = expression(upload.with.name, { github: { run_attempt: 1 } });
  const retry = expression(upload.with.name, { github: { run_attempt: 2 } });
  assert.notEqual(first, retry);
  assert.equal(expression(prepare.outputs["artifact-name"], { github: { run_attempt: 1 } }), first);
  assert.equal(expression(deploy.with.artifact_name, {
    github: { run_attempt: 2 },
    needs: { prepare: { outputs: { "artifact-name": first } } },
  }), first);
});
