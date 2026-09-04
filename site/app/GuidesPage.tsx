import { Fragment, type ReactNode } from "react";
import { Code, CodeFrame } from "./CodePanel";
import { sample } from "./code";
import { build, components, install, themes } from "./data";
import { docBySlug, docs } from "./docs.mjs";
import { href } from "./links";

/**
 * The guide pages, the index over them, and the body `/start/` renders.
 *
 * Prose, written here rather than generated, because it is the one part of
 * this site that is an argument rather than a description: what the library
 * assumes, who owns what, and which promises hold. The numbers inside it are
 * still read from the generated data — the component count, the theme count,
 * the pinned revisions — so a page cannot claim there are thirty components
 * when there are thirty-four.
 *
 * The code is not written here. Every sample is a real file in
 * `site/content/samples/`, highlighted by the same tokeniser the component
 * snippets go through, so it re-skins with the page and can be read on its own.
 */
export function GuidePage({ slug }: { readonly slug: string }) {
  const doc = docBySlug(slug);
  if (!doc) return null;
  const index = docs.findIndex((entry) => entry.slug === slug);
  const previous = docs[index - 1];
  const next = docs[index + 1];

  return (
    <article className="doc">
      <p className="eyebrow">Guide</p>
      <h1>{doc.title}</h1>
      <p className="lede">{doc.summary}</p>

      <Body slug={slug} />

      <nav className="doc-neighbours" aria-label="Other documentation">
        {previous ? (
          <a href={href(`/guides/${previous.slug}/`)} rel="prev">
            <span>Previous</span>
            <strong>{previous.title}</strong>
          </a>
        ) : (
          <span />
        )}
        {next ? (
          <a href={href(`/guides/${next.slug}/`)} rel="next">
            <span>Next</span>
            <strong>{next.title}</strong>
          </a>
        ) : null}
      </nav>
    </article>
  );
}

/** Every guide, listed in reading order. */
export function GuidesIndex() {
  return (
    <div className="shell">
      <h1>Guides</h1>
      <p className="lede">
        How the library is themed, who owns what between it and your application, and the two
        things that are true of every component in it. Installing it is over at{" "}
        <a href={href("/start/")}>Start</a>.
      </p>

      <nav className="doc-index" aria-label="Guides">
        {docs.map((doc) => (
          <a key={doc.slug} href={href(`/guides/${doc.slug}/`)}>
            <strong>{doc.title}</strong>
            <span>{doc.summary}</span>
          </a>
        ))}
      </nav>
    </div>
  );
}

function Body({ slug }: { readonly slug: string }) {
  switch (slug) {
    case "differences":
      return <Differences />;
    case "theming":
      return <Theming />;
    case "ownership-and-events":
      return <Ownership />;
    case "accessibility-and-motion":
      return <Accessibility />;
    case "browser-demos":
      return <BrowserDemos />;
    default:
      return null;
  }
}

/**
 * What this library is, relative to the one it is built on.
 *
 * The prose is an argument; the table under it is not written here at all.
 * Every row comes from the same exported catalog the component pages read, so
 * a component cannot say one thing on its own page and another here, and a
 * component added in Rust arrives in this table without anyone remembering it.
 */
