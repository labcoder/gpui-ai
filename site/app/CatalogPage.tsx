import { useMemo, useState } from "react";
import { components, componentsByCategory, type Component } from "./data";
import { buildIndex, search } from "./search.mjs";
import { href } from "./links";

/**
 * Every component, grouped by what it is for.
 *
 * The pre-render emits all of them, so the page is complete and indexable
 * before any JavaScript runs; the search narrows what is already there.
 *
 * Grouped by category while nobody is searching, because that is how a reader
 * who does not yet know what they want finds it. The moment there is a query
 * the groups go and one ranked list takes their place: with results spread
 * over six headings, the best answer is wherever its category happens to fall,
 * which makes the ranking invisible and the page a filter rather than a
 * search.
 */
export function CatalogPage() {
  const [query, setQuery] = useState("");
  const { grouped, groups } = useMemo(() => narrow(query), [query]);

  const shown = groups.reduce((total, [, entries]) => total + entries.length, 0);

  return (
    <div className="shell">
      <h1>Components</h1>
      <p className="lede">{`${components.length} components, each with a live demo running the real Rust.`}</p>

      <div className="filter">
        <label htmlFor="component-filter">Search components</label>
        <input
          id="component-filter"
          type="search"
          data-site-search=""
          value={query}
          placeholder="chat, table, approval…"
          onChange={(event) => setQuery(event.target.value)}
        />
        <output htmlFor="component-filter" aria-live="polite">{`${shown} of ${components.length}`}</output>
      </div>

      {groups.length === 0 ? (
        <p className="empty">{`Nothing matches “${query}”.`}</p>
      ) : (
        groups.map(([category, entries]) => (
          <section className="category" key={category} aria-label={category}>
            {grouped ? <h2>{category}</h2> : null}
            <ul className="cards">
              {entries.map((component) => (
                <li className="card" key={component.slug} data-component={component.slug}>
                  <a href={href(`/components/${component.slug}/`)}>{component.title}</a>
                  <p>{component.summary}</p>
                  <p className="card-api">
                    <code>{component.api}</code>
                  </p>
                </li>
              ))}
            </ul>
          </section>
        ))
      )}
    </div>
  );
}

/** What to draw for a query: either the catalog's groups, or one ranked list. */
export interface Narrowed {
  /** False once there is a query, when ranking replaces browsing. */
  readonly grouped: boolean;
  readonly groups: readonly (readonly [string, readonly Component[]])[];
}

/**
 * The components to show for a query.
 *
 * Shared with the rail, which draws the same answer in a narrower column. Both
 * search the one index — a page whose rail and body disagreed about what a
 * word matched would be worse than either of them alone.
 */
export function narrow(query: string): Narrowed {
  if (!query.trim()) return { grouped: true, groups: componentsByCategory() };
  const ranked = search(INDEX, query);
  // One section, so it lays out the same way a group does, with a label a
  // screen reader can announce and no heading nobody asked for.
  return { grouped: false, groups: ranked.length > 0 ? [["Search results", ranked]] : [] };
}

/**
 * Built once for the life of the page.
 *
 * The catalog is generated and never changes at run time, so rebuilding it per
 * keystroke would be lowercasing the same 34 records over and over.
 */
const INDEX = buildIndex(components);
