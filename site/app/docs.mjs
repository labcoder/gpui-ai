// The documentation pages, in reading order.
//
// One list, used by the routes, the index, the previous/next links, and the
// social-card capture — so a page cannot exist without being reachable, be
// linked without existing, or claim a card nobody renders. The order is the
// order someone new reads them in: install it, make it look right, learn who
// owns what, then the two things that are true of every component.
//
// Plain JavaScript, like `route-path.mjs` and `theme-resolve.mjs`, because
// `site/scripts/capture-og.mjs` reads it and cannot import TypeScript.

/** @typedef {{ slug: string, title: string, summary: string }} Doc */

/** @type {readonly Doc[]} */
export const docs = [
  {
    slug: "getting-started",
    title: "Getting started",
    summary: "What you need, how to add the dependency, and a window with a component in it.",
  },
  {
    slug: "theming",
    title: "Theming",
    summary: "Every colour, radius, and type style comes from the active theme, including yours.",
  },
  {
    slug: "ownership-and-events",
    title: "Ownership and events",
    summary: "Your application owns the state and the clock; components render what you give them.",
  },
  {
    slug: "accessibility-and-motion",
    title: "Accessibility and motion",
    summary: "Reduced motion, keyboard reach, and contrast, and what each of them guarantees.",
  },
  {
    slug: "browser-demos",
    title: "Browser demo limits",
    summary: "What the demos on this site can and cannot do, and why the native runtime decides.",
  },
];

/** @param {string} slug */
export function docBySlug(slug) {
  return docs.find((doc) => doc.slug === slug);
}