function Differences() {
  const counted = (kind: string) =>
    components.filter((component) => component.lineage.kind === kind).length;
  const categories: string[] = [];
  for (const component of components) {
    if (!categories.includes(component.category)) categories.push(component.category);
  }

  return (
    <>
      <p>
        gpui-ai is built on <a href="https://github.com/longbridge/gpui-component">gpui-component</a>,
        and is not a replacement for it. If you need a button, an input, a dialog or a dock, that is
        the library to reach for — this one does not wrap them and does not re-export them. What it
        adds is the layer above: the surfaces an application grows when a model is doing the work,
        where the data arrives while it is being drawn and a person has to be able to follow, check
        and stop it.
      </p>
      <p>
        The line between the two moves, and this page moves with it: gpui-component has since grown
        chat parts of its own — a message scroller, an attachment surface, bubbles and markers — and
        where they meet something here, the row below says so rather than leaving you to find out.
      </p>
      <p>
        Every component below says which of three things it is. The words are the library&rsquo;s own,
        exported from the same place its API documentation comes from.
      </p>

      <ul className="lineage-counts">
        <li className="lineage-new">
          <strong>{counted("new")}</strong>
          <span>
            <b>New.</b> Built from primitives. Upstream has no component for the thing at all, or has
            one whose look cannot be brought into line with this library&rsquo;s.
          </span>
        </li>
        <li className="lineage-extends">
          <strong>{counted("extends")}</strong>
          <span>
            <b>Extends.</b> Upstream&rsquo;s component does the work; this adds what an agent surface
            needs and hands the rest back.
          </span>
        </li>
        <li className="lineage-composes">
          <strong>{counted("composes")}</strong>
          <span>
            <b>Composes.</b> An upstream component is mounted inside a surface upstream has no
            equivalent for — you could not reach it by configuring anything it ships.
          </span>
        </li>
      </ul>

      <p>
        There is no fourth kind. Nothing in the catalogue is an upstream component under a new name:
        the library re-exports none of them, so every page you can open is one of the three above.
        Button, icon, spinner and text rendering count as primitives here — nearly everything uses
        them, and counting them would put every component in one bucket and tell you nothing.
      </p>

      <Section id="every-component" title="Every component">
        <div className="lineage-table-scroll">
          <table className="lineage-table">
            <thead>
              <tr>
                <th scope="col">Component</th>
                <th scope="col">Kind, and what it stands on</th>
                <th scope="col">What it adds, and why it is here</th>
              </tr>
            </thead>
            <tbody>
              {categories.map((category) => (
                <Fragment key={category}>
                  <tr className="lineage-category">
                    <th scope="colgroup" colSpan={3}>
                      {category}
                    </th>
                  </tr>
                  {components
                    .filter((component) => component.category === category)
                    .map((component) => (
                      <tr key={component.slug}>
                        <th scope="row">
                          <a href={href(`/components/${component.slug}/`)}>{component.api}</a>
                        </th>
                        <td className="lineage-kind-cell">
                          <span className={`lineage-tag lineage-${component.lineage.kind}`}>
                            {component.lineage.label}
                          </span>
                          {component.lineage.basis ? (
                            <code>{component.lineage.basis}</code>
                          ) : (
                            <span className="lineage-none">not built on one</span>
                          )}
                        </td>
                        <td>{component.lineage.note}</td>
                      </tr>
                    ))}
                </Fragment>
              ))}
            </tbody>
          </table>
        </div>
      </Section>

      <Section id="one-more" title="One extension that is not a component">
        <p>
          <code>ButtonLabelExt</code> is the one place the library extends an upstream{" "}
          <em>primitive</em> in public API. Upstream&rsquo;s label squeezes its glyphs into a one-em
          box; this composes the button&rsquo;s child slot instead, so descenders and accents keep the
          theme&rsquo;s leading. Everything else about the button — sizes, colours, focus, activation,
          disabled states — stays upstream&rsquo;s.
        </p>
      </Section>

      <Section id="which-one" title="Which library you want">
        <p>
          If you are building an application interface — settings, forms, panels, menus — gpui-component
          is the whole answer and this library has nothing to add to it. If your interface has a model
          in it, the difference is not decoration: a grid that has to draw while its rows are still
          arriving, a transcript that keeps its place as messages land off screen, a tool call a person
          can allow or deny, a reasoning trace that folds itself away when it settles. Those are the
          {" "}
          {components.length} surfaces here, and each one names above what it is standing on.
        </p>
      </Section>
    </>
  );
}

/** A code sample from `site/content/samples/`, with the file strip above it. */
function Sample({ name, file }: { readonly name: string; readonly file: string }) {
  const code = sample(name);
  return (
    <div className="code-panel">
      <CodeFrame file={file} />
      <Code lines={code.lines} fallback={code.code} />
    </div>
  );
}

function Section({
  id,
  title,
  children,
}: {
  readonly id: string;
  readonly title: string;
  readonly children: ReactNode;
}) {
  return (
    <section aria-labelledby={id}>
      <h2 id={id}>{title}</h2>
      {children}
    </section>
  );
}

