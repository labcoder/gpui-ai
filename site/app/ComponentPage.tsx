import { useState } from "react";
import { CodePanel } from "./CodePanel";
import { snippet } from "./code";
import { Demo } from "./Demo";
import {
  build,
  componentBySlug,
  nextComponent,
  previousComponent,
  snippetSource,
  type Component,
  type LineageEntry,
} from "./data";
import { apiHref, demoSrc, href, sourceHref } from "./links";

/**
 * One component, entirely from generated data.
 *
 * Nothing on this page is written per component: the prose, the events, the
 * measured demo height, and the Rust all come out of `catalog.json` and
 * `snippets.json`, so a component added in Rust gets a complete page.
 *
 * One column, not two. The shell already spends a rail on the catalog, and a
 * second sidebar here would leave the demo narrower than the width its height
 * was measured at — which is the one thing this page has to get right.
 *
 * The states are the page's state, not the demo's. A story's switcher used to
 * be listed below the frame with a block of Rust under each entry — code for
 * something the reader could not run without leaving the page, beside a demo
 * pinned to whichever state the story opened in. Now one choice moves both.
 */
export function ComponentPage({ slug }: { readonly slug: string }) {
  const component = componentBySlug(slug);
  if (!component) {
    return (
      <div className="shell">
        <h1>Unknown component</h1>
        <p className="lede">
          Nothing in the catalog has the slug <code>{slug}</code>.{" "}
          <a href={href("/components/")}>Browse all components</a>.
        </p>
      </div>
    );
  }

  const previous = previousComponent(slug);
  const next = nextComponent(slug);
  return <Body component={component} previous={previous} next={next} />;
}

/**
 * Where this component stands relative to the library it is built on.
 *
 * High on the page, under the summary, because it answers the question a
 * reader arrives with when they already have gpui-component: is this a thing
 * that library does not have, or a thing it does have with additions? The
 * words are the exporter's, so a component cannot claim one thing here and
 * another in the guide that gathers them all.
 */
function Lineage({ lineage }: { readonly lineage: LineageEntry }) {
  return (
    <aside className={`lineage lineage-${lineage.kind}`}>
      <p className="lineage-head">
        <span className="lineage-tag">{lineage.label}</span>
        {lineage.basis ? (
          <span className="lineage-basis">
            gpui-component <code>{lineage.basis}</code>
          </span>
        ) : (
          <span className="lineage-basis">no upstream counterpart</span>
        )}
      </p>
      <p className="lineage-note">{lineage.note}</p>
      <p className="lineage-more">
        <a href={href("/guides/differences/")}>How every component compares</a>
      </p>
    </aside>
  );
}

