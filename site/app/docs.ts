/**
 * The documentation pages, in reading order.
 *
 * One list, used by the routes, the index, the rail, and the previous/next
 * links, so a page cannot exist without being reachable or be linked without
 * existing. The order is the order someone new reads them in: install it, make
 * it look right, learn who owns what, then the two things that are true of
 * every component.
 */
export interface Doc {
  readonly slug: string;
  /** The page's heading, and the link text everywhere it is linked. */
  readonly title: string;
  /** One sentence, used for the page metadata and the index card. */
  readonly summary: string;
}

export const docs: readonly Doc[] = [
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

export function docBySlug(slug: string): Doc | undefined {
  return docs.find((doc) => doc.slug === slug);
}