/**
 * What `/start/` renders.
 *
 * It lives here because it is the same kind of writing as the guides and reads
 * from the same generated numbers; it is exported rather than routed here
 * because it answers a different question, and a visitor looking for "how do I
 * get it" should not have to find a section called Docs first.
 */
export function StartBody() {
  const gpui = build.upstream.find((entry) => entry.id === "gpui");
  const component = build.upstream.find((entry) => entry.id === "gpui-component");

  return (
    <>
      <Section id="requirements" title="What you need">
        <p>
          A Rust toolchain, and a machine that can run a GPUI application: GPUI draws through the
          platform&rsquo;s GPU API, so a working graphics stack is a requirement rather than an
          optimisation. On Linux that means the usual X11 or Wayland development packages, which is
          what the <code>x11</code> and <code>wayland</code> features below pull in.
        </p>
      </Section>

      <Section id="install" title="Adding the dependency">
        <p>
          Four crates, because an application uses all four directly: <code>gpui</code> and{" "}
          <code>gpui-component</code> are what you write interface against,{" "}
          <code>gpui_platform</code> opens the window, and <code>gpui-ai</code> is this library.
        </p>
        <Sample name="install" file="Cargo.toml" />
        <p>
          GPUI is on crates.io under another name: GPUI Kit publishes a snapshot of Zed&rsquo;s
          crate as <code>{gpui?.crate}</code>, so the dependency carries a <code>package</code>{" "}
          rename and every <code>use gpui::</code> path in your application keeps working. This
          release was built against{" "}
          <a href={gpui?.repository}>
            <code>
              {gpui?.crate} {gpui?.version}
            </code>
          </a>{" "}
          and{" "}
          <a href={component?.repository}>
            <code>
              {component?.crate} {component?.version}
            </code>
          </a>
          . They are one compatible set: a different <code>{gpui?.crate}</code> underneath{" "}
          <code>gpui-component</code> gives you two copies of GPUI&rsquo;s types, and nothing
          shared between them will compile.
        </p>
      </Section>

      <Section id="first-window" title="A window with a component in it">
        <p>
          One call during setup — <code>gpui_ai::init</code>, which initializes
          gpui-component too — and a <code>Root</code> around the top-level view.{" "}
          <code>Root</code> owns the theme, the rem size, and the layer that dialogs and popovers
          are drawn into; a component rendered outside one has no theme to read.
        </p>
        <Sample name="start-window" file="src/main.rs" />
        <p>
          Stateless components are fluent builders you construct where you render them, as above.
          Stateful ones are GPUI entities: keep the entity and its subscriptions in your own state,
          because their lifetime is your application&rsquo;s, not the frame&rsquo;s.
        </p>
      </Section>

      <Section id="next" title="Where to go next">
        <p>
          <a href={href("/components/")}>All {components.length} components</a>, each with its
          demo, its events, and the exact code its story is cut from. Then{" "}
          <a href={href("/guides/ownership-and-events/")}>Ownership and events</a>, which is the one
          idea the whole library is arranged around.
        </p>
      </Section>
    </>
  );
}

