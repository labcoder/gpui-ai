import './styles.css';
import { parseEmbedOptions, parseThemeEvent, themeMessage } from './query.js';

function preferredTheme(explicit) {
  if (explicit !== undefined) return explicit;
  try {
    if (window.parent !== window) {
      return window.parent.document.documentElement.classList.contains('dark') ? 'dark' : 'light';
    }
  } catch {
    // Cross-origin embeds fall through to the viewer preference.
  }
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

function showFallback(error, expected = false) {
  const report = expected ? console.info : console.error;
  report('GPUI example fallback:', error);
  document.getElementById('loading')?.remove();
  const fallback = document.getElementById('fallback');
  if (fallback) {
    fallback.hidden = false;
    const detail = fallback.querySelector('[data-error]');
    if (detail) detail.textContent = error instanceof Error ? error.message : String(error);
  }
}

function watchTheme(initialTheme) {
  let theme;
  let wasm;
  let syncScheduled = false;
  const sync = () => {
    if (!wasm || syncScheduled) return;
    syncScheduled = true;
    const attempt = () => {
      if (wasm.set_gallery_theme(theme)) {
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
    document.documentElement.classList.toggle('dark', theme !== 'light');
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

async function initEmbed() {
  const options = parseEmbedOptions(window.location.search);
  const theme = preferredTheme(options.theme);
  const themeChannel = watchTheme(theme);

  if (!navigator.gpu) {
    showFallback(new Error('This live example requires a browser with WebGPU support.'), true);
    return;
  }

  try {
    const wasm = await import('./wasm/gallery_web.js');
    await wasm.default();
    wasm.validate_story(options.story);
    await wasm.run(options.story, themeChannel.current(), document.body.dataset.assetBase || undefined);
    themeChannel.connect(wasm);
    window.mightyGpui = Object.freeze({ currentTheme: () => wasm.gallery_theme() });
    document.getElementById('loading')?.remove();
  } catch (error) {
    showFallback(error, String(error).startsWith('unknown story:'));
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
}

if (document.body.dataset.page === 'index') {
  initIndex();
} else {
  initEmbed();
}
