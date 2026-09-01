// Decides who the wheel belongs to: this example, or the page around it.
//
// GPUI's web platform calls `preventDefault()` on every wheel event before it
// has looked at it, so a canvas swallows the wheel whether or not the story
// has anything to scroll. On a page where a demo fills most of the column that
// leaves a reader stuck: the page will not move in either direction while the
// pointer is over it, and a story already scrolled to its end simply eats the
// gesture.
//
// So the page gets the wheel by default, and this example takes it only once
// the reader has clicked into it — which is what someone about to scroll a
// transcript does anyway. Moving the pointer out gives it straight back, so
// nothing is held that was not asked for.
//
// Two listeners, and the difference between them is the whole decision:
//
// **Not ours.** Stop propagation in the capture phase at `window`, which is
// the one place upstream of the canvas. The platform's listener never runs,
// nothing calls `preventDefault`, and the browser scrolls the page as it would
// over any other frame.
//
// **Ours.** Call `preventDefault` here rather than leaving it to the platform.
// Saying the wheel is ours was not enough on its own: a story shorter than its
// frame has nothing left to scroll, and a document that cannot scroll hands
// the gesture to its parent — so the page moved under a reader who had clicked
// into the demo precisely to stop that. Taking the default keeps the scroll
// here whether or not this story has anywhere to put it. Propagation is
// untouched, so GPUI still sees the event and still scrolls its own content.
//
// `touch-action` is the same story on a touch screen, and is handled in the
// stylesheet: the canvas is created with `touch-action: none`, which stops a
// finger scrolling the page at all.

/** The dataset flag that marks this document as holding the wheel. */
export const CAPTURED = 'captured';

/**
 * What a wheel event should have done to it, given who owns the wheel.
 *
 * Returns the name of the method to call, so the decision can be read and
 * tested without a browser: `'preventDefault'` while this example holds the
 * wheel, `'stopPropagation'` while the page does.
 */
export function wheelAction(captured) {
  return captured ? 'preventDefault' : 'stopPropagation';
}

/**
 * Shares the wheel between this example and the page around it.
 *
 * `win` is the frame's own window; `tell` is handed each change so the host
 * can say out loud who has the wheel. Standalone — no parent to share with —
 * the caller keeps the wheel and never calls this.
 */
export function shareTheWheel(win, tell) {
  const body = win.document.body;
  const held = () => CAPTURED in body.dataset;

  const set = (captured) => {
    if (captured === held()) return;
    if (captured) body.dataset[CAPTURED] = '';
    else delete body.dataset[CAPTURED];
    tell(captured);
  };

  win.addEventListener(
    'wheel',
    (event) => {
      event[wheelAction(held())]();
    },
    { capture: true, passive: false },
  );

  win.document.addEventListener('pointerdown', () => set(true));
  // `pointerleave` on the document fires when the pointer leaves the frame.
  win.document.addEventListener('pointerleave', () => set(false));
  win.addEventListener('blur', () => set(false));
  tell(false);

  return { set, held };
}
