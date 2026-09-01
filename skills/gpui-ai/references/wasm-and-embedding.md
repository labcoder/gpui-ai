# WASM and embedding

The native GPUI runtime is the authoritative behavior. A browser build is a
real target with different clocks, font loading, assets, focus, and input—not a
CSS wrapper around the native pixels.

Read the exact GPUI web-platform source at the application's locked revision;
that surface changes faster than native GPUI.

## Build the target early

If the application ships WebAssembly, add the target check before polishing
the UI:

```sh
cargo check --target wasm32-unknown-unknown
```

Run the real browser host as well. A target check cannot prove fonts loaded,
touch input reaches the intended control, a canvas has a useful size, or a
popup escapes its host.

## Clocks

Do not call `std::time::Instant::now()` in code that executes on WASM. Use a
target-compatible clock such as `web_time::Instant`, or a small cfg-selected
clock abstraction shared by native and browser targets.

Application work should still drive `Progressive<T>` transitions. A compatible
clock is for elapsed display, scheduling, and animation math; it is not a reason
to simulate model or tool progress inside a component.

## Fonts are canvas resources

The GPUI canvas cannot use a font merely because the surrounding HTML page
loaded it with CSS. Supply fonts through the GPUI asset/text system used by the
locked web-platform revision. If that revision requires explicit font bytes,
register the needed faces before opening a window.

Verify actual glyph pixels in browser tests. Furniture can render successfully
around missing text, making a screenshot look like a layout bug rather than a
font database failure.

Keep theme font-family names aligned with the faces registered in GPUI. Test
regular, semibold, italic, and monospace text that the application actually
uses rather than assuming one regular sample proves the family.

## Assets and base paths

Native applications can commonly provide `gpui_component_assets::Assets`
directly. A browser host may need an asset provider constructed with the URL
base where icons and other files are deployed. Use the API at the locked
revision and test a non-root deployment path, such as GitHub Pages.

Do not bake the website's current origin into a reusable component or asset
path.

## Focus, keyboard, pointer, and touch

Test a component inside the actual embed, not only as the full browser page.
The host and canvas must agree about focus and event ownership.

- A direct activation of an editable control may summon the software keyboard;
  clicking unrelated canvas content must not.
- Pointer capture and wheel handling should stop only the event the component
  actually consumes.
- Touch targets need the same typed intent as mouse clicks.
- Keyboard activation and Escape dismissal remain required even when the
  browser demo is primarily touched.
- An overlay above content must pass input through unless it is itself an
  intentional control.

Test at mobile viewport sizes with a real touch-capable browser when mobile web
is a supported surface.

## Browser limitations

WebGPU availability, browser scheduling, and platform text behavior can differ
from native GPUI. Document a limitation rather than weakening native behavior
to make a showcase pass. Conversely, do not dismiss an input, font, or focus
failure as a demo problem when the application intends to ship the browser
target.
