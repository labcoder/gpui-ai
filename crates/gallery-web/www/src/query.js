// Theme identity lives in the Rust registry, which is generated from the
// `themes/` directory. The host therefore checks only the shape of a name and
// lets the gallery reject anything it does not know, so adding a theme file
// never means editing this list.
const THEME_SLUG = /^[a-z0-9][a-z0-9-]*$/;

function normalizeTheme(value) {
  // `system` is the host's own sentinel for "follow the viewer's preference".
  if (!value || value === 'system') return undefined;
  return THEME_SLUG.test(value) ? value : undefined;
}

export function parseEmbedOptions(search) {
  const params = new URLSearchParams(search);
  const story = params.get('story') || undefined;

  return { story, theme: normalizeTheme(params.get('theme')) };
}

export function parseThemeMessage(data) {
  if (data?.type !== 'gpui-ai-theme') return undefined;
  return normalizeTheme(data.theme);
}

export function themeMessage(theme) {
  return { type: 'gpui-ai-theme', theme };
}

export function parseThemeEvent(event, parent, origin) {
  if (event.source !== parent || event.origin !== origin) return undefined;
  return parseThemeMessage(event.data);
}

/** What an embed can tell its host about itself. */
const STATES = new Set(['ready', 'failed']);

/**
 * Told to the host when this example starts drawing, or gives up.
 *
 * The embed paints no loading card of its own, so the window it sits in is
 * what says the demo is starting — and it needs to be told when to stop.
 * `failed` matters as much as `ready`: an example that will never draw must
 * not leave a window claiming forever that it is about to.
 *
 * The story travels with it because a host may have replaced the frame
 * (Reload does), and a message from the one it threw away must not answer for
 * the one it is waiting on.
 */
export function statusMessage(story, state) {
  return { type: 'gpui-ai-status', story: story ?? '', state };
}

export function parseStatusMessage(data) {
  if (data?.type !== 'gpui-ai-status') return undefined;
  if (typeof data.story !== 'string' || !STATES.has(data.state)) return undefined;
  return { story: data.story, state: data.state };
}
