// Typed access to everything the site renders.
//
// All of it is generated — `npm run generate` writes site/generated from the
// Rust story registry and the themes/ directory. Nothing here is authored, and
// nothing here may be edited by hand.

import buildJson from "../generated/build.json";
import catalogJson from "../generated/catalog.json";
import highlightJson from "../generated/highlight.json";
import snippetsJson from "../generated/snippets.json";
import themesJson from "../generated/themes.json";

/** One state a story's switcher offers. */
export interface Variant {
  readonly id: string;
  readonly label: string;
}

/** Prose the component page renders beside the demo. */
export interface Behavior {
  readonly ownership: string;
  readonly interaction: string;
  readonly semantics: string;
  readonly overflow: string;
}

/** One component, as the exporter describes it. */
export interface Component {
  readonly sequence: number;
  readonly slug: string;
  readonly title: string;
  readonly compactLabel: string;
  readonly category: string;
  readonly summary: string;
  readonly source: string;
  readonly api: string;
  readonly usage: string;
  /**
   * The story's measured natural height in pixels.
   *
   * The demo frame is sized from this, so it fits the story instead of
   * padding a three-chip row out to the height of a data table.
   */
  readonly height: number;
  readonly windowTitle: string;
  readonly variants: readonly Variant[];
  readonly events: readonly string[];
  readonly event: string | null;
  readonly limitation: string;
  readonly behavior: Behavior;
}

/** The site-only hero story, which is deliberately not a component. */
export interface Hero {
  readonly slug: string;
  readonly title: string;
  readonly windowTitle: string;
  readonly siteOnly: true;
}

/** One upstream repository this release is pinned against. */
export interface UpstreamPin {
  readonly id: string;
  readonly label: string;
  readonly repository: string;
  readonly commit: string;
  readonly note: string;
}

/** What this build is: its version, its source, and what it is pinned to. */
export interface BuildInfo {
  readonly version: string;
  readonly repository: string;
  readonly homepage: string;
  readonly license: string;
  readonly upstream: readonly UpstreamPin[];
}

/** A theme preset and the `--ai-*` values the chrome is painted from. */
export interface Theme {
  readonly slug: string;
  readonly label: string;
  readonly registryName: string;
  readonly group: string;
  readonly mode: "light" | "dark";
  readonly radius: number;
  readonly radiusLg: number;
  readonly fontSize: number;
  readonly shadow: boolean;
  readonly tokens: Readonly<Record<string, string>>;
}

/** Themes grouped for the picker; the upstream pack is credited separately. */
export interface ThemeGroup {
  readonly id: string;
  readonly label: string;
  readonly license?: string;
  readonly source?: string;
  readonly themes: readonly Theme[];
}

export const build = buildJson as BuildInfo;
export const components = catalogJson.components as readonly Component[];
export const categories = catalogJson.categories as readonly string[];
export const hero = catalogJson.hero as Hero;
export const themeGroups = themesJson.groups as readonly ThemeGroup[];

/** Every theme across both groups, in picker order. */
export const themes: readonly Theme[] = themeGroups.flatMap((group) => group.themes);

const snippetsBySlug = snippetsJson.snippets as Readonly<
  Record<string, Readonly<Record<string, string>>>
>;

/** The copyable Rust for one story variant, cut from the gallery's source. */
export function snippet(slug: string, variant = "default"): string | undefined {
  return snippetsBySlug[slug]?.[variant];
}

/**
 * One piece of a highlighted line: its text, and what kind of thing it is.
 *
 * The second element is absent for ordinary text. The kinds are named rather
 * than coloured so the stylesheet can paint them from the active theme.
 */
export type CodeToken = readonly [text: string, category?: string];

// Widened through `unknown` on purpose: TypeScript reads the generated JSON as
// `string[][][]`, which cannot be narrowed to a tuple that requires its first
// element. The generator emits the tuple shape and refuses to write a file
// whose tokens do not reassemble into the snippet, so the shape is checked —
// just not by the compiler.
const highlightedBySlug = highlightJson.snippets as unknown as Readonly<
  Record<string, Readonly<Record<string, readonly (readonly CodeToken[])[]>>>
>;

/**
 * The same snippet, split into tokens.
 *
 * Highlighted at build time by `site/scripts/generate-highlight.mjs`, which
 * checks that reassembling the tokens gives back the snippet exactly. Copy
 * still reads `snippet()`, so what a visitor pastes never comes through here.
 */
export function highlighted(
  slug: string,
  variant = "default",
): readonly (readonly CodeToken[])[] | undefined {
  return highlightedBySlug[slug]?.[variant];
}

export function componentBySlug(slug: string): Component | undefined {
  return components.find((component) => component.slug === slug);
}

/** The component before this one in catalog order, for prev/next links. */
export function previousComponent(slug: string): Component | undefined {
  const index = components.findIndex((component) => component.slug === slug);
  return index > 0 ? components[index - 1] : undefined;
}

/** The component after this one in catalog order, for prev/next links. */
export function nextComponent(slug: string): Component | undefined {
  const index = components.findIndex((component) => component.slug === slug);
  return index >= 0 && index < components.length - 1 ? components[index + 1] : undefined;
}

/** Components grouped by category, in the catalog's own order. */
export function componentsByCategory(): readonly (readonly [string, readonly Component[]])[] {
  return categories.map(
    (category) =>
      [category, components.filter((component) => component.category === category)] as const,
  );
}
