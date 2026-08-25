// When the GPUI canvas is safe to show.
//
// Its own module so it can be tested: the alternative is exercising it through
// main.js, which starts the gallery on import.

/**
 * The backing store a canvas has when nothing has drawn into it.
 *
 * `gpui_web`'s `prepare_canvas` sets only inline styles — width and height of
 * 100% — and never touches the `width`/`height` content attributes, so a canvas
 * it has just created carries the sizes the HTML default gives it.
 */
const PRISTINE = { width: 300, height: 150 };

/**
 * Whether GPUI has drawn a frame covering this canvas's box.
 *
 * The canvas cannot simply be shown as soon as it exists: `gpui_web`
 * configures its surface with `transparent: false`, which is
 * `CompositeAlphaMode::Opaque`, so until a frame is presented the compositor
 * reads it as solid black.
 *
 * Three things have to be true, and each of them rules out a state this
 * actually passes through:
 *
 * - It is not still pristine. On a narrow phone a demo's box can be smaller
 *   than 300x150, and then an untouched canvas covers it on both axes — the
 *   size test alone would show black.
 * - Its box has a size at all. Rendering is skipped while the physical size is
 *   zero, so a canvas with no box has certainly not been drawn into.
 * - The backing store covers the box. The surface is seeded at 0x0, clamped to
 *   1x1, and only resized once the `ResizeObserver` reports real geometry;
 *   stretching that one pixel across the window is its own kind of flash.
 *   Covers rather than equals, so a device pixel ratio above 1 still passes.
 *
 * @param {{ width: number, height: number, clientWidth: number, clientHeight: number }} canvas
 */
export function hasDrawn(canvas) {
  const pristine = canvas.width === PRISTINE.width && canvas.height === PRISTINE.height;
  return (
    !pristine &&
    canvas.clientWidth > 0 &&
    canvas.width >= canvas.clientWidth &&
    canvas.height >= canvas.clientHeight
  );
}
