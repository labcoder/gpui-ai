import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { build, componentsByCategory, type Component } from "./data";
import { href } from "./links";
import type { Route } from "./routes";

/**
 * The three modes the site ships a control for.
 *
 * The registry holds forty-five themes and S-05 builds the picker over all of
 * them; these are the ones a visitor reaches for without wanting to browse.
 * `contrast` is a real registry theme rather than a filter, so all three go
 * through the same `data-theme` attribute the generated stylesheet keys on.
 */
const MODES = [
  { id: "light", label: "Light" },
  { id: "dark", label: "Dark" },
  { id: "contrast", label: "Contrast" },
] as const;

type Mode = (typeof MODES)[number]["id"];

/** Modes the embedded gallery reads as dark when the host has not named one. */
const DARK_MODES = new Set<Mode>(["dark", "contrast"]);

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
  const [mode, setMode] = useState<Mode>("light");
  const [drawerOpen, setDrawerOpen] = useState(false);
  const closeDrawer = useCallback(() => setDrawerOpen(false), []);

  // The document element is outside React's tree — the pre-render never emits
  // it — so the attribute is written after mount. Reading a stored preference
  // during render is what breaks hydration; S-05 adds persistence through an
  // inline script in the document head, where React cannot disagree with it.
  useEffect(() => {
    const root = document.documentElement;
    root.dataset.theme = mode;
    // The embed guesses light or dark from this class when the host has not
    // named a theme, which is how a frame starts in the right mode before
    // anyone tells it anything.
    root.classList.toggle("dark", DARK_MODES.has(mode));

    // Guessing is not enough for a demo that is already running: it would sit
    // there in the old theme, a white window inside a black page. The embed
    // listens for this message and checks that it came from its own parent on
    // its own origin, so telling it exactly which theme costs one post. The
    // capture-phase listener catches frames the observer promotes later, which
    // would otherwise only ever see the class. S-05 still owns persistence,
    // the URL parameter, the full picker, and the cross-fade.
    const tell = (frame: HTMLIFrameElement) =>
      frame.contentWindow?.postMessage(
        { type: "gpui-ai-theme", theme: mode },
        window.location.origin,
      );
    for (const frame of document.querySelectorAll("iframe")) tell(frame);
    const onLoad = (event: Event) => {
      if (event.target instanceof HTMLIFrameElement) tell(event.target);
    };
    document.addEventListener("load", onLoad, true);
    return () => document.removeEventListener("load", onLoad, true);
  }, [mode]);

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

      <Masthead mode={mode} onMode={setMode} drawerOpen={drawerOpen} onDrawer={setDrawerOpen} />

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
  mode,
  onMode,
  drawerOpen,
  onDrawer,
}: {
  readonly mode: Mode;
  readonly onMode: (mode: Mode) => void;
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
          <a href={href("/themes/")}>Themes</a>
        </nav>
        <ModeSwitch mode={mode} onMode={onMode} />
      </div>
    </header>
  );
}

/**
 * Light, dark, and contrast, as buttons rather than a menu.
 *
 * Three toggle buttons in a labelled group: reachable by Tab, operable by Enter
 * and Space with no key handling of its own, and each states whether it is the
 * current mode. A styled `<div>` with a click handler would look identical and
 * be unusable without a mouse.
 */
function ModeSwitch({
  mode,
  onMode,
}: {
  readonly mode: Mode;
  readonly onMode: (mode: Mode) => void;
}) {
  return (
    <div className="mode-switch" role="group" aria-label="Theme">
      {MODES.map((candidate) => (
        <button
          key={candidate.id}
          type="button"
          data-theme-choice={candidate.id}
          aria-pressed={mode === candidate.id}
          onClick={() => onMode(candidate.id)}
        >
          {candidate.label}
        </button>
      ))}
    </div>
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
 * the two in agreement; S-12 replaces the substring match with a real index
 * over events and prose and adds the `/` shortcut.
 */
function ComponentNav({ route, idPrefix }: { readonly route: Route; readonly idPrefix: string }) {
  const [query, setQuery] = useState("");
  const searchId = `${idPrefix}-component-search`;
  const needle = query.trim().toLowerCase();

  const groups = useMemo(() => {
    const matches = (component: Component) =>
      !needle ||
      `${component.title} ${component.category} ${component.api}`.toLowerCase().includes(needle);
    return componentsByCategory()
      .map(([category, entries]) => [category, entries.filter(matches)] as const)
      .filter(([, entries]) => entries.length > 0);
  }, [needle]);

  const shown = groups.reduce((total, [, entries]) => total + entries.length, 0);

  return (
    <div className="component-nav">
      <div className="nav-search">
        <label htmlFor={searchId}>Find a component</label>
        <input
          id={searchId}
          type="search"
          value={query}
          placeholder="chat, table, approval…"
          onChange={(event) => setQuery(event.target.value)}
        />
        <output htmlFor={searchId} aria-live="polite">{`${shown} shown`}</output>
      </div>
      <nav aria-label="All components">
        {groups.map(([category, entries]) => (
          <section key={category}>
            <h3>{category}</h3>
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
