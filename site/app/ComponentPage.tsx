import { CodePanel } from "./CodePanel";
import { Demo } from "./Demo";
import {
  build,
  componentBySlug,
  nextComponent,
  previousComponent,
  snippet,
  type Component,
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

  return (
    <div className="shell">
      <p className="eyebrow">{component.category}</p>
      <h1>{component.title}</h1>
      <p className="lede">{component.summary}</p>

      <Demo
        story={component.slug}
        title={component.windowTitle}
        height={component.height}
        caption={`Running the real component, compiled to WebAssembly. The frame is ${component.height} px tall because that is what the story measures.`}
      />

      <Reference component={component} />

      <Variants component={component} />

      <section aria-labelledby="code">
        <h2 id="code">Code</h2>
        <p className="lede">Cut from the gallery story this page runs, so it stays true.</p>
        <CodePanel
          slug={component.slug}
          label={`the ${component.title} snippet`}
          actions={[
            { href: demoSrc(component.slug), text: "Open in the gallery" },
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
 * The states this story can show.
 *
 * Most components have one. Where the gallery offers a switcher, each state is
 * listed with its own snippet if one has been cut for it; D-11 adds the rest,
 * and S-06 wires the switcher to the frame.
 */
function Variants({ component }: { readonly component: Component }) {
  if (component.variants.length === 0) return null;
  return (
    <section aria-labelledby="variants">
      <h2 id="variants">States</h2>
      <p className="lede">
        The gallery story switches between these; the demo above starts on the first.
      </p>
      {component.variants.map((variant) => {
        const code = snippet(component.slug, variant.id);
        return (
          <div className="variant" key={variant.id}>
            <h3>{variant.label}</h3>
            {code ? (
              <CodePanel
                slug={component.slug}
                variant={variant.id}
                label={`the ${variant.label} snippet`}
              />
            ) : null}
          </div>
        );
      })}
    </section>
  );
}
