import { StrictMode } from "react";
import { hydrateRoot } from "react-dom/client";
import { App } from "./App";
import { missingRoute, routeFor } from "./routes";

// The markup already exists: every route is pre-rendered at build time, so the
// browser hydrates rather than mounts. That is what lets GitHub Pages serve a
// deep link with no server behind it.
//
// routeFor applies the same rule the pre-render used, including the base path
// and an explicit index.html, so the client cannot pick a different route than
// the markup it is hydrating.
// `404.html` is served with the address the visitor typed still in the bar, so
// the same rule that finds a page finds the absence of one. Falling back to
// the first route instead would have hydrated every 404 into the home page.
const route = routeFor(window.location.pathname, import.meta.env.BASE_URL) ?? missingRoute;
const root = document.getElementById("root");
if (root && route) {
  hydrateRoot(
    root,
    <StrictMode>
      <App route={route} />
    </StrictMode>,
  );
}
