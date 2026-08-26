import { StrictMode } from "react";
import { renderToString } from "react-dom/server";
import { App } from "./app/App";
import { preloadCodeFor } from "./app/code";
import { missingRoute, routes } from "./app/routes";
import type { Route } from "./app/routes";

export { routes };

/**
 * Renders one route to the markup the browser will hydrate.
 *
 * Async only to await the route's code chunk: the render itself stays
 * `renderToString`, and the awaited data is the same chunk the browser loads
 * before hydrating, which is what keeps the two trees identical.
 */
export async function render(path: string): Promise<string> {
  const route = routes.find((candidate) => candidate.path === path);
  if (!route) throw new Error(`no route for ${path}`);
  await preloadCodeFor(route);
  return draw(route);
}

/**
 * The 404 page's markup.
 *
 * Separate from `render` because this route is deliberately not in `routes` —
 * it is one file rather than a directory, and nothing may link to it or list
 * it — but it is the same component tree, so it carries the same chrome as
 * every other page.
 */
export function renderNotFound(): string {
  return draw(missingRoute);
}

function draw(route: Route): string {
  return renderToString(
    <StrictMode>
      <App route={route} />
    </StrictMode>,
  );
}
