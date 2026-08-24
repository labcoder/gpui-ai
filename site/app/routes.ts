import { components } from "./data";

/** One page the site emits as real HTML. */
export interface Route {
  /** Path from the site root, always with a trailing slash. */
  readonly path: string;
  /** Browser tab title. */
  readonly title: string;
  /** What the page is, for metadata. */
  readonly description: string;
  /** Which page component renders it. */
  readonly kind: "home" | "index" | "component" | "themes";
  /** Present for `kind: "component"`. */
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
    description: "AI-native UI components for GPUI, the Rust UI framework behind Zed.",
    kind: "home",
  },
  {
    path: "/components/",
    title: "Components · gpui-ai",
    description: `All ${components.length} gpui-ai components, grouped by what they are for.`,
    kind: "index",
  },
  {
    path: "/themes/",
    title: "Themes · gpui-ai",
    description: "Every bundled theme, and the tokens each one sets.",
    kind: "themes",
  },
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

export function routeFor(path: string): Route | undefined {
  const normalized = path.endsWith("/") ? path : `${path}/`;
  return routes.find((route) => route.path === normalized);
}
