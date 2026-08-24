import "./site.css";
import { CatalogPage } from "./CatalogPage";
import { ComponentPage } from "./ComponentPage";
import { HomePage } from "./HomePage";
import { ThemesPage } from "./ThemesPage";
import { build } from "./data";
import { href } from "./links";
import type { Route } from "./routes";

/**
 * The whole site, one route at a time.
 *
 * The header and footer here are the minimum a page needs to be navigable.
 * S-04 replaces them with the real shell — category rail, search, theme and
 * mode controls, mobile drawer — and S-03 brings the fonts.
 */
export function App({ route }: { readonly route: Route }) {
  return (
    <>
      <a className="skip-link" href="#content">
        Skip to content
      </a>
      <header className="site-header">
        <div className="shell">
          <a className="wordmark" href={href("/")}>
            gpui-ai
          </a>
          <nav className="site-nav" aria-label="Site">
            <a href={href("/components/")}>Components</a>
            <a href={href("/themes/")}>Themes</a>
            <a href={href("/api/")}>API</a>
            <a href={build.repository}>GitHub</a>
          </nav>
        </div>
      </header>

      <main id="content">
        {route.kind === "home" ? <HomePage /> : null}
        {route.kind === "index" ? <CatalogPage /> : null}
        {route.kind === "themes" ? <ThemesPage /> : null}
        {route.kind === "component" ? <ComponentPage slug={route.slug ?? ""} /> : null}
      </main>

      <footer className="site-footer">
        <div className="shell">
          <span>{`gpui-ai v${build.version} · ${build.license}`}</span>
          <span>
            Built from <a href={build.repository}>labcoder/gpui-ai</a>
          </span>
        </div>
      </footer>
    </>
  );
}
