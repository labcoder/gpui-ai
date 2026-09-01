import assert from 'node:assert/strict';
import test from 'node:test';
import { CAPTURED, shareTheWheel, wheelAction } from './wheel.js';

// The failure these pin down is one the browser gate could only catch by
// accident: while the demo held the wheel, nothing in this frame acted on the
// event at all — the code trusted GPUI's own `preventDefault` to stop the
// scroll. A story shorter than its frame has nothing to scroll, so the browser
// handed the gesture to the parent and the page moved under a reader who had
// clicked in to stop exactly that. Whether the gate noticed depended on where
// the demo happened to sit on the page, which is not a test.

/** A window that records what its listeners do to the events it is given. */
function stubWindow() {
  const listeners = new Map();
  const documentListeners = new Map();
  const told = [];
  const win = {
    document: {
      body: { dataset: {} },
      addEventListener: (type, handler) => documentListeners.set(type, handler),
    },
    addEventListener: (type, handler) => listeners.set(type, handler),
  };
  const wheel = () => {
    const calls = [];
    const event = {
      preventDefault: () => calls.push('preventDefault'),
      stopPropagation: () => calls.push('stopPropagation'),
    };
    listeners.get('wheel')(event);
    return calls;
  };
  return { win, told, wheel, fire: (type) => (documentListeners.get(type) ?? listeners.get(type))() };
}

test('the page keeps the wheel until someone clicks into the frame', () => {
  assert.equal(wheelAction(false), 'stopPropagation');
});

test('a frame that holds the wheel takes the default rather than trusting it', () => {
  assert.equal(wheelAction(true), 'preventDefault');
});

test('an untouched frame lets the wheel out to the page', () => {
  const { win, told, wheel } = stubWindow();
  shareTheWheel(win, (captured) => told.push(captured));

  assert.deepEqual(told, [false], 'the host is told who has the wheel at the start');
  assert.deepEqual(wheel(), ['stopPropagation']);
});

test('clicking in takes the wheel, and the scroll stops here', () => {
  const { win, told, wheel, fire } = stubWindow();
  shareTheWheel(win, (captured) => told.push(captured));

  fire('pointerdown');
  assert.equal(CAPTURED in win.document.body.dataset, true);
  assert.deepEqual(told, [false, true]);

  // The whole point: the frame acts on the event itself. A story with nothing
  // left to scroll would otherwise hand the gesture to the page.
  assert.deepEqual(wheel(), ['preventDefault']);
});

test('the pointer leaving gives the wheel straight back', () => {
  const { win, told, wheel, fire } = stubWindow();
  shareTheWheel(win, (captured) => told.push(captured));

  fire('pointerdown');
  fire('pointerleave');
  assert.equal(CAPTURED in win.document.body.dataset, false);
  assert.deepEqual(told, [false, true, false]);
  assert.deepEqual(wheel(), ['stopPropagation']);
});

test('losing the window gives it back too, and says so once', () => {
  const { win, told, fire } = stubWindow();
  shareTheWheel(win, (captured) => told.push(captured));

  fire('pointerdown');
  fire('blur');
  fire('blur');
  assert.deepEqual(told, [false, true, false], 'a state that did not change is not announced');
});
