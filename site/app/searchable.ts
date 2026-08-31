// One searchable list over two kinds of thing.
//
// The rail and the catalogue search components. The decorations were not in
// that index at all, which is most of why nobody found them: they were a page
// you had to already know the name of. A decoration is not a component and
// never will be — one is something the library gives you, the other something
// you paint into it — so they are indexed as their own kind rather than
// smuggled into the catalogue.

import { components, decorations, type Component, type DecorationEntry } from "./data";
import { buildIndex, search } from "./search.mjs";

/** A component or a decoration, as the index sees it. */
export interface Found {
  readonly kind: "component" | "decoration";
  readonly slug: string;
  /** What the nav prints, and the card's link text. */
  readonly label: string;
  /** One line under the label on a card. */
  readonly summary: string;
  /** The type name, where there is one; empty for a decoration. */
  readonly api: string;
  /** The section it is listed under when results are grouped. */
  readonly category: string;
  /** Where it goes. */
  readonly path: string;
}

/**
 * The searchable shape.
 *
 * A decoration has no type name and no events, so those fields are empty
 * rather than invented: an empty field scores nothing, which is the truth.
 */
interface Indexed {
  readonly kind: "component" | "decoration";
  readonly slug: string;
  readonly label: string;
  readonly api: string;
  readonly title: string;
  readonly compactLabel: string;
  readonly events: readonly string[];
  readonly category: string;
  readonly summary: string;
  readonly usage: string;
  readonly behavior: Prose;
  readonly path: string;
}

type Prose = Readonly<Record<string, string>>;

function fromComponent(component: Component): Indexed {
  return {
    kind: "component",
    slug: component.slug,
    label: component.compactLabel,
    api: component.api,
    title: component.title,
    compactLabel: component.compactLabel,
    events: component.events,
    category: component.category,
    summary: component.summary,
    usage: component.usage,
    behavior: component.behavior as unknown as Prose,
    path: `/components/${component.slug}/`,
  };
}

function fromDecoration(decoration: DecorationEntry): Indexed {
  return {
    kind: "decoration",
    slug: decoration.slug,
    label: decoration.label,
    api: "",
    title: decoration.label,
    compactLabel: decoration.label,
    events: [],
    category: DECORATIONS,
    summary: decoration.note,
    usage: "",
    behavior: {},
    path: `/effects/${decoration.slug}/`,
  };
}

/** The heading decorations are listed under. */
export const DECORATIONS = "Decorations";

const RECORDS: readonly Indexed[] = [
  ...components.map(fromComponent),
  ...decorations.map(fromDecoration),
];

/**
 * Built once for the life of the page.
 *
 * The catalog is generated and never changes at run time, so rebuilding it per
 * keystroke would be lowercasing the same fifty records over and over.
 */
const INDEX = buildIndex(RECORDS);

function found(record: Indexed): Found {
  return {
    kind: record.kind,
    slug: record.slug,
    label: record.label,
    summary: record.summary,
    api: record.api,
    category: record.category,
    path: record.path,
  };
}

/** Everything, grouped the way the catalogue groups it, decorations last. */
export function everything(
  categories: readonly string[],
): readonly (readonly [string, readonly Found[]])[] {
  const groups = categories.map(
    (category) =>
      [category, RECORDS.filter((record) => record.category === category).map(found)] as const,
  );
  return [
    ...groups,
    [DECORATIONS, RECORDS.filter((record) => record.kind === "decoration").map(found)] as const,
  ];
}

/** What a query matches, best first. */
export function matches(query: string): readonly Found[] {
  return search(INDEX, query).map(found);
}
