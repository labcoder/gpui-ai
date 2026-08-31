// Paths that used to be pages, and where they went.
//
// Deliberately not routes: a redirect is not a page, carries no chrome, must
// not appear in the sitemap, and must not be pre-rendered as React. The build
// writes one small HTML file per entry — a canonical link for anything reading
// the site, and a refresh for anyone who followed an old link.
//
// They exist because a URL someone bookmarked or linked from elsewhere is not
// ours to break, and `/extensions/` in particular was the only way anyone
// reached the decorations at all.

/** @typedef {{ from: string, to: string }} Redirect */

/** @type {readonly Redirect[]} */
export const redirects = [
  // Extensions became Effects, and its one page became fifteen.
  { from: "/extensions/", to: "/effects/" },
  // Docs became Guides, so that "docs" is not two things: the prose here and
  // the generated API reference next to it.
  { from: "/docs/", to: "/guides/" },
  { from: "/docs/getting-started/", to: "/start/" },
  { from: "/docs/theming/", to: "/guides/theming/" },
  { from: "/docs/ownership-and-events/", to: "/guides/ownership-and-events/" },
  { from: "/docs/accessibility-and-motion/", to: "/guides/accessibility-and-motion/" },
  { from: "/docs/browser-demos/", to: "/guides/browser-demos/" },
];