function Theming() {
  return (
    <>
      <Section id="tokens" title="There is no styling layer">
        <p>
          Every colour, radius, spacing value, shadow, and type style resolves through{" "}
          <code>cx.theme()</code>. gpui-ai adds no styling layer of its own and contains no
          hardcoded colours, so light and dark, the {themes.length} bundled themes, a theme of your
          own, and live token editing all work without a single per-component override.
        </p>
        <Sample name="theme-tokens" file="src/panel.rs" />
        <p>
          Layout resolves through semantic spacing tokens and the rem scale rather than raw pixels,
          which is what makes window zoom work. A test in this repository enforces it, so a stray{" "}
          <code>px(12.)</code> fails the build rather than reaching a release.
        </p>
      </Section>

      <Section id="bundled" title="The bundled themes are files">
        <p>
          The {themes.length} themes on the <a href={href("/themes/")}>Themes page</a> are JSON
          files in <code>themes/</code>, embedded at build time and registered at startup. Adding a
          file adds a theme; there is no table of presets to update alongside it.
        </p>
        <p>
          Each file declares a name, a mode, and its colours, and may declare a corner radius, a
          shadow flag, and a base font size.
        </p>
        <p>
          Anything a theme leaves out is worth knowing about. Applying a theme writes only the
          metrics that theme names, and what it leaves behind is the <em>previous</em>{" "}
          theme&rsquo;s value rather than the default. An application that installs one theme and
          keeps it never sees this. One that lets a person switch does: a theme asking for 14&nbsp;px
          type leaves every theme chosen after it at 14&nbsp;px until the process restarts. Three of
          the {themes.length} bundled themes declare metrics and the rest do not, so the sample
          below puts back what the incoming theme does not mention.
        </p>
      </Section>

      <Section id="custom" title="Using your own">
        <p>
          Load a theme pack into the registry and switch to one of the themes in it. The format is
          the one the bundled packs use, so a theme downloaded from the Themes page is a working
          starting point.
        </p>
        <Sample name="theme-custom" file="src/theme.rs" />
        <p>
          The website is painted from the same numbers as the demos inside it. That is not a
          coincidence to admire: it is the check. If a theme reads badly on this site, it reads
          badly in an application.
        </p>
      </Section>
    </>
  );
}

function Ownership() {
  return (
    <>
      <Section id="who-owns" title="Applications own state; components render snapshots">
        <p>
          No component in this library fetches anything, keeps fixture data, or runs a clock of its
          own. Your application owns the content, the lifecycle, and the timing of the work, and
          hands a component the current state each frame. A component may still time something it
          is entirely responsible for — the &ldquo;Copied&rdquo; confirmation on a chat message
          clears itself after a couple of seconds, and dropping the chat drops it — but nothing
          that a snapshot describes is ever advanced from inside. That is what makes every component testable without a network,
          and what makes a streamed reply resumable, cancellable, and replayable by you rather than
          by us.
        </p>
        <Sample name="own-progressive" file="src/reply.rs" />
        <p>
          <code>Progressive&lt;T&gt;</code> pairs content with a lifecycle —{" "}
          <code>Pending</code>, <code>Running</code>, <code>Complete</code>, or{" "}
          <code>Failed</code> with a reason — and a revision that changes only when one of them
          actually does. Every component that shows progressive work consumes the same type, so
          there is one lifecycle to learn rather than one per component.
        </p>
      </Section>

      <Section id="events" title="Events are keyed by your identifiers">
        <p>
          Components emit typed events carrying the identifier you gave the thing, never its
          position in a collection. A list that reorders, filters, or loses a row while a decision
          is in flight still resolves to the thing the decision was about — an index would not.
        </p>
        <Sample name="own-events" file="src/approvals.rs" />
      </Section>

      <Section id="cues" title="Cues, for the things that are not state">
        <p>
          Some moments are worth a sound or a haptic: a reply arriving, a response settling, text
          copied, a prompt submitted, work cancelled, a gate decided. The library never plays audio
          — it says the moment happened and leaves the decision to you, in one place rather than at
          every call site.
        </p>
        <Sample name="own-cues" file="src/feedback.rs" />
        <p>
          Cues are hints, never state. Every cue corresponds to a typed event or a snapshot
          transition your application already receives, so ignoring cues entirely loses nothing but
          the sound.
        </p>
      </Section>
    </>
  );
}

