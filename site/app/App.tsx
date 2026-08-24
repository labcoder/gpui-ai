import { componentBySlug, components, categories, hero, themeGroups, themes } from "./data";
import type { Route } from "./routes";

/**
 * The whole site, one route at a time.
 *
 * Deliberately plain: S-08 replaces these bodies with the real pages and S-03
 * paints them from the generated tokens. What matters now is that every route
 * renders identical markup on the server and in the browser, because that is
 * what hydration compares.
 */
export function App({ route }: { readonly route: Route }) {
  return (
    <main>
      <h1>{route.title.replace(" · gpui-ai", "")}</h1>
      <p>{route.description}</p>
      {route.kind === "home" ? <Home /> : null}
      {route.kind === "index" ? <Index /> : null}
      {route.kind === "themes" ? <Themes /> : null}
      {route.kind === "component" ? <ComponentPage slug={route.slug ?? ""} /> : null}
    </main>
  );
}

function Home() {
  return (
    <p>
      {components.length} components across {categories.length} categories, {themes.length} themes.
      The hero story is <code>{hero.slug}</code>.
    </p>
  );
}

function Index() {
  return (
    <ul>
      {components.map((component) => (
        <li key={component.slug}>
          <a href={`/gpui-ai/components/${component.slug}/`}>{component.title}</a> — {component.category}
        </li>
      ))}
    </ul>
  );
}

function Themes() {
  return (
    <ul>
      {themeGroups.map((group) => (
        <li key={group.id}>
          {group.label}: {group.themes.length} themes
          {group.license ? ` (${group.license})` : ""}
        </li>
      ))}
    </ul>
  );
}

function ComponentPage({ slug }: { readonly slug: string }) {
  const component = componentBySlug(slug);
  if (!component) return <p>Unknown component.</p>;
  return (
    <dl>
      <dt>API</dt>
      <dd>
        <code>{component.api}</code>
      </dd>
      <dt>Source</dt>
      <dd>
        <code>{component.source}</code>
      </dd>
      <dt>Story</dt>
      <dd>
        <code>story={component.slug}</code>
      </dd>
    </dl>
  );
}
