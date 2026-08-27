import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { narrow } from "./CatalogPage";
import { build, themeGroups } from "./data";
import { href } from "./links";
import type { Route } from "./routes";
import { SYSTEM, paint, useTheme } from "./theme";

/**
 * The control most visitors want, in front of the one most of them do not.
 *
 * Forty-five themes is a browsing decision; light or dark is a reflex. These
 * three are buttons in the masthead and the rest sit behind a picker, and both
 * write to the same store, so neither can fall out of step with the other.
 */
const MODES = [
  { id: SYSTEM, label: "System" },
  { id: "light", label: "Light" },
  { id: "dark", label: "Dark" },
] as const;

/**
 * The site's chrome, on every page.
 *
 * A catalog rail on a wide screen, the same catalog as a modal drawer on a
 * narrow one, a mode control, and a skip link into the content. Everything the
 * shell knows about the current page arrives as the `route` prop, which both
 * the pre-render and the browser derive from the same rule — so there is no
 * state here that the server and the client could disagree about.
 */
export function Shell({
  route,
  children,
}: {
  readonly route: Route;
  readonly children: ReactNode;
}) {
  const { choice, applied, isDark, setChoice } = useTheme();
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [focusSearch, setFocusSearch] = useState(false);
  const [hydrated, setHydrated] = useState(false);
  const closeDrawer = useCallback(() => setDrawerOpen(false), []);

  // Hydration renders the server snapshot, which is the default rather than
  // whatever this visitor chose — that is what lets React reuse the markup
  // instead of throwing it away. Painting on that first pass repaints the page
  // to the default and straight back: a visitor with Ember Dusk stored gets
  // ember-dusk from the inline script, nord-frost from here, then ember-dusk
  // again. Waiting one render lets the store swap in the real value first, in
  // the same pass, so this paints once and with the right answer.
  useEffect(() => {
    // The head's inline script paints the theme before this bundle loads, so
    // data-theme cannot distinguish interactive React from inert pre-rendered
    // markup. Mark readiness only from an effect: by this point hydration has
    // attached the shell's event handlers and browser drivers can safely act.
    document.documentElement.dataset.siteHydrated = "";
    setHydrated(true);
  }, []);

  useEffect(() => {
    if (!hydrated) return;
    // The inline script in the document head painted this before first paint.
    // Repainting here is what keeps it right after a change, and `paint`
    // returns early when nothing moved.
    //
    // Demos are not told anything from here. Each `Demo` owns its own frame
    // and reads the site theme for itself, because a frame may be overriding
    // it — and a shell that posted to every frame on every change would undo
    // that override the moment the page repainted, or a frame reloaded.
    paint(applied, isDark);
  }, [hydrated, applied, isDark]);

  // `/` puts the cursor in the search box, from anywhere on the page.
  //
  // Which box: the one on screen. The rail carries it on a wide window, the
  // catalog page carries its own, and on a narrow window neither is showing —
  // there the shortcut opens the drawer, which is where the search lives.
  // `offsetParent` is what "on screen" means here: the rail is display:none
  // below 60rem, and a hidden input can still be focused, which would take the
  // cursor somewhere the reader cannot see it.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "/" || event.metaKey || event.ctrlKey || event.altKey) return;
      const target = event.target as HTMLElement | null;
      // Someone typing a slash into a field means a slash.
      if (target?.isContentEditable) return;
      if (target && /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName)) return;

      const boxes = [...document.querySelectorAll<HTMLInputElement>("input[data-site-search]")];
      const showing = boxes.find((box) => box.offsetParent !== null);
      event.preventDefault();
      if (showing) {
        showing.focus();
        showing.select();
        return;
      }
      setDrawerOpen(true);
      setFocusSearch(true);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // Opening the drawer to search it and then having to reach for the box would
  // be half a shortcut. The drawer's input does not exist until it is open, so
  // this runs after the render that created it.
  useEffect(() => {
    if (!drawerOpen || !focusSearch) return;
    setFocusSearch(false);
    const box = document.querySelector<HTMLInputElement>(
      ".nav-drawer input[data-site-search]",
    );
    box?.focus();
  }, [drawerOpen, focusSearch]);

  // A drawer left open across a resize would sit invisibly over a desktop
  // layout, holding focus and keeping the page inert.
  useEffect(() => {
    if (!drawerOpen) return;
    const wide = window.matchMedia("(min-width: 60rem)");
    const close = () => {
      if (wide.matches) setDrawerOpen(false);
    };
    wide.addEventListener("change", close);
    return () => wide.removeEventListener("change", close);
  }, [drawerOpen]);

  return (
    <>
      <a className="skip-link" href="#content">
        Skip to content
      </a>

      <Masthead
        choice={choice}
        onChoice={setChoice}
        drawerOpen={drawerOpen}
        onDrawer={setDrawerOpen}
      />

      <div className="layout">
        <aside className="desktop-rail" aria-label="Component catalog">
          {/* The category labels below are h3s. Without an h2 above them the
              document jumps straight from nothing to level three, which is a
              heading-order error and reads as a missing section to anyone
              navigating by heading. The drawer has a visible one already. */}
          <h2 className="visually-hidden">Component catalog</h2>
          <ComponentNav route={route} idPrefix="rail" />
        </aside>
        <main id="content" tabIndex={-1}>
          {children}
        </main>
      </div>

      <Drawer route={route} open={drawerOpen} onClose={closeDrawer} />

      <footer className="site-footer">
        <div className="shell">
          <span>{`gpui-ai v${build.version} · ${build.license}`}</span>
          <nav aria-label="Repository">
            <a href={build.repository}>Source</a>
            <a href={`${build.repository}/releases/tag/v${build.version}`}>Release notes</a>
            <a href={href("/api/")}>API documentation</a>
          </nav>
        </div>
      </footer>
    </>
  );
}

