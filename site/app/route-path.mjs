// Path normalisation shared by the pre-render, the browser, and the tests.
//
// Plain JavaScript on purpose: the node test imports it directly, so the rule
// that decides which route a URL means is covered by a real test rather than
// only by the typechecker.

/**
 * Reduces a URL path to the canonical route path.
 *
 * A link may name the file the server returns, so `/components/chat/index.html`
 * has to mean the same route as `/components/chat/`. If the browser disagreed
 * with the pre-render here, hydration would fail and React would silently
 * rebuild the page as whatever route the client picked instead.
 *
 * @param {string} pathname
 * @param {string} [base] the site's base path, for example `/gpui-ai/`
 * @returns {string} a path that always starts and ends with `/`
 */
export function normalizeRoutePath(pathname, base = "/") {
  const prefix = base.replace(/\/$/, "");
  let path = prefix && pathname.startsWith(prefix) ? pathname.slice(prefix.length) : pathname;
  path = path.replace(/index\.html$/, "");
  if (!path.startsWith("/")) path = `/${path}`;
  if (!path.endsWith("/")) path = `${path}/`;
  return path;
}

/**
 * The file name a route's social card is published under.
 *
 * A route path is a directory tree and a file name cannot be, so this flattens
 * one to the other: `/` is `home`, `/components/chat/` is `components-chat`.
 * The build writes the tags that point at these and the capture writes the
 * files, from this one function, so a route that gains a card cannot end up
 * with a tag naming a file nobody made.
 *
 * @param {string} routePath a canonical route path, as `normalizeRoutePath` returns
 * @returns {string} a file name with no extension
 */
export function socialCardName(routePath) {
  const parts = routePath.split("/").filter(Boolean);
  return parts.length === 0 ? "home" : parts.join("-");
}
