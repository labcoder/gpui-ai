import { themeGroups, themes } from "./data";

/**
 * Every bundled theme.
 *
 * Provisional: S-11 gives each theme the same demo trio and a "use on site"
 * control. For now it lists what exists and where each group comes from, which
 * is what the upstream pack's license requires anyway.
 */
export function ThemesPage() {
  return (
    <div className="shell">
      <h1>Themes</h1>
      <p className="lede">
        {`${themes.length} presets.`} Each one is a JSON file in <code>themes/</code>; the site,
        the gallery, and the demos are all painted from the same numbers.
      </p>

      {themeGroups.map((group) => (
        <section className="category" key={group.id} aria-label={group.label}>
          <h2>{group.label}</h2>
          {group.license ? (
            <p className="lede">
              {`${group.themes.length} themes from `}
              {group.source ? <a href={group.source}>{group.label}</a> : group.label}
              {`, used under ${group.license} and shown as published.`}
            </p>
          ) : (
            <p className="lede">{`${group.themes.length} themes designed for this library.`}</p>
          )}
          <ul className="theme-cards">
            {group.themes.map((theme) => (
              <li className="theme-card" key={theme.slug} data-theme-card={theme.slug}>
                <strong>{theme.label}</strong>
                <dl className="metadata">
                  <dt>Mode</dt>
                  <dd>{theme.mode}</dd>
                  <dt>Radius</dt>
                  <dd>{`${theme.radius} px`}</dd>
                  <dt>Base size</dt>
                  <dd>{`${theme.fontSize} px`}</dd>
                </dl>
                <div className="swatches" aria-hidden="true">
                  {["--ai-background", "--ai-foreground", "--ai-primary", "--ai-accent"].map(
                    (token) => (
                      <i key={token} style={{ background: theme.tokens[token] }} />
                    ),
                  )}
                </div>
              </li>
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}
