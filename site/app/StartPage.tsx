import { StartBody } from "./GuidesPage";
import { href } from "./links";

/**
 * How to get gpui-ai, as its own destination.
 *
 * It used to be the first of five pages inside a section called Docs, which
 * put the one thing every visitor needs behind a word that means four other
 * things. The prose is unchanged and still lives beside the guides it reads
 * like; what changed is that the site now has a door marked Start.
 */
export function StartPage() {
  return (
    <article className="doc">
      <p className="eyebrow">Start</p>
      <h1>Getting started</h1>
      <p className="lede">
        What you need, how to add the dependency, and a complete window with a component in it.
      </p>

      <StartBody />

      <nav className="doc-neighbours" aria-label="Where to go next">
        <span />
        <a href={href("/components/")} rel="next">
          <span>Next</span>
          <strong>Browse the components</strong>
        </a>
      </nav>
    </article>
  );
}
