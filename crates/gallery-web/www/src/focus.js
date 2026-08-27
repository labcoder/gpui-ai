// Keeps an embedded example from stealing its host page's focus.
//
// GPUI's web platform focuses a hidden input element the moment it creates
// its window — that is how the canvas hears the keyboard — and during later
// input-focus changes. Standalone that is correct. Embedded it is a page-wide event:
// a focus() call inside an iframe pulls the browser's focus out of the host
// document and into this frame, and the host cannot defend itself — `inert`
// on the frame's ancestors does not reach a child document focusing its own
// elements. A demo finishing its boot half a minute into the visit lands that
// grab wherever the reader happens to be, in the worst case inside the site's
// open navigation drawer, which is modal and has just promised to hold focus.
//
// So, embedded, focus is taken only when it is already this document's, or is
// being given: a real pointer or key landing in this frame. `window.event` is
// what tells those apart, and it is the right instrument because it is scoped
// to this realm — set only while an event is being dispatched to a listener
// here. Nothing the host page does can open the gate from outside: not its
// clicks, not its keys, not the user activation that same-origin frames
// share. And GPUI's own pointerdown handler calls preventDefault() before it
// focuses — suppressing the browser's native click-to-focus — so its manual
// focus() during that trusted dispatch is exactly what the gate must admit,
// and does.

/** The events whose dispatch means the reader is acting on this frame. */
export const USER_INPUT_EVENTS = Object.freeze([
  'pointerdown',
  'mousedown',
  'pointerup',
  'mouseup',
  'click',
  'touchstart',
  'touchend',
  'keydown',
  'keyup',
]);

/**
 * Whether a programmatic focus() may proceed.
 *
 * `event` is the realm's `window.event`: undefined outside any dispatch. The
 * type matters — the host legitimately posts theme messages into this frame,
 * and a `message` event is trusted without being the reader's hand — and so
 * does `isTrusted`, because a synthetic event is a script's, not a reader's.
 */
export function mayTakeFocus(documentHasFocus, event) {
  if (documentHasFocus) return true;
  return Boolean(event && event.isTrusted && USER_INPUT_EVENTS.includes(event.type));
}

/**
 * Installs the gate over `HTMLElement.prototype.focus` in the given window.
 *
 * A prototype patch, because the call being intercepted is made by generated
 * wasm-bindgen glue deep inside the platform — there is no seam of ours
 * between it and the DOM. Everything else in this document that calls focus()
 * meets the same gate, which is the same correct answer for the same reason:
 * nothing here may take the page's focus unless the reader is giving it.
 */
export function guardEmbeddedFocus(win = window) {
  const focus = win.HTMLElement.prototype.focus;
  win.HTMLElement.prototype.focus = function (...args) {
    if (!mayTakeFocus(win.document.hasFocus(), win.event)) return;
    focus.apply(this, args);
  };
}
