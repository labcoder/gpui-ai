import assert from "node:assert/strict";
import { test } from "node:test";
import { Cdp, settleAll, waitForValue } from "../scripts/cdp.mjs";
import { posterFrameLooksReal } from "../scripts/capture-posters.mjs";

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
