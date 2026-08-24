import { useMemo, useState } from "react";
import { components, componentsByCategory, type Component } from "./data";
import { href } from "./links";

/**
 * Every component, grouped by what it is for.
 *
 * The pre-render emits all of them, so the page is complete and indexable
 * before any JavaScript runs; the filter narrows what is already there. S-12
 * replaces the substring match with a real index over events and prose, and
 * adds the `/` shortcut.
 */
export function CatalogPage() {
  const [query, setQuery] = useState("");
  const needle = query.trim().toLowerCase();

  const groups = useMemo(() => {
    const matches = (component: Component) =>
      !needle ||
      `${component.title} ${component.category} ${component.summary} ${component.api}`
        .toLowerCase()
        .includes(needle);
    return componentsByCategory()
      .map(([category, entries]) => [category, entries.filter(matches)] as const)
      .filter(([, entries]) => entries.length > 0);
  }, [needle]);

  const shown = groups.reduce((total, [, entries]) => total + entries.length, 0);

  return (
    <div className="shell">
      <h1>Components</h1>
      <p className="lede">{`${components.length} components, each with a live demo running the real Rust.`}</p>

      <div className="filter">
        <label htmlFor="component-filter">Filter components</label>
        <input
          id="component-filter"
          type="search"
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
            <h2>{category}</h2>
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
