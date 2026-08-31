import { CodePanel } from "./CodePanel";
import { Demo } from "./Demo";
import {
  build,
  decorationBySlug,
  effects,
  nextDecoration,
  previousDecoration,
} from "./data";
import { demoSrc, href } from "./links";

/**
 * One decoration: running, described, and with the Rust that makes it.
 *
 * Nothing here is written per decoration. The label and the note come from the
 * gallery's own list, the code is cut from the function that draws it, and the
 * demo is the shared story addressed by state — which is what lets fifteen
 * pages share one WebAssembly build and one measured height.
 */
export function DecorationPage({ slug }: { readonly slug: string }) {
  const decoration = decorationBySlug(slug);
  if (!decoration) {
    return (
      <div className="shell">
        <h1>Unknown decoration</h1>
        <p className="lede">
          Nothing in the effects list has the slug <code>{slug}</code>.{" "}
          <a href={href("/effects/")}>Browse all effects</a>.
        </p>
      </div>
    );
  }

  const previous = previousDecoration(slug);
  const next = nextDecoration(slug);

  return (
    <div className="shell">
      <p className="eyebrow">
        <a href={href("/effects/")}>Effects</a> · Decoration
      </p>
      <h1>{decoration.label}</h1>
      <p className="lede">{decoration.note}</p>

      <Demo
        story={effects.story}
        title={`${decoration.label} — gpui-ai`}
        height={effects.height}
        variant={decoration.slug}
        caption={`The decorations story, opened on ${decoration.label}. The switcher above the card is the story's own — every state it offers has a page here.`}
      />

      <section aria-labelledby="code">
        <h2 id="code">Code</h2>
        <p className="lede">
          Cut from the gallery&rsquo;s own decorations, so it is the code that drew the frame
          above rather than a description of it.
        </p>
        <CodePanel
          slug={effects.story}
          variant={decoration.slug}
          label={`the ${decoration.label} decoration`}
          file={effects.source}
          actions={[
            { href: demoSrc(effects.story, undefined, decoration.slug), text: "Open in the gallery" },
            {
              href: `${build.repository}/blob/main/${effects.source}`,
              text: "Implementation",
            },
          ]}
        />
      </section>

      <section aria-labelledby="applying">
        <h2 id="applying">Applying it</h2>
        <p>
          Every framed component takes a <code>Decoration</code>, and the layer decides for itself
          whether it belongs to the shape — either by carrying the frame&rsquo;s radius, or by not
          reaching a corner. <a href={href("/effects/")}>The two rules</a> are the whole contract.
        </p>
        <CodePanel
          slug={effects.story}
          variant="applying"
          label="applying a decoration"
          file={effects.source}
        />
      </section>

      <nav className="pager" aria-label="Decorations">
        {previous ? (
          <a className="previous" href={href(`/effects/${previous.slug}/`)} rel="prev">
            <span>Previous</span>
            {previous.label}
          </a>
        ) : (
          <span />
        )}
        {next ? (
          <a className="next" href={href(`/effects/${next.slug}/`)} rel="next">
            <span>Next</span>
            {next.label}
          </a>
        ) : null}
      </nav>
    </div>
  );
}
