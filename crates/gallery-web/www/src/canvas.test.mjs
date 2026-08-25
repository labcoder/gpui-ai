import assert from 'node:assert/strict';
import test from 'node:test';
import { hasDrawn } from './canvas.js';

/** A canvas gpui_web has created but never drawn into. */
const pristine = (clientWidth, clientHeight) => ({
  width: 300,
  height: 150,
  clientWidth,
  clientHeight,
});

test('a canvas nothing has drawn into is never shown', () => {
  // Desktop: the default backing store does not cover the box, so the size
  // test alone would have caught this one.
  assert.equal(hasDrawn(pristine(898, 436)), false);

  // A narrow phone with a short story — 320px wide, a 52px demo — is the case
  // the size test alone gets wrong: 300x150 covers 288x52 on both axes. Shown
  // here, the reader gets a black rectangle where the component should be.
  assert.equal(hasDrawn(pristine(288, 52)), false);
  assert.equal(hasDrawn(pristine(299, 149)), false);
  assert.equal(hasDrawn(pristine(1, 1)), false);
});

test('a canvas with no box has not been drawn into either', () => {
  assert.equal(hasDrawn({ width: 898, height: 436, clientWidth: 0, clientHeight: 0 }), false);
});

test('the sizes the surface passes through on the way up are not a frame', () => {
  // Seeded at 0x0 and clamped to 1x1 before the ResizeObserver reports the
  // real geometry. Stretching one pixel across the window is its own flash.
  assert.equal(hasDrawn({ width: 1, height: 1, clientWidth: 898, clientHeight: 436 }), false);
  assert.equal(hasDrawn({ width: 898, height: 1, clientWidth: 898, clientHeight: 436 }), false);
});

test('a canvas drawn at the size of its box is shown', () => {
  assert.equal(hasDrawn({ width: 898, height: 436, clientWidth: 898, clientHeight: 436 }), true);
  // Small boxes are fine once they have actually been drawn into.
  assert.equal(hasDrawn({ width: 288, height: 52, clientWidth: 288, clientHeight: 52 }), true);
  // And a device pixel ratio above 1 covers the box rather than matching it,
  // which is why this asks whether it covers rather than whether it equals.
  assert.equal(hasDrawn({ width: 1796, height: 872, clientWidth: 898, clientHeight: 436 }), true);
});
