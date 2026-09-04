// Typed access to everything the site renders.
//
// All of it is generated — `npm run generate` writes site/generated from the
// Rust story registry and the themes/ directory. Nothing here is authored, and
// nothing here may be edited by hand.

import buildJson from "../generated/build.json";
import catalogJson from "../generated/catalog.json";
import installJson from "../generated/install.json";
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

/**
 * Where a component stands relative to gpui-component.
 *
 * On every component page, and gathered on `/guides/differences/`: a reader
 * choosing between this library and the one it is built on should not have to
 * read the source to find out which of the two they are looking at.
 */
export interface LineageEntry {
  /** `new`, `extends`, or `composes` — what the page styles and filters on. */
  readonly kind: string;
  /** The one-word label a reader sees. */
  readonly label: string;
  /** The upstream component it is built on, empty where there is none. */
  readonly basis: string;
  /** What it adds, and why it is here rather than upstream. */
  readonly note: string;
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
  readonly lineage: LineageEntry;
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

/** One decoration an application can paint into a component's frame. */
export interface DecorationEntry {
  readonly slug: string;
  readonly label: string;
  /** One line on what it is and how it is made. */
  readonly note: string;
}

/**
 * The Effects section, as the exporter describes it.
 *
 * Decorations are not components and are not in the catalog: a component is
 * something the library gives you, and a decoration is something you paint
 * into one. They share a single gallery story, addressed by state, which is
 * why one story and one height serve fifteen pages.
 */
export interface Effects {
  /** The gallery story every decoration page runs. */
  readonly story: string;
  readonly height: number;
  readonly windowTitle: string;
  /** Where the decorations are implemented, for the source link. */
  readonly source: string;
  readonly decorations: readonly DecorationEntry[];
}

/** The site-only hero story, which is deliberately not a component. */
export interface Hero {
  readonly slug: string;
  readonly title: string;
  readonly windowTitle: string;
  /**
   * The hero's measured height at its settled state.
   *
   * Optional because the hero is not in `StoryId::ALL` and the exporter has
   * carried it without a height in the past; a home page that renders no hero
   * is better than one that guesses at the size of it.
   */
  readonly height?: number;
  readonly siteOnly: true;
}

/** One upstream crate this release is built against. */
export interface UpstreamPin {
  readonly id: string;
  readonly label: string;
  /** The name on crates.io, which is not always the name in `use` paths. */
  readonly crate: string;
  readonly repository: string;
  readonly version: string;
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
export const effects = catalogJson.effects as unknown as Effects;
export const decorations = effects.decorations;

export function decorationBySlug(slug: string): DecorationEntry | undefined {
  return decorations.find((decoration) => decoration.slug === slug);
}

/** The decoration before this one, for prev/next links. */
export function previousDecoration(slug: string): DecorationEntry | undefined {
  const index = decorations.findIndex((decoration) => decoration.slug === slug);
  return index > 0 ? decorations[index - 1] : undefined;
}

/** The decoration after this one, for prev/next links. */
export function nextDecoration(slug: string): DecorationEntry | undefined {
  const index = decorations.findIndex((decoration) => decoration.slug === slug);
  return index >= 0 && index < decorations.length - 1 ? decorations[index + 1] : undefined;
}

/**
 * The file every snippet is cut from.
 *
 * Not a component's own `source`: that is where its type is implemented, which
 * is a different file. The code on a component page comes from the gallery
 * story that page runs, and the strip above it says so.
 */
export const snippetSource = catalogJson.snippetSource as string;
export const themeGroups = themesJson.groups as readonly ThemeGroup[];

/** Every theme across both groups, in picker order. */
export const themes: readonly Theme[] = themeGroups.flatMap((group) => group.themes);

/**
 * One piece of a highlighted line: its text, and what kind of thing it is.
 *
 * The second element is absent for ordinary text. The kinds are named rather
 * than coloured so the stylesheet can paint them from the active theme.
 */
export type CodeToken = readonly [text: string, category?: string];

/** Code the site shows that was not cut from a story. */
export interface CodeSample {
  readonly lang: string;
  readonly code: string;
  readonly lines: readonly (readonly CodeToken[])[];
}

/**
 * The dependency lines the home page shows.
 *
 * Composed by the generate step from `build.json`, so the version and both
 * repository URLs come from the manifests rather than from a string in a
 * component, and the text the page prints is the text that was highlighted.
 *
 * The one code payload that stays eagerly bundled: the home page paints it in
 * its first screen. Every other snippet loads per route through
 * `site/app/code.ts` — the corpus of 34 stories' code has no business in the
 * bundle a themes visitor downloads.
 */
export const install = installJson as unknown as CodeSample;

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
