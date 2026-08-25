import { Code, CodeFrame } from "./CodePanel";
import { Demo } from "./Demo";
import { build, components, componentsByCategory, hero, install, themes } from "./data";
import { href } from "./links";
import { useTheme } from "./theme";

/** A mixed sample of the themes available from the full picker. */
const STRIP = [
  "light",
  "dark",
  "contrast",
  "midnight-violet",
  "ember-dusk",
  "nord-frost",
  "solstice",
] as const;

/** The project overview, install snippet, and guided demo. */
export function HomePage() {
  return (
    <div className="shell home">
      <h1>Components for AI applications built with GPUI</h1>
      <p className="lede">
        {`${components.length} components for chat, streamed responses, tool calls, approvals, and more. Built for `}
        <a href="https://gpui.rs/">GPUI</a>
        {` and gpui-component, with ${themes.length} included themes.`}
      </p>

      <section className="home-install" aria-labelledby="install">
        <h2 id="install">Install</h2>
        <div className="install">
          <p className="lede">
            gpui-ai installs from Git because its current GPUI dependencies are not available on
            crates.io.
          </p>
          <CodeFrame file="Cargo.toml" />
          <Code lines={install.lines} fallback={install.code} />
        </div>
      </section>

      {hero.height ? (
        <Demo
          story={hero.slug}
          title={hero.windowTitle}
          height={hero.height}
          caption="Send the question or choose a suggestion. The demo runs a fixed script with tool calls, reasoning, and a streamed reply."
        />
      ) : null}

      <ThemeStrip />

      <section aria-labelledby="categories">
        <h2 id="categories">Components</h2>
        <ul className="cards">
          {componentsByCategory().map(([category, entries]) => (
            <li className="card" key={category}>
              <a href={href("/components/")}>{category}</a>
              <p>{`${entries.map((component) => component.title).join(", ")}.`}</p>
              <p className="card-api">
                {`${entries.length} component${entries.length === 1 ? "" : "s"}`}
              </p>
            </li>
          ))}
        </ul>
      </section>

      <section aria-labelledby="principles">
        <h2 id="principles">How it is built</h2>
        <dl className="notes">
          <div>
            <dt>Uses gpui-component</dt>
            <dd>
              gpui-ai composes gpui-component controls and themes. Custom components use GPUI
              directly.
            </dd>
          </div>
          <div>
            <dt>Your application owns state</dt>
            <dd>
              Components render your data and send typed events. Your application handles
              requests, retries, and storage.
            </dd>
          </div>
          <div>
            <dt>JSON themes</dt>
            <dd>
              Add a theme file to include it in the registry, the gallery, and this website.
            </dd>
          </div>
          <div>
            <dt>Reduced motion</dt>
            <dd>
              Animations respect reduced-motion settings and keep their state readable.
            </dd>
          </div>
        </dl>
      </section>

      <section aria-labelledby="architecture">
        <h2 id="architecture">How this site works</h2>
        <p className="lede">
          The site serves static HTML. Your browser runs the gallery&apos;s GPUI components through
          WebAssembly and WebGPU. It downloads the shared gallery once and starts each demo as it
          scrolls into view.
        </p>
        <ul className="chips">
          <li className="chip">
            <a href={href("/api/")}>API documentation</a>
          </li>
          <li className="chip">
            <a href={build.repository}>Source on GitHub</a>
          </li>
          <li className="chip">
            <a href={`${build.repository}/releases/tag/v${build.version}`}>
              {`Release v${build.version}`}
            </a>
          </li>
          <li className="chip">
            <a href={href("/themes/")}>Themes</a>
          </li>
        </ul>
      </section>

      <section aria-labelledby="build">
        <h2 id="build">Build details</h2>
        <p className="lede">
          We build this site from <code>main</code>, so the demos may be ahead of the latest
          release. The links below show the exact revisions.
        </p>
        <dl className="facts">
          <div>
            <dt>Latest release</dt>
            <dd>
              <a href={`${build.repository}/releases/tag/v${build.version}`}>
                {`v${build.version}`}
              </a>
            </dd>
          </div>
          <div>
            <dt>License</dt>
            <dd>{build.license}</dd>
          </div>
          {build.upstream.map((pin) => (
            <div key={pin.id}>
              <dt>{pin.label}</dt>
              <dd>
                <a href={`${pin.repository}/commit/${pin.commit}`}>
                  <code>{pin.commit.slice(0, 12)}</code>
                </a>
              </dd>
            </div>
          ))}
        </dl>
      </section>
    </div>
  );
}

/** Theme swatches that use the same store as the masthead picker. */
function ThemeStrip() {
  const { applied, setChoice } = useTheme();
  const offered = STRIP.map((slug) => themes.find((theme) => theme.slug === slug)).filter(
    (theme) => theme !== undefined,
  );

  return (
    <section className="theme-strip" aria-labelledby="try-it-in">
      <h2 id="try-it-in">Preview themes</h2>
      <ul>
        {offered.map((theme) => (
          <li key={theme.slug}>
            <button
              type="button"
              data-use-theme={theme.slug}
              aria-pressed={applied === theme.slug}
              onClick={() => setChoice(theme.slug)}
              // Its own fill and its own text, so the swatch shows the theme.
              // Not its own border: a theme's border is a divider between two
              // of its surfaces, and against the page it can be invisible —
              // which is how a light swatch disappears on a light page.
              style={{
                background: theme.tokens["--ai-background"],
                color: theme.tokens["--ai-foreground"],
              }}
            >
              {theme.label}
            </button>
          </li>
        ))}
        <li>
          <a href={href("/themes/")}>{`All ${themes.length} themes`}</a>
        </li>
      </ul>
    </section>
  );
}