function Body({
  component,
  previous,
  next,
}: {
  readonly component: Component;
  readonly previous: Component | undefined;
  readonly next: Component | undefined;
}) {
  const states = component.variants;
  // The state the page is showing. `undefined` while there is nothing to
  // choose, and the first state otherwise — which is the one the story opens
  // in, so the page and the frame agree before anyone touches either.
  const [showing, setShowing] = useState<string | undefined>(states[0]?.id);
  const shown = states.find((state) => state.id === showing);
  // Whether this state has Rust of its own, or shares the story's. Most do
  // not yet: a story marks one region, and a state that only changes the data
  // it is given has no separate code to show. Saying which is what stops the
  // panel from claiming to be something it is not.
  const ownCode = Boolean(shown && snippet(component.slug, shown.id));

  return (
    <div className="shell">
      <p className="eyebrow">{component.category}</p>
      <h1>{component.title}</h1>
      <p className="lede">{component.summary}</p>

      <Lineage lineage={component.lineage} />

      <Demo
        story={component.slug}
        title={component.windowTitle}
        height={component.height}
        variants={states.length > 0 ? states : undefined}
        variant={showing}
        onVariant={setShowing}
        caption={`Running the real component, compiled to WebAssembly. The frame is ${component.height} px tall, which is what this story measures at the demo width in the default type size — a narrower column or a theme that changes that size will wrap it differently.`}
      />

      <Reference component={component} />

      <section aria-labelledby="code">
        <h2 id="code">Code</h2>
        <p className="lede">
          {!shown
            ? "Cut from the gallery story this page runs, so it stays true."
            : ownCode
              ? `Cut from the gallery story this page runs — the ${shown.label} state, which is the one showing above.`
              : `Cut from the gallery story this page runs. The states above change what the demo is given rather than how it is built, so they share this code.`}
        </p>
        <CodePanel
          slug={component.slug}
          variant={ownCode ? shown?.id : undefined}
          label={`the ${component.title} snippet`}
          file={snippetSource}
          actions={[
            { href: demoSrc(component.slug, undefined, shown?.id), text: "Open in the gallery" },
            { href: sourceHref(component, build.repository), text: "Implementation" },
          ]}
        />
      </section>


      <section aria-labelledby="ownership">
        <h2 id="ownership">Events and ownership</h2>
        <dl className="notes">
          <div>
            <dt>Who owns the state</dt>
            <dd>{component.behavior.ownership}</dd>
          </div>
          <div>
            <dt>What it emits</dt>
            <dd>{component.behavior.interaction}</dd>
          </div>
          <div>
            <dt>What it means</dt>
            <dd>{component.behavior.semantics}</dd>
          </div>
          <div>
            <dt>Overflow and motion</dt>
            <dd>{component.behavior.overflow}</dd>
          </div>
        </dl>
        {component.events.length > 0 ? (
          <>
            <h3>Event types</h3>
            <ul className="chips">
              {component.events.map((event) => (
                <li className="chip" key={event}>
                  {event}
                </li>
              ))}
            </ul>
          </>
        ) : null}
      </section>

      <section aria-labelledby="limits">
        <h2 id="limits">What the demo does and does not show</h2>
        <div className="caveat">
          <p>
            The demo above is the same Rust as the native component, compiled to WebAssembly and
            drawn on a WebGPU canvas — not a mock-up, a recording, or a re-implementation. What it
            paints is what the component paints: layout, colour from the live theme, and motion.
          </p>
          <p>{component.limitation}</p>
          <p>
            A canvas cannot stand in for the platform, so treat everything that goes through the
            operating system as unproven here: keyboard handling and shortcuts, screen reader
            semantics, text input and IME, the clipboard, file drops, and the frame rate you would
            get on your own GPU. Judge those in a native build. If your browser has no WebGPU, the
            frame says so rather than showing an empty box.
          </p>
        </div>
      </section>

      <nav className="pager" aria-label="Catalog">
        {previous ? (
          <a className="previous" href={href(`/components/${previous.slug}/`)} rel="prev">
            <span>Previous</span>
            {previous.title}
          </a>
        ) : null}
        {next ? (
          <a className="next" href={href(`/components/${next.slug}/`)} rel="next">
            <span>Next</span>
            {next.title}
          </a>
        ) : null}
      </nav>
    </div>
  );
}

/**
 * Where this component lives, directly under the thing it describes.
 *
 * In the flow rather than in a sidebar, so it is the same on a phone as on a
 * desktop — a reference hidden behind a breakpoint is a reference nobody on a
 * small screen has.
 */
function Reference({ component }: { readonly component: Component }) {
  return (
    <dl className="component-reference" aria-label="Reference">
      <div>
        <dt>Type</dt>
        <dd>
          <a href={apiHref(component)}>
            <code>{component.api}</code>
          </a>
        </dd>
      </div>
      <div>
        <dt>Source</dt>
        <dd>
          <a href={sourceHref(component, build.repository)}>
            <code>{component.source.replace("crates/gpui-ai/src/", "")}</code>
          </a>
        </dd>
      </div>
      <div>
        <dt>Story</dt>
        <dd>
          <code>{component.slug}</code>
        </dd>
      </div>
      <div>
        <dt>Demo height</dt>
        <dd>{`${component.height} px`}</dd>
      </div>
      <div>
        <dt>Events</dt>
        <dd>{component.events.length > 0 ? component.events.join(", ") : "None"}</dd>
      </div>
    </dl>
  );
}


/**
 * States the story offers that no snippet has been cut for.
 *
 * The ones that have code are the switcher above the demo. This is what is
 * left: named, so the page does not quietly pretend the story has fewer states
 * than it does, and unswitched, because a button that changes the frame while
 * the code beneath it stays put is the thing this page just stopped doing.
 */
function UnwrittenStates({ component }: { readonly component: Component }) {
  const withCode = component.variants.filter((variant) => snippet(component.slug, variant.id));
  if (withCode.length > 0 || component.variants.length === 0) return null;

  return (
    <section aria-labelledby="variants">
      <h2 id="variants">States</h2>
      <p className="lede">
        The gallery story switches between these; the demo above opens on the first.
      </p>
      <ul className="chips">
        {component.variants.map((variant) => (
          <li className="chip" key={variant.id}>
            {variant.label}
          </li>
        ))}
      </ul>
    </section>
  );
}
