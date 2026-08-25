// Which demos are allowed to be running.
//
// Every live demo is one instance of the shared gallery binary — a seventeen
// megabyte module, its own WASM heap, and a WebGPU surface — so a page cannot
// simply start them all and leave them started. Two rules keep that bounded:
//
//   - a demo more than a viewport away stops. `Demo` observes its own frame
//     and stops wanting a seat the moment it is that far off, which is the
//     rule that actually does the work on a long page.
//   - at most `LIVE_LIMIT` run at once, whatever the page asks for. Nothing
//     the site ships puts more than three demos within a viewport of each
//     other, so this is a backstop rather than a daily constraint; it exists so
//     that a page which one day does is slow, not fatal.
//
// The seats are ranked by distance, so when more demos want to run than may,
// the ones a reader is actually looking at win. Ranking rather than
// first-come also means an evicted demo takes its seat back on its own once a
// nearer one leaves — there is no queue to get stuck in.
//
// Module state on purpose. The constraint is on the machine, not on any one
// React tree, and a governor scoped to a component could not see the other
// demos it is competing with.

/**
 * How many demos may run at once.
 *
 * Three, because `/themes/` shows the same three components side by side and
 * the whole point of that row is comparing them at a glance.
 */
export const LIVE_LIMIT = 3;

/** Seats that want to run, in the order they asked, with how far off they are. */
const wanted = new Map();

/**
 * Asks for this seat to be running, or updates how far away it is.
 *
 * `away` is the gap in pixels between the frame and the viewport, zero for a
 * frame on screen. It is what the ranking sorts on.
 */
export function wantSeat(seat, away) {
  wanted.set(seat, away);
  settle();
}

/** Gives up the seat. A demo that is not asking cannot be told to run. */
export function dropSeat(seat) {
  if (!wanted.delete(seat)) return;
  // Told first, and by name. `settle` only reaches the seats still asking, so
  // a seat that has just left would otherwise never hear that it has stopped —
  // and would keep its frame, which is the whole thing this file prevents.
  seat.live(false);
  settle();
}

/** Every seat, nearest first. Ties keep the order they asked in. */
function ranked() {
  return [...wanted.entries()].sort(([, a], [, b]) => a - b).map(([seat]) => seat);
}

function settle() {
  ranked().forEach((seat, index) => seat.live(index < LIVE_LIMIT));
}

/** What is running right now. For tests; nothing on the page asks. */
export function seated() {
  return ranked().slice(0, LIVE_LIMIT);
}

/** Forgets every seat. For tests, which share this module between cases. */
export function clearSeats() {
  wanted.clear();
}

/**
 * How far a frame is from the viewport, in pixels, from an observer entry.
 *
 * Zero while any part of it is on screen. `rootBounds` is the *expanded* root
 * once a root margin is in play — it would report every frame the observer
 * fires for as being at zero — so this measures against the real viewport.
 */
export function distanceAway(box, viewportHeight) {
  return Math.max(0, -box.bottom, box.top - viewportHeight);
}
