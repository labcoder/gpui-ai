// The rule that decides which theme a visitor sees.
//
// Plain JavaScript with no imports, because two very different things have to
// agree on it: the React app, and the inline script in `site/index.html` that
// runs before first paint so the page never flashes the wrong palette. The
// inline script cannot import anything, so it is written out by hand — and
// `site/test/theme.test.mjs` extracts it from the document and checks it against
// this module across every combination, so the two cannot drift apart.

/** Follow whatever the operating system asks for. */
export const SYSTEM = "system";

/**
 * What a visitor who has chosen nothing sees.
 *
 * The site opens on a named palette rather than on the machine's light or dark
 * because those two are the least interesting themes it ships: they are the
 * plain baseline every component is built against. Nord Frost is the registry
 * doing something, which is the point of a page about a theme registry, and it
 * is a dark theme, which is what most of the demos were designed against.
 *
 * `system` remains one choice among the rest, and remains the fallback when
 * this name is not one the registry ships — see `resolveChoice`.
 */
export const DEFAULT = "nord-frost";

// Theme identity lives in the Rust registry, which is generated from the
// `themes/` directory, so this checks only the shape of a name. A slug the
// registry does not know simply matches no `[data-theme]` rule, and `:root`
// keeps the page on the default — the same bet `crates/gallery-web/www/src/query.js`
// makes, and the reason adding a theme file never means editing a list here.
const SLUG = /^[a-z0-9][a-z0-9-]*$/;

function valid(value) {
  return typeof value === "string" && (value === SYSTEM || SLUG.test(value)) ? value : undefined;
}

/**
 * What the visitor has chosen, in order of how deliberate the choice was.
 *
 * A theme in the URL beats a stored one: it is how a link says "look at this in
 * Ember Dusk", and it should win for that visit without overwriting what the
 * visitor picked for themselves.
 *
 * `fallback` is what a visitor who has chosen nothing gets. It defaults to
 * [`DEFAULT`], and callers that can see the theme registry pass [`SYSTEM`]
 * instead when the registry has stopped shipping that name — the inline script
 * in `index.html` cannot see the registry, so the last `?? SYSTEM` is the same
 * safety net expressed in the rule itself.
 */
export function resolveChoice({ param, stored, fallback = DEFAULT } = {}) {
  return valid(param) ?? valid(stored) ?? valid(fallback) ?? SYSTEM;
}

/** The registry theme a choice resolves to right now. */
export function appliedTheme(choice, prefersDark) {
  if (choice !== SYSTEM) return choice;
  return prefersDark ? "dark" : "light";
}

/**
 * Whether the embedded gallery should read the page as dark.
 *
 * The embed guesses from a `dark` class on the document when the host has not
 * named a theme. Only the three presets it knows can be guessed; everything
 * else is told explicitly, and this is the fallback for the moment before that.
 */
export function isDarkTheme(applied, darkSlugs) {
  return darkSlugs.has(applied);
}
