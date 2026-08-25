import type { ReactNode } from "react";
import { Code, CodeFrame } from "./CodePanel";
import { build, components, install, sample, themes } from "./data";
import { docBySlug, docs } from "./docs";
import { href } from "./links";

/**
 * The five documentation pages, and the index over them.
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
export function DocsPage({ slug }: { readonly slug: string }) {
  const doc = docBySlug(slug);
  if (!doc) return null;
  const index = docs.findIndex((entry) => entry.slug === slug);
  const previous = docs[index - 1];
  const next = docs[index + 1];

  return (
    <article className="doc">
      <p className="eyebrow">Documentation</p>
      <h1>{doc.title}</h1>
      <p className="lede">{doc.summary}</p>

      <Body slug={slug} />

      <nav className="doc-neighbours" aria-label="Other documentation">
        {previous ? (
          <a href={href(`/docs/${previous.slug}/`)} rel="prev">
            <span>Previous</span>
            <strong>{previous.title}</strong>
          </a>
        ) : (
          <span />
        )}
        {next ? (
          <a href={href(`/docs/${next.slug}/`)} rel="next">
            <span>Next</span>
            <strong>{next.title}</strong>
          </a>
        ) : null}
      </nav>
    </article>
  );
}

/** Every documentation page, listed in reading order. */
export function DocsIndex() {
  return (
    <div className="shell">
      <h1>Documentation</h1>
      <p className="lede">
        Five pages: what the library needs, how it is themed, who owns what, and the two things
        that are true of every component in it.
      </p>

      <nav className="doc-index" aria-label="Documentation">
        {docs.map((doc) => (
          <a key={doc.slug} href={href(`/docs/${doc.slug}/`)}>
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
    case "getting-started":
      return <GettingStarted />;
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

function GettingStarted() {
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
        <p>
          Git, because this is not on crates.io. Publishing there requires every dependency to
          carry a crates.io version, and the released <code>gpui</code> predates everything this
          library is built on.
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
          Pin <code>gpui-ai</code> to a tag or a revision. Leave <code>gpui</code> without a{" "}
          <code>rev</code>: <code>gpui-component</code> declares the same dependency and Cargo has
          to resolve both to one source. This release was built against{" "}
          <a href={`${gpui?.repository}/commit/${gpui?.commit}`}>
            <code>{gpui?.commit.slice(0, 7)}</code>
          </a>{" "}
          of Zed and{" "}
          <a href={`${component?.repository}/commit/${component?.commit}`}>
            <code>{component?.commit.slice(0, 7)}</code>
          </a>{" "}
          of gpui-component; every release records the pair it was built against.
        </p>
      </Section>

      <Section id="first-window" title="A window with a component in it">
        <p>
          Two calls during setup — <code>gpui_component::init</code> and{" "}
          <code>gpui_ai::init</code> — and a <code>Root</code> around the top-level view.{" "}
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
          <a href={href("/docs/ownership-and-events/")}>Ownership and events</a>, which is the one
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
          shadow flag, and a base font size. Anything it leaves out falls back to the default —
          not to whatever the previously applied theme happened to set.
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
          No component in this library holds a timer, fetches anything, or keeps fixture data. Your
          application owns the content, the lifecycle, and the clock, and hands a component the
          current state each frame. That is what makes every component testable without a network,
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
          <a href={href("/docs/browser-demos/")}>keyboard action dispatch is native-only</a> on the
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

      <Section id="keyboard" title="Keyboard dispatch is native-only">
        <p>
          Pressing Tab, Shift+Tab, or Ctrl+C inside a live demo freezes that demo. The page around
          it keeps working and reloading brings it back. Native builds are unaffected.
        </p>
        <p>
          The cause is upstream and specific: gpui-component enables GPUI&rsquo;s profiler feature
          for everything that depends on it, and the profiler&rsquo;s action handling reads the
          clock through <code>std::time::Instant</code> where the rest of that module uses the
          WebAssembly-safe one. <code>std::time</code> is unimplemented on{" "}
          <code>wasm32-unknown-unknown</code>, so dispatching any action panics — and because
          WebAssembly aborts rather than unwinding, a borrow held across the panic is never
          released and every later update fails. It is a one-import fix in the wrong repository.
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