function Masthead({
  choice,
  onChoice,
  drawerOpen,
  onDrawer,
}: {
  readonly choice: string;
  readonly onChoice: (choice: string) => void;
  readonly drawerOpen: boolean;
  readonly onDrawer: (open: boolean) => void;
}) {
  return (
    <header className="masthead">
      <div className="masthead-inner">
        <button
          className="nav-toggle"
          type="button"
          data-nav-toggle=""
          aria-expanded={drawerOpen}
          aria-controls="site-nav-panel"
          onClick={() => onDrawer(!drawerOpen)}
        >
          Index
        </button>
        <a className="wordmark" href={href("/")}>
          gpui-ai
        </a>
        <nav className="masthead-nav" aria-label="Site">
          <a href={href("/components/")}>Components</a>
          <a href={href("/docs/")}>Docs</a>
          <a href={href("/themes/")}>Themes</a>
        </nav>
        <div className="theme-controls">
          <ModeSwitch choice={choice} onChoice={onChoice} />
          <ThemePicker choice={choice} onChoice={onChoice} />
        </div>
      </div>
    </header>
  );
}

/**
 * System, light, and dark, as buttons rather than a menu.
 *
 * Three toggle buttons in a labelled group: reachable by Tab, operable by Enter
 * and Space with no key handling of its own, and each states whether it is the
 * current mode. A styled `<div>` with a click handler would look identical and
 * be unusable without a mouse. Choosing any other theme leaves all three
 * unpressed, which is the honest report — none of them is what is showing.
 */
function ModeSwitch({
  choice,
  onChoice,
}: {
  readonly choice: string;
  readonly onChoice: (choice: string) => void;
}) {
  return (
    <div className="mode-switch" role="group" aria-label="Mode">
      {MODES.map((candidate) => (
        <button
          key={candidate.id}
          type="button"
          data-theme-choice={candidate.id}
          aria-pressed={choice === candidate.id}
          onClick={() => onChoice(candidate.id)}
        >
          {candidate.label}
        </button>
      ))}
    </div>
  );
}

/**
 * Every theme in the registry, grouped the way it ships.
 *
 * A native select, deliberately: forty-five options in three groups is what
 * `<optgroup>` is for, and the platform's own control already handles the
 * keyboard, the screen reader, and the small-screen presentation better than a
 * hand-built listbox would. The library's own themes are split by mode; the
 * vendored pack is one group carrying its licence, which is the condition it
 * is shipped under.
 */
function ThemePicker({
  choice,
  onChoice,
}: {
  readonly choice: string;
  readonly onChoice: (choice: string) => void;
}) {
  const groups = useMemo(() => {
    const own = themeGroups.find((group) => group.id === "gpui-ai");
    const upstream = themeGroups.find((group) => group.id !== "gpui-ai");
    const byMode = (mode: "light" | "dark") =>
      own?.themes.filter((theme) => theme.mode === mode) ?? [];
    const credit = upstream?.license ? ` (${upstream.license})` : "";
    return [
      { key: "own-light", label: "gpui-ai · Light", themes: byMode("light") },
      { key: "own-dark", label: "gpui-ai · Dark", themes: byMode("dark") },
      { key: "upstream", label: `${upstream?.label ?? ""}${credit}`, themes: upstream?.themes ?? [] },
    ].filter((group) => group.themes.length > 0);
  }, []);

  return (
    <p className="theme-picker">
      <label htmlFor="site-theme">Theme</label>
      <select id="site-theme" value={choice} onChange={(event) => onChoice(event.target.value)}>
        <option value={SYSTEM}>Follow the system</option>
        {groups.map((group) => (
          <optgroup key={group.key} label={group.label}>
            {group.themes.map((theme) => (
              <option key={theme.slug} value={theme.slug}>
                {theme.label}
              </option>
            ))}
          </optgroup>
        ))}
      </select>
    </p>
  );
}

/**
 * The catalog as a modal drawer, for screens with no room for a rail.
 *
 * Everything beside the panel is made `inert` while it is open, which is what
 * keeps Tab inside it — the platform's own answer to a focus trap, rather than
 * a hand-written keydown cycle that has to be kept in step with whatever is
 * focusable. Escape closes it and hands focus back to the control that opened
 * it, because focus left on a removed panel lands nowhere.
 */
