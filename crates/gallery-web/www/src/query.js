export function parseEmbedOptions(search) {
  const params = new URLSearchParams(search);
  const story = params.get('story') || undefined;
  const requestedTheme = params.get('theme');
  const theme = ['light', 'dark', 'contrast'].includes(requestedTheme)
    ? requestedTheme
    : undefined;

  return { story, theme };
}
