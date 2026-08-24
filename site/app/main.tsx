import { StrictMode } from "react";
import { hydrateRoot } from "react-dom/client";
import { App } from "./App";
import { routeFor, routes } from "./routes";

// The markup already exists: every route is pre-rendered at build time, so the
// browser hydrates rather than mounts. That is what lets GitHub Pages serve a
// deep link with no server behind it.
//
// The base path is stripped before matching so the same build works from the
// dev server root and from /gpui-ai/ on Pages.
const base = import.meta.env.BASE_URL.replace(/\/$/, "");
const path = window.location.pathname.startsWith(base)
  ? window.location.pathname.slice(base.length) || "/"
  : window.location.pathname;

const route = routeFor(path) ?? routes[0];
const root = document.getElementById("root");
if (root && route) {
  hydrateRoot(
    root,
    <StrictMode>
      <App route={route} />
    </StrictMode>,
  );
}
