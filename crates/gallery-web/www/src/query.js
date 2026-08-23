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
