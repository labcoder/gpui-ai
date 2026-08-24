// Typed access to everything the site renders.
//
// All of it is generated — `npm run generate` writes site/generated from the
// Rust story registry and the themes/ directory. Nothing here is authored, and
// nothing here may be edited by hand.

import catalogJson from "../generated/catalog.json";
import snippetsJson from "../generated/snippets.json";
import themesJson from "../generated/themes.json";

/** How much room a story's demo frame needs. */
export type Viewport = "wide" | "tall";

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
  readonly viewport: Viewport;
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

export function componentBySlug(slug: string): Component | undefined {
  return components.find((component) => component.slug === slug);
}
