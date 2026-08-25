import { useSyncExternalStore } from "react";
import { themes } from "./data";
import { DEFAULT, SYSTEM, appliedTheme, resolveChoice } from "./theme-resolve.mjs";

export { DEFAULT, SYSTEM };

/** Where a deliberate choice is kept between visits. */
const STORAGE_KEY = "gpui-ai:theme";

/** The query parameter a shared link carries. */
export const THEME_PARAM = "theme";

/** Slugs the embedded gallery should treat as dark. */
const DARK_SLUGS: ReadonlySet<string> = new Set(
  themes.filter((theme) => theme.mode === "dark").map((theme) => theme.slug),
);

/**
 * The default, but only while the registry still ships it.
 *
 * Themes are files under `themes/`, so the default is a name that a future
 * commit could rename or delete without touching this module. Rather than open
 * on a theme that resolves to nothing, fall back to following the system.
 * `site/test/theme.test.mjs` asserts the registry does ship it, so this is a
 * net rather than an expectation.
 */
const FALLBACK: string = new Set(themes.map((theme) => theme.slug)).has(DEFAULT) ? DEFAULT : SYSTEM;

/**
 * Everything the shell renders before it knows anything about the browser.
 *
 * The pre-render produces this, and so does the browser's first render, which
 * is what lets React hydrate without discarding the markup. The inline script
 * in `site/index.html` has already painted the real palette by then — it works
 * on the document element, which is outside React's tree entirely.
 */
const SERVER_SNAPSHOT = `${FALLBACK} ${appliedTheme(FALLBACK, false)}`;

// The snapshot carries the choice *and* what it currently resolves to, because
// React re-renders on a changed snapshot and nothing else. The operating system
// flipping does not change the choice — it is still `system` — so a snapshot of
// the choice alone would leave the page painted for the palette that was.
const listeners = new Set<() => void>();
let snapshot: string = SERVER_SNAPSHOT;

function readStored(): string | undefined {
  try {
    return window.localStorage.getItem(STORAGE_KEY) ?? undefined;
  } catch {
    // Private windows and blocked storage both throw. Not being able to
    // remember a preference is not a reason to fail to show a page.
    return undefined;
  }
}

function readParam(): string | undefined {
  return new URLSearchParams(window.location.search).get(THEME_PARAM) ?? undefined;
}

function prefersDark(): boolean {
  return window.matchMedia("(prefers-color-scheme: dark)").matches;
}

/**
 * What was chosen in this tab, which outranks storage and the URL alike.
 *
 * Storage can be readable and unwritable at the same time — a full quota, a
 * browser that permits reads and denies writes — and then `setChoice` records
 * nothing while the previous value stays readable. Without this the very next
 * read would hand back the value the write failed to replace and quietly undo
 * the visitor's choice.
 */
let chosen: string | undefined;

function compute(): string {
  const choice = resolveChoice({
    param: chosen ?? readParam(),
    stored: readStored(),
    fallback: FALLBACK,
  });
  return `${choice} ${appliedTheme(choice, prefersDark())}`;
}

function publish(): void {
  const next = compute();
  if (next === snapshot) return;
  snapshot = next;
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  if (listeners.size === 1) {
    // Another tab changing the preference, and the operating system flipping
    // its own, both have to reach a page that is already open.
    window.addEventListener("storage", publish);
    window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", publish);
  }
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0) {
      window.removeEventListener("storage", publish);
      window.matchMedia("(prefers-color-scheme: dark)").removeEventListener("change", publish);
    }
  };
}

function getSnapshot(): string {
  const next = compute();
  if (next !== snapshot) snapshot = next;
  return snapshot;
}

function getServerSnapshot(): string {
  return SERVER_SNAPSHOT;
}

/**
 * The theme the visitor has chosen, and how to change it.
 *
 * `useSyncExternalStore` is here for its server snapshot: React renders the
 * same neutral value on both sides and swaps in the real one after hydration,
 * rather than producing markup the browser disagrees with and throwing the
 * whole pre-render away.
 */
export function useTheme(): {
  readonly choice: string;
  readonly applied: string;
  readonly isDark: boolean;
  readonly setChoice: (choice: string) => void;
} {
  const [choice = SYSTEM, applied = "light"] = useSyncExternalStore(
    subscribe,
    getSnapshot,
    getServerSnapshot,
  ).split(" ");

  return {
    choice,
    applied,
    isDark: DARK_SLUGS.has(applied),
    setChoice,
  };
}

/**
 * Records a choice everywhere it has to be recorded.
 *
 * Storage so it survives a reload, the URL so the page can be linked as it
 * looks, and the store so every control updates at once. The URL is rewritten
 * rather than pushed: choosing a theme is not somewhere to go Back to.
 */
export function setChoice(choice: string): void {
  chosen = choice;

  try {
    // Every choice is stored, `system` included. It used to be the state that
    // meant "nothing chosen", so forgetting it was the same as recording it;
    // now that the site opens on a named theme, a visitor who asks to follow
    // their machine has said something, and it has to survive a reload.
    window.localStorage.setItem(STORAGE_KEY, choice);
  } catch {
    // See readStored. `chosen` above is what keeps the choice applied for the
    // rest of this visit when the write is refused.
  }

  try {
    // Every choice, the default included. A reader who picks a theme and
    // copies the address expects the link to show what they were looking at,
    // and the plain address cannot do that: it means "the default", which is
    // whatever the person opening it has already chosen for themselves.
    const url = new URL(window.location.href);
    url.searchParams.set(THEME_PARAM, choice);
    window.history.replaceState(null, "", url);
  } catch {
    // A sandboxed frame refuses history writes. Between this and storage,
    // both places a choice is normally recorded can be closed off, and
    // `chosen` is what still makes the page obey it.
  }

  publish();
}

/**
 * Paints the document, and tells any demo already running.
 *
 * The attribute goes on the document element because `themes.css` keys on it
 * there and `html { font-size: var(--ai-font-size) }` reads a token at that
 * level. React does not own that element, which is exactly why the inline
 * script can set it first without anything to disagree with.
 */
export function paint(applied: string, isDark: boolean): void {
  const root = document.documentElement;
  const changed = root.dataset.theme !== applied;

  if (changed) {
    // A cross-fade, added only for the moment the colours change, so the
    // transition never catches an ordinary hover. Reduced motion collapses it
    // through the media query in site.css rather than a second code path.
    root.classList.add("theming");
    window.setTimeout(() => root.classList.remove("theming"), 240);
    root.dataset.theme = applied;
  }

  // Always, even when the attribute already matched: the inline script sets
  // the attribute but not this class — it would need the registry's list of
  // dark themes to do so — and the embed reads the class to guess a theme
  // nobody has named for it yet.
  root.classList.toggle("dark", isDark);
}

/** Tells one frame which theme to draw, in the shape the embed listens for. */
export function tellFrame(frame: HTMLIFrameElement, theme: string): void {
  frame.contentWindow?.postMessage({ type: "gpui-ai-theme", theme }, window.location.origin);
}