function Accessibility() {
  return (
    <>
      <Section id="motion" title="Reduced motion resolves to a useful frame">
        <p>
          Every effect in the library is built on GPUI&rsquo;s animation system, so a reduced-motion
          run is not a run with the animation skipped and nothing in its place. One-shot reveals
          settle at their end state; repeating effects — the text shimmer, the breathing signal —
          render at rest. A reader who has asked for less motion sees the finished answer, not an
          empty box that was waiting to be animated into.
        </p>
        <p>
          Nothing installs idle redraw on its own. A component animates because it was asked to,
          which is why a page of them does not keep a machine awake.
        </p>
        <p>
          The demos on this site honour the setting too, and any of them can be pinned either way
          with <code>motion=reduced</code> or <code>motion=full</code> — see{" "}
          <a href={href("/guides/browser-demos/")}>Browser demo limits</a>.
        </p>
      </Section>

      <Section id="keyboard" title="Keyboard and focus">
        <p>
          Interactive components are reachable and operable from the keyboard, and every control
          carries an accessible name and role rather than relying on the shape it happens to be
          drawn as. Composites expose their structure — a conversation is a list of messages, a
          gate is a group with a question and two answers — so a screen reader can move through
          them rather than reading a wall.
        </p>
        <p>
          One caveat, and it is the browser&rsquo;s:{" "}
          <a href={href("/guides/browser-demos/")}>keyboard action dispatch is native-only</a> on the
          pinned GPUI revision under WebAssembly. Pointer activation works in both.
        </p>
      </Section>

      <Section id="contrast" title="Contrast">
        <p>
          The bundled themes are shown exactly as published, including where a palette makes a
          choice we would not have. What this site does guarantee is its own chrome: secondary text
          is painted from a derived token measured against the background it actually sits on, for
          every one of the {themes.length} themes, rather than from a value that happened to look
          right in one of them.
        </p>
        <p>
          A theme is a set of numbers, and whether those numbers read is a property of the numbers.
          If a theme on the <a href={href("/themes/")}>Themes page</a> is hard to read here, it will
          be hard to read in an application, which is the useful thing to learn before shipping it.
        </p>
      </Section>
    </>
  );
}

function BrowserDemos() {
  return (
    <>
      <Section id="what-they-are" title="What the demos are">
        <p>
          Every demo on this site is the real component, compiled to WebAssembly and drawn by the
          same code a native application runs. They are not videos, screenshots, or a re-implemented
          preview. One binary carries all {components.length} stories, which is why a demo starts
          when it scrolls into view rather than on page load.
        </p>
        <p>
          A demo stops when it is more than a viewport away, and at most three run at once. Each one
          is a WebAssembly instance with its own heap and a GPU surface, and a long page of them
          left running would cost more than the page is worth.
        </p>
      </Section>

      <Section id="webgpu" title="They need WebGPU">
        <p>
          There is no WebGL fallback. Without WebGPU the site shows a still frame captured from the
          real component and downloads nothing — a seventeen-megabyte binary that cannot be used is
          worse than an honest picture.
        </p>
      </Section>

      <Section id="motion" title="They follow your motion preference">
        <p>
          A demo reads <code>prefers-reduced-motion</code> and tells the running gallery, so a
          reader who has asked their machine for stillness gets the same answer here as in a native
          build. GPUI takes that preference from the platform, and the web platform has none —
          without this every demo on the site shimmered at them.
        </p>
        <p>
          Adding <code>motion=reduced</code> or <code>motion=full</code> to a demo&rsquo;s address
          pins it either way, which is how you can see what reduced motion does to a component
          without changing a system setting to find out.
        </p>
      </Section>

      <Section id="keyboard" title="Keyboard in the browser">
        <p>
          Keyboard input works in the live demos: Tab and Shift+Tab move focus, arrow keys reach
          the components that use them, and text selection and Ctrl+C behave. Earlier builds froze
          a demo on the first key press because GPUI&rsquo;s profiler — whose action path reads a
          clock that <code>wasm32-unknown-unknown</code> does not implement — was compiled into
          everything. Upstream now scopes that feature to the one crate that uses it, so the web
          build no longer contains the failing path at all.
        </p>
        <p>
          The native build remains the runtime the keyboard, clipboard, and accessibility test
          suites actually exercise; treat the browser as a faithful demonstration rather than the
          verified surface.
        </p>
      </Section>

      <Section id="authority" title="The native runtime decides">
        <p>
          Where the browser and a native build disagree, the native build is the one that has been
          verified: it is the runtime the test suite exercises and the one the components are
          designed for. The demos exist so you can see a component before depending on it, not to
          define what it does.
        </p>
      </Section>
    </>
  );
}
