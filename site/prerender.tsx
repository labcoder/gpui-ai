import { StrictMode } from "react";
import { renderToString } from "react-dom/server";
import { App } from "./app/App";
import { routes } from "./app/routes";

export { routes };

/** Renders one route to the markup the browser will hydrate. */
export function render(path: string): string {
  const route = routes.find((candidate) => candidate.path === path);
  if (!route) throw new Error(`no route for ${path}`);
  return renderToString(
    <StrictMode>
      <App route={route} />
    </StrictMode>,
  );
}
