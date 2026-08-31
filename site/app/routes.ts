import { components, decorations } from "./data";
import { docs } from "./docs.mjs";
import { normalizeRoutePath } from "./route-path.mjs";

/** One page the site emits as real HTML. */
export interface Route {
  /** Path from the site root, always with a trailing slash. */
  readonly path: string;
  /** Browser tab title. */
  readonly title: string;
  /** What the page is, for metadata. */
  readonly description: string;
  /** Which page component renders it. */
  readonly kind:
    | "home"
    | "start"
    | "index"
    | "component"
    | "effects"
    | "decoration"
    | "showcase"
    | "themes"
    | "guides"
    | "guide"
    | "missing";
  /** Present for `kind: "component"`, `"decoration"`, and `"guide"`. */
  readonly slug?: string;
}

/**
 * Every route, in the order the site emits them.
 *
 * The list is derived from the generated catalog rather than written out, so a
 * component added in Rust becomes a page without anyone remembering to add it
 * here. S-02 pre-renders each of these to `<path>index.html`; without that a
 * deep link 404s on GitHub Pages, which has no server to rewrite it.
 */
export const routes: readonly Route[] = [
  {
    path: "/",
    title: "gpui-ai",
    description: "Components for AI applications built with GPUI.",
    kind: "home",
  },
  {
    path: "/start/",
    title: "Start · gpui-ai",
    description:
      "What you need, how to add the dependency, and a complete window with a component in it.",
    kind: "start",
  },
  {
    path: "/components/",
    title: "Components · gpui-ai",
    description: `All ${components.length} gpui-ai components, grouped by what they are for.`,
    kind: "index",
  },
  {
    path: "/effects/",
    title: "Effects · gpui-ai",
    description:
      "What an application paints into a component rather than around it: a decoration slot on every framed component, and a motion channel to drive it.",
    kind: "effects",
  },
  ...decorations.map(
    (decoration): Route => ({
      path: `/effects/${decoration.slug}/`,
      title: `${decoration.label} · Effects · gpui-ai`,
      description: decoration.note,
      kind: "decoration",
      slug: decoration.slug,
    }),
  ),
  {
    path: "/showcase/",
    title: "Showcase · gpui-ai",
    description:
      "Whole compositions rather than parts: a conversation, a docked workspace, and an instrument for the motion policy.",
    kind: "showcase",
  },
  {
    path: "/themes/",
    title: "Themes · gpui-ai",
    description: "Every bundled theme, and the tokens each one sets.",
    kind: "themes",
  },
  {
    path: "/guides/",
    title: "Guides · gpui-ai",
    description:
      "Theming gpui-ai, who owns what between it and your application, and what a browser demo cannot prove.",
    kind: "guides",
  },
  ...docs.map(
    (doc): Route => ({
      path: `/guides/${doc.slug}/`,
      title: `${doc.title} · gpui-ai`,
      description: doc.summary,
      kind: "guide",
      slug: doc.slug,
    }),
  ),
  ...components.map(
    (component): Route => ({
      path: `/components/${component.slug}/`,
      title: `${component.title} · gpui-ai`,
      description: component.summary,
      kind: "component",
      slug: component.slug,
    }),
  ),
];

/**
 * The page a URL that names nothing gets.
 *
 * Not in `routes`: it is emitted as `404.html` rather than as a directory,
 * because that is the one file GitHub Pages serves for a path it cannot find,
 * and it is deliberately absent from the sitemap. It exists as a route anyway
 * so that the pre-render and the browser agree about what to draw — the client
 * used to fall back to the home route for an unknown path, which would have
 * hydrated the 404 into a copy of the front page.
 */
export const missingRoute: Route = {
  path: "/404/",
  title: "Page not found · gpui-ai",
  description: "That page is not here.",
  kind: "missing",
};

export function routeFor(path: string, base = "/"): Route | undefined {
  const normalized = normalizeRoutePath(path, base);
  return routes.find((route) => route.path === normalized);
}
