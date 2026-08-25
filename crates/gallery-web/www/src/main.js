import './styles.css';
import { hasDrawn } from './canvas.js';
import {
  parseEmbedOptions,
  parseStatusMessage,
  parseThemeEvent,
  statusMessage,
  themeMessage,
} from './query.js';
import { pinScaleFactor } from './scale.js';

/**
 * Whether the page this example is embedded in is showing a dark theme.
 *
 * Returns undefined when there is no host to ask, or when it is on another
 * origin. The host marks its own document with a `dark` class from the theme
 * registry, which knows every theme's mode — this document knows only the
 * three names below.
 */
function hostIsDark() {
  try {
    if (window.parent === window) return undefined;
    return window.parent.document.documentElement.classList.contains('dark');
  } catch {
    return undefined;
  }
}

function preferredTheme(explicit) {
  if (explicit !== undefined) return explicit;
  const host = hostIsDark();
  if (host !== undefined) return host ? 'dark' : 'light';
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

function showFallback(error, expected = false) {
  const report = expected ? console.info : console.error;
  report('GPUI example fallback:', error);
  // Left on the document as well as posted, for the same reason `ready` is: a
  // host that had not finished attaching its listener would otherwise go on
  // saying "Starting" over an example that has already given up.
  document.body.dataset.failed = '';
  const fallback = document.getElementById('fallback');
  if (fallback) {
    fallback.hidden = false;
    const detail = fallback.querySelector('[data-error]');
    if (detail) detail.textContent = error instanceof Error ? error.message : String(error);
  }
}

// Only the basic presets have a mode this document knows by name. Every other
// theme's mode lives in the registry, which the host can see and this cannot.
const HOST_MODES = Object.freeze({ light: false, dark: true, contrast: true });

/**
 * Which mode to put this document's own chrome in.
 *
 * The host comes before the viewer's preference, and it has to: this sets
 * `color-scheme`, and a transparent iframe whose used colour scheme differs
 * from its embedder's is composited *opaque* — white on a light scheme, near
 * black on a dark one. So a site on a dark theme, viewed on a machine that
 * prefers light, would paint a solid white rectangle over the demo window for
 * the whole load. Matching the host keeps the frame transparent, which is what
 * lets the window behind it show through until there are pixels.
 *
 * The theme's own name is consulted first because it is the more specific
 * answer, and last comes the viewer's preference, for a document with no host
 * to ask — popped out into a tab of its own.
 */
function hostPrefersDark(theme) {
  const known = HOST_MODES[theme];
  if (known !== undefined) return known;
  const host = hostIsDark();
  if (host !== undefined) return host;
  return window.matchMedia('(prefers-color-scheme: dark)').matches;
}

function watchTheme(initialTheme) {
  let theme;
  let wasm;
  let syncScheduled = false;
  const sync = () => {
    if (!wasm || syncScheduled) return;
    syncScheduled = true;
    const attempt = () => {
      let applied = false;
      try {
        applied = wasm.set_gallery_theme(theme);
      } catch (error) {
        // The gallery is the authority on theme names; a name it rejects is
        // host input, not a crash.
        console.warn('gpui-ai: ignoring unknown theme', theme, error);
        syncScheduled = false;
        return;
      }
      if (applied) {
        syncScheduled = false;
      } else {
        window.requestAnimationFrame(attempt);
      }
    };
    window.requestAnimationFrame(attempt);
  };
  const apply = (next) => {
    if (next === theme) return;
    theme = next;
    document.documentElement.classList.toggle('dark', hostPrefersDark(theme));
    document.documentElement.classList.toggle('contrast', theme === 'contrast');
    document.documentElement.dataset.theme = theme;
    sync();
  };

  window.addEventListener('message', (event) => {
    const next = parseThemeEvent(event, window.parent, window.location.origin);
    if (next) apply(next);
  });
  apply(initialTheme);
  return {
    current: () => theme,
    connect: (runningWasm) => {
      wasm = runningWasm;
      sync();
    },
  };
}

// Upstream's WASM asset source fetches `<endpoint>/assets/icons/<name>.svg`
// through reqwest, which rejects relative URLs, so resolve the page's own base
// against the document. That keeps the icons on this origin wherever the host
// is mounted — a dev server, the built gallery, or a subpath on Pages.
function assetEndpoint() {
  const configured = document.body.dataset.assetBase;
  if (!configured) return undefined;
  return new URL(configured, window.location.href).href.replace(/\/+$/, '');
}

/** Tells the host how this example is doing, when there is a host to tell. */
function announce(story, state) {
  if (window.parent === window) return;
  window.parent.postMessage(statusMessage(story, state), window.location.origin);
}

/** Longest the canvas is kept hidden waiting for a frame that may never come. */
const FIRST_FRAME_TIMEOUT_MS = 8000;

/**
 * Shows the canvas once GPUI has drawn into it, and not before.
 *
 * `run()` resolves long before there are pixels — it returns as soon as the
 * platform has spawned its graphics init — so it is no signal that anything is
 * on screen.
 *
 * The timeout is deliberate. If upstream ever stops sizing the canvas this
 * way, a demo nobody can see is a worse failure than a moment of black.
 */
function revealWhenDrawn(story) {
  const startedAt = performance.now();

  const show = () => {
    document.body.dataset.ready = '';
    announce(story, 'ready');
  };

  const check = () => {
    const canvas = document.querySelector('body > canvas');
    if ((canvas && hasDrawn(canvas)) || performance.now() - startedAt > FIRST_FRAME_TIMEOUT_MS) {
      show();
      return;
    }
    window.requestAnimationFrame(check);
  };

  window.requestAnimationFrame(check);
}

async function initEmbed() {
  pinScaleFactor();
  const options = parseEmbedOptions(window.location.search);
  const theme = preferredTheme(options.theme);
  const themeChannel = watchTheme(theme);

  // Either outcome is announced. A host that is only ever told about success
  // leaves its window saying "Starting" over an example that has already
  // given up and drawn the reason why.
  if (!navigator.gpu) {
    showFallback(new Error('This live example requires a browser with WebGPU support.'), true);
    announce(options.story, 'failed');
    return;
  }

  try {
    const wasm = await import('./wasm/gallery_web.js');
    await wasm.default();
    wasm.validate_story(options.story);
    await wasm.run(options.story, themeChannel.current(), assetEndpoint());
    themeChannel.connect(wasm);
    window.gpuiAi = Object.freeze({ currentTheme: () => wasm.gallery_theme() });
    // Not "ready" yet: `run` returns before the first frame. The host keeps
    // saying "Starting" in the demo window's title bar until there are pixels.
    revealWhenDrawn(options.story);
  } catch (error) {
    showFallback(error, String(error).startsWith('unknown story:'));
    announce(options.story, 'failed');
  }
}

function initIndex() {
  const toggle = document.querySelector('[data-theme-toggle]');
  let dark = window.matchMedia('(prefers-color-scheme: dark)').matches;

  const apply = () => {
    document.documentElement.classList.toggle('dark', dark);
    toggle?.setAttribute('aria-pressed', String(dark));
    toggle?.replaceChildren(dark ? 'Use light theme' : 'Use dark theme');
    document.querySelectorAll('iframe').forEach((frame) => {
      frame.contentWindow?.postMessage(themeMessage(dark ? 'dark' : 'light'), window.location.origin);
    });
  };

  toggle?.addEventListener('click', () => {
    dark = !dark;
    apply();
  });
  apply();
  watchExamples();
}

/**
 * Says which examples on this page are still starting.
 *
 * The embed used to paint a card of its own while it loaded, and this page
 * relied on it. It does not any more — a card in the middle of a frame the
 * site draws a window around was the thing worth removing — so this page has
 * to say it, the same way the site's demo window does.
 */
function watchExamples() {
  const frames = [...document.querySelectorAll('.demo iframe')];
  for (const frame of frames) {
    const heading = frame.closest('.demo')?.querySelector('h2');
    if (!heading) continue;
    const hint = document.createElement('span');
    hint.className = 'demo-status';
    hint.setAttribute('role', 'status');
    hint.textContent = 'Starting';
    heading.append(hint);
  }

  window.addEventListener('message', (event) => {
    if (event.origin !== window.location.origin) return;
    if (!parseStatusMessage(event.data)) return;
    const frame = frames.find((candidate) => candidate.contentWindow === event.source);
    frame?.closest('.demo')?.querySelector('.demo-status')?.remove();
  });
}

if (document.body.dataset.page === 'index') {
  initIndex();
} else {
  initEmbed();
}
