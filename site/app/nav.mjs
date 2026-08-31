// The site's destinations, in one list, used everywhere navigation appears.
//
// It exists because the site had two navigation surfaces that disagreed. The
// masthead carried four links and appeared only above 60rem; the drawer that
// replaced it below that width carried the component catalogue and nothing
// else. So on a phone, a tablet, or a desktop window at half width, the guides,
// the effects and the themes had no link anywhere on the site — the routes
// answered, and nothing pointed at them.
//
// One list, rendered by both, so a destination cannot be added to one and
// forgotten in the other. `site/test/pages.checks.mjs` asserts every route is
// reachable from it.
//
// Plain JavaScript, like `docs.mjs` and `route-path.mjs`, because the build
// scripts and the tests read it and cannot import TypeScript.

/**
 * @typedef {{
 *   path: string,
 *   label: string,
 *   blurb: string,
 *   covers?: string,
 * }} Destination
 */

/**
 * The five questions someone arrives with, in the order they arrive in.
 *
 * `covers` is the path prefix a destination owns, so a page can mark its own
 * entry current without every page knowing the whole map.
 */
/** @type {readonly Destination[]} */
export const destinations = [
  {
    path: "/start/",
    label: "Start",
    blurb: "Add the dependency and open a window with a component in it.",
    covers: "/start/",
  },
  {
    path: "/components/",
    label: "Components",
    blurb: "Thirty-seven surfaces an AI application keeps rebuilding, each one running.",
    covers: "/components/",
  },
  {
    path: "/effects/",
    label: "Effects",
    blurb: "Decorations an application paints into a component, and the motion channel.",
    covers: "/effects/",
  },
  {
    path: "/showcase/",
    label: "Showcase",
    blurb: "Whole compositions rather than parts: a conversation, a workspace, an instrument.",
    covers: "/showcase/",
  },
  {
    path: "/guides/",
    label: "Guides",
    blurb: "Theming, who owns what, accessibility, and what a browser demo cannot prove.",
    covers: "/guides/",
  },
  {
    path: "/themes/",
    label: "Themes",
    blurb: "Fifty-five presets, the tokens each one sets, and the JSON to take away.",
    covers: "/themes/",
  },
  {
    path: "/api/",
    label: "API",
    blurb: "The generated reference for every type this library exports.",
  },
];

/**
 * Which destination a path belongs to, or undefined at the front door.
 *
 * @param {string} path
 * @returns {Destination | undefined}
 */
export function destinationFor(path) {
  return destinations.find((destination) =>
    destination.covers ? path.startsWith(destination.covers) : path === destination.path,
  );
}
