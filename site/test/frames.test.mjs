import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";

import {
  clearSeats,
  distanceAway,
  dropSeat,
  LIVE_LIMIT,
  seated,
  wantSeat,
} from "../app/frames.mjs";

/** A stand-in for one `Demo`, remembering the last thing it was told. */
function seat(name) {
  const it = { name, running: false, told: [] };
  it.live = (running) => {
    it.running = running;
    if (it.told[it.told.length - 1] !== running) it.told.push(running);
  };
  return it;
}

beforeEach(() => clearSeats());

test("no more than the limit run at once, however many ask", () => {
  const asking = Array.from({ length: LIVE_LIMIT + 2 }, (_, index) => seat(`demo-${index}`));
  // All at the same distance, so nothing but the order they asked in can
  // decide — which is the case a page of demos in one column produces.
  for (const one of asking) wantSeat(one, 0);

  assert.equal(
    asking.filter((one) => one.running).length,
    LIVE_LIMIT,
    "the whole point of the governor is that this is bounded",
  );
  assert.deepEqual(
    asking.slice(0, LIVE_LIMIT).map((one) => one.running),
    Array(LIVE_LIMIT).fill(true),
    "ties keep the order they asked in",
  );
});

test("the nearest demos are the ones that run", () => {
  const near = seat("near");
  const far = seat("far");
  const middle = seat("middle");
  // Deliberately asked worst-first: arriving early must not beat being closer
  // to the reader, or scrolling down a page would leave the demos behind you
  // running and the one you are looking at idle.
  wantSeat(far, 4_000);
  wantSeat(middle, 900);
  wantSeat(near, 0);

  assert.deepEqual(
    seated().map((one) => one.name),
    ["near", "middle", "far"].slice(0, LIVE_LIMIT),
  );
});

test("a demo evicted by a nearer one takes its seat back when that one leaves", () => {
  const asking = Array.from({ length: LIVE_LIMIT + 1 }, (_, index) => seat(`demo-${index}`));
  for (const [index, one] of asking.entries()) wantSeat(one, index * 100);
  const evicted = asking[asking.length - 1];
  assert.equal(evicted.running, false);

  // The one in front of it scrolls away. Nothing re-asks on the evicted demo's
  // behalf — it is still wanting a seat, and the ranking simply reaches it.
  dropSeat(asking[0]);

  assert.equal(evicted.running, true, "an evicted demo must not be stuck idle for good");
});

test("a demo is told to stop exactly once when it is evicted", () => {
  const first = seat("first");
  wantSeat(first, 2_000);
  for (let index = 0; index < LIVE_LIMIT; index += 1) wantSeat(seat(`nearer-${index}`), 0);

  // Every settle re-tells every seat, and React would re-render on each one if
  // the value really changed. It does not: only the transitions are recorded.
  assert.deepEqual(first.told, [true, false]);
});

test("a demo that stops asking is told to stop", () => {
  const one = seat("one");
  wantSeat(one, 0);
  assert.equal(one.running, true);

  // Nothing else is asking, so nothing else will be ranked — and a seat that
  // has been removed from the ranking is not there to be told about it. This
  // is the case where a demo scrolls out of range on a page of its own.
  dropSeat(one);

  assert.equal(one.running, false, "a demo left running is a WASM instance nobody freed");
});

test("dropping a seat that never asked changes nothing", () => {
  const one = seat("one");
  const stranger = seat("stranger");
  wantSeat(one, 0);
  dropSeat(stranger);

  assert.equal(one.running, true);
  assert.deepEqual(stranger.told, [], "a seat that never asked must not be told anything");
});

test("distance is zero on screen and the gap once off it", () => {
  const viewport = 900;

  assert.equal(distanceAway({ top: 100, bottom: 400 }, viewport), 0, "fully on screen");
  assert.equal(distanceAway({ top: -50, bottom: 200 }, viewport), 0, "half off the top");
  assert.equal(distanceAway({ top: 800, bottom: 1_200 }, viewport), 0, "half off the bottom");
  assert.equal(distanceAway({ top: 1_100, bottom: 1_400 }, viewport), 200, "below the fold");
  assert.equal(distanceAway({ top: -700, bottom: -300 }, viewport), 300, "scrolled past");
});
