export function parseEmbedOptions(search) {
  const params = new URLSearchParams(search);
  const story = params.get('story') || undefined;
  const requestedTheme = params.get('theme');
  const theme = ['light', 'dark', 'contrast'].includes(requestedTheme)
    ? requestedTheme
    : undefined;

  return { story, theme };
}

export function parseThemeMessage(data) {
  if (data?.type !== 'mighty-gpui-theme') return undefined;
  return ['light', 'dark', 'contrast'].includes(data.theme) ? data.theme : undefined;
}

export function themeMessage(theme) {
  return { type: 'mighty-gpui-theme', theme };
}

export function parseThemeEvent(event, parent, origin) {
  if (event.source !== parent || event.origin !== origin) return undefined;
  return parseThemeMessage(event.data);
}
