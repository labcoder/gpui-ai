import assert from 'node:assert/strict';
import test from 'node:test';
import { pinScaleFactor } from './scale.js';

test('a HiDPI window is pinned to one logical pixel per CSS pixel', () => {
  const view = { devicePixelRatio: 2 };

  assert.equal(pinScaleFactor(view), true);
  assert.equal(view.devicePixelRatio, 1);
});

test('the pin holds against a later read, not just the first', () => {
  const view = { devicePixelRatio: 3 };
  pinScaleFactor(view);

  // GPUI reads the ratio while it starts and again when the surface resizes.
  // A one-shot assignment that something else overwrote would put the demo
  // back at triple scale halfway through a session.
  assert.equal(view.devicePixelRatio, 1);
  assert.equal(view.devicePixelRatio, 1);
});

test('an ordinary display is left alone', () => {
  const view = { devicePixelRatio: 1 };

  assert.equal(pinScaleFactor(view), false);
  assert.equal(view.devicePixelRatio, 1);
});

test('the pin can be undone, so a fixed upstream can take the ratio back', () => {
  const view = { devicePixelRatio: 2 };
  pinScaleFactor(view);

  Object.defineProperty(view, 'devicePixelRatio', { configurable: true, value: 2 });
  assert.equal(view.devicePixelRatio, 2);
});
