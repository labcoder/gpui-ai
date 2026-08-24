import { Demo } from "./Demo";
import { build, categories, components, componentsByCategory, hero, themes } from "./data";
import { href } from "./links";
import { useTheme } from "./theme";

/**
 * Themes offered on the front page, as a row of swatches.
 *
 * A short, deliberately mixed set: the two defaults, the accessible one, and
 * four that look nothing like each other. The masthead picker has all
 * forty-five; this is here so a visitor finds out within one click that the
 * page and the demo above it are painted from the same file.
 */
const STRIP = [
  "light",
  "dark",
  "contrast",
  "midnight-violet",
  "ember-dusk",
  "nord-frost",
  "solstice",
] as const;

/**
 * What gpui-ai is, shown before it is described.
 *
 * The two lines that install it come first, because someone who has already
 * decided wants them and should not have to scroll a demo to reach them. Then
 * the hero: the guided demo, a working prompt bar that runs a scripted
 * exchange when it is sent. It is the same binary every other demo on the site
 * runs, which is the claim the whole site makes.
 */
export function HomePage() {
  return (
    <div className="shell">
      <h1>Components for software that thinks out loud</h1>
      <p className="lede">
        {"AI-native UI for "}
        <a href="https://gpui.rs/">GPUI</a>
        {`, the Rust framework behind Zed. ${components.length} components across ${categories.length} categories and ${themes.length} themes, each one demonstrated by the real compiled component rather than a screenshot.`}
      </p>

      <section aria-labelledby="install">
        <h2 id="install">Install the latest release</h2>
        <div className="install">
          <p className="lede">
            Not on crates.io yet: GPUI itself is only published from Git, so this is too.
          </p>
          <pre className="code">
            <code>{`[dependencies]
gpui-ai = { git = "${build.repository}", tag = "v${build.version}" }
gpui = { git = "https://github.com/zed-industries/zed" }`}</code>
          </pre>
        </div>
      </section>

      {hero.height ? (
        <Demo
          story={hero.slug}
          title={hero.windowTitle}
          height={hero.height}
          caption="Send the question, or pick a suggestion. The tool calls, the reasoning and the reply are a fixed script — this demonstrates the components, not a language model."
        />
      ) : null}

      <ThemeStrip />

      <section aria-labelledby="categories">
        <h2 id="categories">What is in it</h2>
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
            <dt>Composed, never forked</dt>
            <dd>
              Everything here is built on gpui-component rather than replacing it, so an
              application already using it keeps its own controls, its own theme, and its upgrade
              path.
            </dd>
          </div>
          <div>
            <dt>The application owns the data</dt>
            <dd>
              A component renders what it is given and reports what was done to it through a typed
              event. None of them fetch, retry, or persist anything on your behalf.
            </dd>
          </div>
          <div>
            <dt>Themes are files</dt>
            <dd>
              A theme is JSON in a directory. Adding one adds it to the registry, to this site, and
              to every demo on it, with no code to change.
            </dd>
          </div>
          <div>
            <dt>Motion you can turn off</dt>
            <dd>
              Every animation has a reduced-motion path that lands on a useful state rather than
              stopping partway through one.
            </dd>
          </div>
        </dl>
      </section>

      <section aria-labelledby="architecture">
        <h2 id="architecture">How this site works</h2>
        <p className="lede">
          Every page is real HTML, built ahead of time. Every demo on the site is the same
          WebAssembly binary — one build of the same gallery that runs natively — fetched only
          when a frame scrolls into view, and drawn on a WebGPU canvas. Each frame runs its own
          instance; what they share is the download. Nothing here is a screenshot.
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
        <h2 id="build">What these demos were built from</h2>
        <p className="lede">
          This site is built from <code>main</code>, so the demos can be ahead of the release
          above. These are the commits they were compiled against — for a Git dependency that is
          the only thing that pins what you get.
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

/**
 * Try it in — a row of swatches that repaint the whole page.
 *
 * Each button is painted from its own theme's values, so the row shows what it
 * is offering rather than naming it. Choosing one is the same choice the
 * masthead picker makes, through the same store, and it lasts.
 */
function ThemeStrip() {
  const { applied, setChoice } = useTheme();
  const offered = STRIP.map((slug) => themes.find((theme) => theme.slug === slug)).filter(
    (theme) => theme !== undefined,
  );

  return (
    <section className="theme-strip" aria-labelledby="try-it-in">
      <h2 id="try-it-in">Try it in</h2>
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
