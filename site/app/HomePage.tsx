import { build, categories, components, componentsByCategory, themes } from "./data";
import { href } from "./links";

/**
 * The front page.
 *
 * Provisional: S-09 replaces the top of it with the guided-demo hero, the theme
 * strip, and category posters. What has to be here now is the honest part — what
 * this is, how to depend on it, and exactly which upstream commits it is pinned
 * to, because it installs from Git rather than from crates.io.
 */
export function HomePage() {
  return (
    <div className="shell">
      <h1>gpui-ai</h1>
      <p className="lede">
        AI-native UI components for <a href="https://gpui.rs/">GPUI</a>, the Rust UI framework
        behind Zed.{" "}
        {`${components.length} components across ${categories.length} categories and ${themes.length} themes, each demonstrated here by the real component compiled to WebAssembly.`}
      </p>

      <section aria-labelledby="install">
        <h2 id="install">Install</h2>
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

      <section aria-labelledby="categories">
        <h2 id="categories">What is in it</h2>
        <ul className="cards">
          {componentsByCategory().map(([category, entries]) => (
            <li className="card" key={category}>
              <a href={href("/components/")}>{category}</a>
              <p>{entries.map((component) => component.title).join(", ")}.</p>
            </li>
          ))}
        </ul>
      </section>

      <section aria-labelledby="architecture">
        <h2 id="architecture">How this site works</h2>
        <p className="lede">
          Every page is real HTML, built ahead of time. The demos are one shared WebAssembly binary
          — the same gallery that runs natively — loaded once, only when a demo scrolls into view,
          and drawn on a WebGPU canvas. Nothing here is a screenshot.
        </p>
        <ul className="chips">
          <li className="chip">
            <a href={href("/api/")}>API documentation</a>
          </li>
          <li className="chip">
            <a href={build.repository}>Source on GitHub</a>
          </li>
          <li className="chip">
            <a href={`${build.repository}/releases/tag/v${build.version}`}>{`Release v${build.version}`}</a>
          </li>
          <li className="chip">
            <a href={href("/themes/")}>Themes</a>
          </li>
        </ul>
      </section>

      <section aria-labelledby="build">
        <h2 id="build">This build</h2>
        <dl className="facts">
          <div>
            <dt>Version</dt>
            <dd>{`v${build.version}`}</dd>
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