function Drawer({
  route,
  open,
  onClose,
}: {
  readonly route: Route;
  readonly open: boolean;
  readonly onClose: () => void;
}) {
  const panel = useRef<HTMLDivElement>(null);
  const closeButton = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) return;
    const opened = panel.current;
    closeButton.current?.focus();

    // The shell renders a fragment, so the panel's siblings are the skip link,
    // the masthead, the layout, and the footer — not `document.body`'s
    // children, which is only the React root and would include the panel.
    const siblings = [...(opened?.parentElement?.children ?? [])].filter(
      (child) => child !== opened,
    );
    for (const sibling of siblings) sibling.setAttribute("inert", "");

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
        return;
      }
      if (event.key !== "Tab" || !opened) return;

      // `inert` keeps Tab out of the page behind, but the sequence still runs
      // off the end of the panel into the browser's own chrome. A modal is
      // supposed to cycle, so the two edges wrap by hand.
      const stops = [...opened.querySelectorAll<HTMLElement>("a[href], button, input, [tabindex]")]
        .filter((element) => element.tabIndex >= 0 && element.offsetParent !== null);
      const first = stops[0];
      const last = stops[stops.length - 1];
      if (!first || !last) return;

      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", onKeyDown);

    return () => {
      for (const sibling of siblings) sibling.removeAttribute("inert");
      document.removeEventListener("keydown", onKeyDown);
      // Focus is sitting inside a panel that is about to be hidden, so it has
      // to go somewhere real — and only once the inert attributes are gone, or
      // the browser refuses to move it. The toggle is where the visitor left
      // it, but crossing the desktop breakpoint hides that button, and focus
      // on a display:none element is focus on nothing. The content is the
      // honest fallback.
      const toggle = document.querySelector<HTMLElement>("[data-nav-toggle]");
      const target = toggle?.offsetParent ? toggle : document.getElementById("content");
      target?.focus();
    };
  }, [open, onClose]);

  return (
    <div
      id="site-nav-panel"
      className="nav-panel"
      role="dialog"
      aria-modal="true"
      aria-labelledby="site-nav-title"
      hidden={!open}
      ref={panel}
    >
      {/* Pointer-reachable, never a tab stop, and out of the accessibility
          tree. A button here would be an unnamed control sitting between the
          visitor and the panel; the named Close button and Escape are the
          ways out that a keyboard can find. */}
      <div className="nav-backdrop" data-nav-close="" aria-hidden="true" onClick={onClose} />
      <div className="nav-drawer">
        <div className="nav-drawer-head">
          <h2 id="site-nav-title">Components</h2>
          <button type="button" data-nav-close="" ref={closeButton} onClick={onClose}>
            Close
          </button>
        </div>
        <ComponentNav route={route} idPrefix="drawer" />
      </div>
    </div>
  );
}

/**
 * Every component, grouped the way the catalog groups them, narrowed by one
 * search box.
 *
 * Rendered twice — rail and drawer — so its ids are prefixed. The query starts
 * empty in the pre-render and in the first client render, which is what keeps
 * the two in agreement.
 *
 * It searches the same index the catalog page does, so the rail and the page
 * can never disagree about what a word matches.
 */
function ComponentNav({ route, idPrefix }: { readonly route: Route; readonly idPrefix: string }) {
  const [query, setQuery] = useState("");
  const searchId = `${idPrefix}-component-search`;

  const { grouped, groups } = useMemo(() => narrow(query), [query]);

  const shown = groups.reduce((total, [, entries]) => total + entries.length, 0);

  return (
    <div className="component-nav">
      <div className="nav-search">
        <label htmlFor={searchId}>Find a component</label>
        <input
          id={searchId}
          type="search"
          data-site-search=""
          value={query}
          placeholder="chat, table, approval…"
          onChange={(event) => setQuery(event.target.value)}
        />
        {/* Only worth telling someone who has a keyboard to press it with, and
            aria-hidden because a screen reader reading "slash" after the label
            of every search box is noise, not a shortcut. */}
        <kbd className="search-key" aria-hidden="true">
          /
        </kbd>
        <output htmlFor={searchId} aria-live="polite">{`${shown} shown`}</output>
      </div>
      <nav aria-label="All components">
        {groups.map(([category, entries]) => (
          <section key={category}>
            {grouped ? <h3>{category}</h3> : null}
            <ul>
              {entries.map((component) => {
                const current = route.kind === "component" && route.slug === component.slug;
                return (
                  <li key={component.slug}>
                    <a
                      className="nav-component-link"
                      href={href(`/components/${component.slug}/`)}
                      {...(current ? { "aria-current": "page" as const } : {})}
                    >
                      {component.compactLabel}
                    </a>
                  </li>
                );
              })}
            </ul>
          </section>
        ))}
      </nav>
    </div>
  );
}
