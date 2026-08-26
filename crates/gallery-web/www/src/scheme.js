// The one decision table for which colour scheme an example paints in.
//
// Two places answer this question, and they must answer it identically: the
// inline script in `embed.html`'s head, which runs before first paint and
// before any module loads, and the module runtime in `main.js`, which owns
// the answer from then on. A transparent iframe whose used scheme differs
// from its embedder's is composited opaque — solid white over a dark host —
// so a drift between the two copies is not a cosmetic bug.
//
// The inline copy cannot import anything: being inline and synchronous is its
// entire job. So the module half resolves through this table, and
// `scheme.test.mjs` executes the real inline source — authored and built —
// across the full input matrix and holds it to these exact answers.

/** The theme names whose mode this document knows without asking anyone. */
export const NAMED_MODES = Object.freeze({ light: false, dark: true, contrast: true });

/**
 * Resolves the scheme for a theme name the address may or may not carry.
 *
 * Precedence, most specific first: the theme's own known mode, then the
 * host's `dark` marker (when there is a same-origin host to ask), then the
 * viewer's preference. `parentIsDark` is `undefined` when there is no host or
 * it cannot be asked — popped out, or cross-origin.
 */
export function schemeFor(theme, parentIsDark, systemPrefersDark) {
  const known = theme === undefined || theme === null ? undefined : NAMED_MODES[theme];
  const dark = known !== undefined ? known : parentIsDark !== undefined ? parentIsDark : systemPrefersDark;
  return { dark: Boolean(dark), contrast: theme === 'contrast' };
}
