import { useMemo, useState } from "react";
import { categories, components, decorations } from "./data";
import { href } from "./links";
import { everything, matches, type Found } from "./searchable";

/**
 * Everything the library can put on screen, grouped by what it is for.
 *
 * The pre-render emits all of it, so the page is complete and indexable
 * before any JavaScript runs; the search narrows what is already there.
 *
 * Grouped by category while nobody is searching, because that is how a reader
 * who does not yet know what they want finds it. The moment there is a query
 * the groups go and one ranked list takes their place: with results spread
 * over six headings, the best answer is wherever its category happens to fall,
 * which makes the ranking invisible and the page a filter rather than a
 * search.
 *
 * Decorations are on this page and in this index, after the components rather
 * than among them. They are not components — a component is something the
 * library gives you, a decoration is something you paint into one — but a
 * reader looking for "that photo thing" is looking here, and until now the
 * search answered that there was no such thing.
 */
export function CatalogPage() {
  const [query, setQuery] = useState("");
  const { grouped, groups } = useMemo(() => narrow(query), [query]);

  const total = components.length + decorations.length;
  const shown = groups.reduce((count, [, entries]) => count + entries.length, 0);

  return (
    <div className="shell">
      <h1>Components</h1>
      <p className="lede">
        {`${components.length} components, each with a live demo running the real Rust — and the ${decorations.length} `}
        <a href={href("/effects/")}>decorations</a>
        {" you can paint into them."}
      </p>

      <div className="filter">
        <label htmlFor="component-filter">Search components</label>
        <input
          id="component-filter"
          type="search"
          data-site-search=""
          value={query}
          placeholder="chat, table, approval, halftone…"
          onChange={(event) => setQuery(event.target.value)}
        />
        <output htmlFor="component-filter" aria-live="polite">{`${shown} of ${total}`}</output>
      </div>

      {groups.length === 0 ? (
        <p className="empty">{`Nothing matches “${query}”.`}</p>
      ) : (
        groups.map(([category, entries]) => (
          <section className="category" key={category} aria-label={category}>
            {grouped ? <h2>{category}</h2> : null}
            <ul className="cards">
              {entries.map((entry) => (
                <li className="card" key={entry.path} data-component={entry.slug}>
                  <a href={href(entry.path)}>{entry.label}</a>
                  <p>{entry.summary}</p>
                  <p className="card-api">
                    {entry.api ? <code>{entry.api}</code> : <span>{entry.category}</span>}
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
  readonly groups: readonly (readonly [string, readonly Found[]])[];
}

/**
 * What to show for a query.
 *
 * Shared with the rail, which draws the same answer in a narrower column. Both
 * search the one index — a page whose rail and body disagreed about what a
 * word matched would be worse than either of them alone.
 */
export function narrow(query: string): Narrowed {
  if (!query.trim()) return { grouped: true, groups: everything(categories) };
  const ranked = matches(query);
  // One section, so it lays out the same way a group does, with a label a
  // screen reader can announce and no heading nobody asked for.
  return { grouped: false, groups: ranked.length > 0 ? [["Search results", ranked]] : [] };
}
