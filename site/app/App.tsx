import "./site.css";
import { CatalogPage } from "./CatalogPage";
import { ComponentPage } from "./ComponentPage";
import { HomePage } from "./HomePage";
import { Shell } from "./Shell";
import { ThemesPage } from "./ThemesPage";
import type { Route } from "./routes";

/** The whole site, one route at a time. */
export function App({ route }: { readonly route: Route }) {
  return (
    <Shell route={route}>
      <Page route={route} />
    </Shell>
  );
}

function Page({ route }: { readonly route: Route }) {
  switch (route.kind) {
    case "home":
      return <HomePage />;
    case "index":
      return <CatalogPage />;
    case "themes":
      return <ThemesPage />;
    case "component":
      return <ComponentPage slug={route.slug ?? ""} />;
  }
}
