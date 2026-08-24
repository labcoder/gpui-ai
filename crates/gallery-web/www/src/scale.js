// GPUI's web backend takes `devicePixelRatio` as its scale factor but sizes the
// canvas's backing store in CSS pixels. On a HiDPI display that means it lays
// out for a window half the size and paints everything at double scale: a story
// the gallery measured at 52 px needs 104, so it overflows a frame the site
// sized from the catalog, and the surface is upscaled and soft either way.
//
// Pinning the ratio to 1 before the module loads makes one logical pixel one CSS
// pixel, which is the geometry every measured height assumes. It costs no
// sharpness, because the backing store was never going to provide any. Remove
// this once upstream scales the surface by the ratio it lays out with.

/**
 * Pins a window's device pixel ratio to 1.
 *
 * Returns whether it changed anything, so a caller can tell a HiDPI viewer from
 * an ordinary one.
 */
export function pinScaleFactor(view = globalThis) {
  if (view.devicePixelRatio === 1) return false;
  Object.defineProperty(view, 'devicePixelRatio', { configurable: true, get: () => 1 });
  return true;
}
