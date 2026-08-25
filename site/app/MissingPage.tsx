import { components } from "./data";
import { href } from "./links";

/**
 * The page a URL that names nothing gets.
 *
 * GitHub Pages serves `404.html` for any path it cannot find, and without one
 * a visitor gets GitHub's own page: no masthead, no rail, no way back. This is
 * the site's own chrome around a short explanation and the three places worth
 * going instead.
 *
 * It says what happened and then gets out of the way. A 404 that apologises at
 * length, or offers a search box the visitor did not ask for, is a page about
 * itself; the useful thing is the way out.
 */
export function MissingPage() {
  return (
    <div className="shell">
      <h1>Page not found</h1>
      <p className="lede">
        That address does not name a page on this site. It may have been a component that has since
        been renamed, or a link that was cut short.
      </p>

      <nav className="missing-ways" aria-label="Where to go instead">
        <a href={href("/components/")}>
          <strong>All {components.length} components</strong>
          <span>Grouped by what they are for, each with a running demo.</span>
        </a>
        <a href={href("/themes/")}>
          <strong>Themes</strong>
          <span>Every bundled theme, and the tokens each one sets.</span>
        </a>
        <a href={href("/")}>
          <strong>Home</strong>
          <span>What the library is, and how to install it.</span>
        </a>
      </nav>
    </div>
  );
}
