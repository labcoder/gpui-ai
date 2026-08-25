// Tidying the rustdoc tree before it is published.
//
// Imported by script/build-docs.mjs, and kept separate from it so it can be
// tested without running `cargo doc`.

/**
 * The "Read more" links rustdoc could not point anywhere.
 *
 * `cargo doc --no-deps` documents this crate and nothing else, which is what
 * keeps the published tree to 50 MB instead of most of the crates.io graph.
 * The cost is that a trait implemented from a dependency has no page here, so
 * when rustdoc appends its "Read more" to the inherited summary it has no
 * target to give it and emits `<a>Read more</a>` — an anchor with no `href`,
 * which is not a link at all. It looks exactly like the ones beside it and
 * does nothing when clicked.
 *
 * There are tens of thousands of them, almost all from GPUI's Tailwind-style
 * `Styled` methods, so every page of a type that styles itself is dotted with
 * them. Removing them leaves the sentence that was already there — "Sets the
 * width of the element. Docs" — with its one working link intact.
 *
 * Anchors that do carry an `href` are untouched: those resolve, mostly to
 * doc.rust-lang.org, and they are the reason this matches the exact inert
 * string rather than anything looser.
 *
 * @param {string} html
 * @returns {{ html: string, removed: number }}
 */
export function stripDeadReadMore(html) {
  const DEAD = "<a>Read more</a>";
  let removed = 0;
  let out = "";
  let from = 0;

  for (;;) {
    const at = html.indexOf(DEAD, from);
    if (at < 0) break;
    // Take the space that separated it from the sentence too, or the summary
    // ends with a stray gap before the closing tag.
    const start = html[at - 1] === " " ? at - 1 : at;
    out += html.slice(from, start);
    from = at + DEAD.length;
    removed += 1;
  }

  return removed === 0 ? { html, removed } : { html: out + html.slice(from), removed };
}
