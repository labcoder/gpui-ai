import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { guardEmbeddedFocus, mayTakeFocus, USER_INPUT_EVENTS } from './focus.js';

// The failure this file pins down was a race observable only in CI: the boot
// of the WASM platform focuses a hidden input, and run 33034197651 lost the
// site drawer's focus to it between two keystrokes. The browser gate now
// waits out a boot behind the open drawer and asserts focus held; these tests
// hold the decision itself still, input by input, where the gate can only
// sample one timing.

/** A window with a recording HTMLElement prototype, unfocused by default. */
function stubWindow() {
  const applied = [];
  class HTMLElement {}
  HTMLElement.prototype.focus = function (...args) {
    applied.push({ target: this, args });
  };
  const win = {
    HTMLElement,
    document: { hasFocus: () => false },
    event: undefined,
  };
  return { win, applied };
}

test('a document that does not hold focus cannot take it on its own', () => {
  assert.equal(mayTakeFocus(false, undefined), false);
});

test('a document that already holds focus keeps its normal focus calls', () => {
  assert.equal(mayTakeFocus(true, undefined), true);
});

test('every reader input event opens the gate while it is being dispatched', () => {
  for (const type of USER_INPUT_EVENTS) {
    assert.equal(
      mayTakeFocus(false, { type, isTrusted: true }),
      true,
      `${type} is the reader acting on this frame`,
    );
  }
});

test('a trusted event that is not the reader acting does not open the gate', () => {
  // The host posts theme changes into the frame; the dispatch is trusted and
  // still nobody's hand.
  assert.equal(mayTakeFocus(false, { type: 'message', isTrusted: true }), false);
});

test('a synthetic input event does not open the gate', () => {
  assert.equal(mayTakeFocus(false, { type: 'pointerdown', isTrusted: false }), false);
});

test('the installed gate swallows the boot-time grab', () => {
  const { win, applied } = stubWindow();
  guardEmbeddedFocus(win);
  new win.HTMLElement().focus();
  assert.equal(applied.length, 0, 'an unfocused document must not take focus at boot');
});

test('the installed gate passes an admitted call through whole', () => {
  const { win, applied } = stubWindow();
  guardEmbeddedFocus(win);
  win.event = { type: 'pointerdown', isTrusted: true };
  const element = new win.HTMLElement();
  element.focus({ preventScroll: true });
  assert.equal(applied.length, 1);
  assert.equal(applied[0].target, element, 'the receiver must survive the patch');
  assert.deepEqual(applied[0].args, [{ preventScroll: true }], 'so must the options');
});

test('the installed gate defers to a document that holds focus', () => {
  const { win, applied } = stubWindow();
  win.document.hasFocus = () => true;
  guardEmbeddedFocus(win);
  new win.HTMLElement().focus();
  assert.equal(applied.length, 1, 'a focused document behaves as if unpatched');
});

test('the embed installs the gate, and before the platform can boot', () => {
  const main = readFileSync(new URL('./main.js', import.meta.url), 'utf8');
  assert.match(
    main,
    /from '\.\/focus\.js'/,
    'main.js must take the gate from focus.js rather than restating it',
  );
  const guarded = main.indexOf('guardEmbeddedFocus()');
  const booted = main.indexOf("import('./wasm/gallery_web.js')");
  assert.ok(guarded !== -1, 'the embed must install the gate');
  assert.ok(booted !== -1, 'the boot this suite reasons about must still exist');
  assert.ok(
    guarded < booted,
    'a gate installed after the platform booted has already missed the grab',
  );
});
