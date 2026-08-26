import { Demo } from "./Demo";
import { componentBySlug, themeGroups, themes, type Theme } from "./data";
import { href } from "./links";
import { useTheme } from "./theme";

/**
 * The three components the page compares themes with.
 *
 * Short ones, on purpose. Forty-five themes and three demos each would be a
 * hundred and thirty-five WebAssembly frames; one trio that re-skins is both
 * lighter and the better comparison, because it is the same pixels changing
 * rather than different ones side by side.
 */
const TRIO = ["loading", "tool-chips", "context"] as const;

/**
 * Every bundled theme, and one set of components to judge them by.
 *
 * The cards are not pictures of themes — choosing one repaints this page, the
 * trio above it, and everything else on the site, because the site and the
 * demos read the same numbers.
 */
export function ThemesPage() {
  const { applied, setChoice } = useTheme();

  return (
    <div className="shell">
      <h1>Themes</h1>
      <p className="lede">
        {`${themes.length} presets. Each one is a JSON file in `}
        <code>themes/</code>
        {" — the site, the gallery, and the demos are all painted from the same numbers, so a theme that works here works in an application."}
      </p>

      <section aria-labelledby="trio">
        <h2 id="trio">The same three, in whatever you pick</h2>
        <div className="theme-trio">
          {/* One composed story rather than three: the page compares themes,
              and each separate demo cost a whole WebAssembly runtime and a
              WebGPU context for pixels the composition shows in one. The
              reservation sums the standalone heights plus the composition's
              two gaps; the demo's own measured report refines it after the
              first frame. */}
          <Demo
            story="themes-trio"
            title="Themes trio — gpui-ai"
            height={TRIO.reduce((total, slug) => total + (componentBySlug(slug)?.height ?? 0), 48)}
          />
        </div>
      </section>

      {themeGroups.map((group) => (
        <section className="category" key={group.id} aria-label={group.label}>
          <h2>{group.label}</h2>
          {group.license ? (
            <p className="lede">
              {`${group.themes.length} themes from `}
              {group.source ? <a href={group.source}>{group.label}</a> : group.label}
              {`, used under ${group.license} and shown exactly as published — including where a
                palette makes a choice we would not have made.`}
            </p>
          ) : (
            <p className="lede">{`${group.themes.length} themes designed for this library.`}</p>
          )}
          <ul className="theme-cards">
            {group.themes.map((theme) => (
              <ThemeCard
                key={theme.slug}
                theme={theme}
                current={applied === theme.slug}
                onUse={() => setChoice(theme.slug)}
              />
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}

function ThemeCard({
  theme,
  current,
  onUse,
}: {
  readonly theme: Theme;
  readonly current: boolean;
  readonly onUse: () => void;
}) {
  return (
    <li className="theme-card" data-theme-card={theme.slug}>
      <div className="theme-card-head">
        <strong>{theme.label}</strong>
        {current ? <span className="theme-current">In use</span> : null}
      </div>

      {/* Painted from the theme's own values rather than the page's, which is
          the only way forty-five palettes can be shown at once. */}
      <div className="swatches" aria-hidden="true">
        {["--ai-background", "--ai-surface", "--ai-foreground", "--ai-primary", "--ai-accent"].map(
          (token) => (
            <i key={token} style={{ background: theme.tokens[token] }} />
          ),
        )}
      </div>

      <p className="theme-readout">
        {`${theme.mode.toUpperCase()} · RADIUS ${theme.radius}/${theme.radiusLg} · BASE ${theme.fontSize}PX · ${
          theme.shadow ? "SHADOW" : "FLAT"
        }`}
      </p>

      <div className="theme-actions">
        <button type="button" data-use-theme={theme.slug} onClick={onUse} disabled={current}>
          {current ? "In use" : "Use on site"}
        </button>
        <a href={href(`/themes/${theme.slug}.json`)} download={`${theme.slug}.json`}>
          Download
        </a>
      </div>
    </li>
  );
}
