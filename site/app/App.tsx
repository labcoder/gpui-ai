import "./site.css";
import { CatalogPage } from "./CatalogPage";
import { ComponentPage } from "./ComponentPage";
import { DecorationPage } from "./DecorationPage";
import { EffectsPage } from "./EffectsPage";
import { GuidePage, GuidesIndex } from "./GuidesPage";
import { HomePage } from "./HomePage";
import { MissingPage } from "./MissingPage";
import { Shell } from "./Shell";
import { ShowcasePage } from "./ShowcasePage";
import { StartPage } from "./StartPage";
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
    case "start":
      return <StartPage />;
    case "index":
      return <CatalogPage />;
    case "component":
      return <ComponentPage slug={route.slug ?? ""} />;
    case "effects":
      return <EffectsPage />;
    case "decoration":
      return <DecorationPage slug={route.slug ?? ""} />;
    case "showcase":
      return <ShowcasePage />;
    case "themes":
      return <ThemesPage />;
    case "guides":
      return <GuidesIndex />;
    case "guide":
      return <GuidePage slug={route.slug ?? ""} />;
    case "missing":
      return <MissingPage />;
  }
}
