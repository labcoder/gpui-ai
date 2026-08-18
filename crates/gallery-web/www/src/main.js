import './styles.css';
import { parseEmbedOptions } from './query.js';

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

function watchTheme(wasm, initialDark) {
  let dark = initialDark;
  const apply = (next) => {
    if (next === dark) return;
    dark = next;
    document.documentElement.classList.toggle('dark', dark);
    wasm.set_theme(dark);
  };

  window.addEventListener('message', (event) => {
    if (event.data?.type === 'mighty-gpui-theme' && typeof event.data.dark === 'boolean') {
      apply(event.data.dark);
    }
  });
}

async function initEmbed() {
  const options = parseEmbedOptions(window.location.search);
  const theme = preferredTheme(options.theme);
  const dark = theme !== 'light';
  document.documentElement.classList.toggle('dark', dark);

  if (!navigator.gpu) {
    showFallback(new Error('This live example requires a browser with WebGPU support.'));
    return;
  }

  try {
    const wasm = await import('./wasm/gallery_web.js');
    await wasm.default();
    wasm.validate_story(options.story);
    watchTheme(wasm, dark);
    await wasm.run(options.story, theme, document.body.dataset.assetBase || undefined);
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
      frame.contentWindow?.postMessage({ type: 'mighty-gpui-theme', dark }, '*');
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
