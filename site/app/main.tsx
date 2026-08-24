import { StrictMode } from "react";
import { hydrateRoot } from "react-dom/client";
import { App } from "./App";

// The markup already exists: S-02 pre-renders every route at build time, so
// the browser hydrates rather than mounts. That is what lets GitHub Pages
// serve a deep link with no server behind it.
const root = document.getElementById("root");
if (root) {
  hydrateRoot(
    root,
    <StrictMode>
      <App />
    </StrictMode>,
  );
}
