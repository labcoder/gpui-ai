import { StrictMode } from "react";
import { hydrateRoot } from "react-dom/client";
import { App } from "./App";
import { routeFor, routes } from "./routes";

// The markup already exists: every route is pre-rendered at build time, so the
// browser hydrates rather than mounts. That is what lets GitHub Pages serve a
// deep link with no server behind it.
//
// routeFor applies the same rule the pre-render used, including the base path
// and an explicit index.html, so the client cannot pick a different route than
// the markup it is hydrating.
const route = routeFor(window.location.pathname, import.meta.env.BASE_URL) ?? routes[0];
const root = document.getElementById("root");
if (root && route) {
  hydrateRoot(
    root,
    <StrictMode>
      <App route={route} />
    </StrictMode>,
  );
}
