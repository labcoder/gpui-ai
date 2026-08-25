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

/**
 * Whether the address asks for reduced motion, and what it asks for.
 *
 * `motion=reduced` forces it on and `motion=full` forces it off; anything else,
 * including no parameter at all, means "ask the machine". A link can therefore
 * pin either answer, which is what makes the reduced-motion promise something
 * a reader can check rather than something they have to take on trust.
 */
export function parseMotion(value) {
  if (value === 'reduced') return true;
  if (value === 'full') return false;
  return undefined;
}

/**
 * The state the address asks the story to open in.
 *
 * Same shape rule as a theme name, a lowercase slug, because the gallery is
 * the only thing that knows which states a story has, and the host must not
 * gatekeep a list it does not own. An unknown one is simply not applied.
 */
export function parseVariant(value) {
  if (!value) return undefined;
  return THEME_SLUG.test(value) ? value : undefined;
}

export function parseEmbedOptions(search) {
  const params = new URLSearchParams(search);
  const story = params.get('story') || undefined;

  return {
    story,
    theme: normalizeTheme(params.get('theme')),
    motion: parseMotion(params.get('motion')),
    variant: parseVariant(params.get('variant')),
  };
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

/**
 * Told to the host when this example takes the wheel, or gives it back.
 *
 * The window says which of the two is happening, because a frame that quietly
 * swallowed the wheel is the thing that made a reader feel stuck in the first
 * place.
 */
export function wheelMessage(story, captured) {
  return { type: 'gpui-ai-wheel', story: story ?? '', captured: Boolean(captured) };
}

export function parseWheelMessage(data) {
  if (data?.type !== 'gpui-ai-wheel') return undefined;
  if (typeof data.story !== 'string' || typeof data.captured !== 'boolean') return undefined;
  return { story: data.story, captured: data.captured };
}

/**
 * What the story measured, for the page to size its frame from.
 *
 * The catalog's heights were measured at one width, and a story's height is a
 * continuous function of the width it is given, so on anything narrower they
 * are wrong and the story scrolls inside its own canvas. This is the story
 * saying what it actually is.
 */
export function sizeMessage(story, height) {
  return { type: 'gpui-ai-size', story: story ?? '', height: Math.round(height) };
}

export function parseSizeMessage(data) {
  if (data?.type !== 'gpui-ai-size') return undefined;
  if (typeof data.story !== 'string') return undefined;
  // A height of zero is a story that has not laid out, and a wild one is a
  // number this page should not be resizing anything from.
  if (!Number.isFinite(data.height) || data.height <= 0 || data.height > 20_000) return undefined;
  return { story: data.story, height: Math.round(data.height) };
}

/**
 * Which state the story is showing, for the page's Copy link and Pop out.
 *
 * The switcher a story draws is inside its canvas, so a reader can change what
 * they are looking at without the page around it knowing. Without this, a
 * shared link always opened the story where the story opens, which is not
 * where the sender was.
 */
export function variantMessage(story, variant) {
  return { type: 'gpui-ai-variant', story: story ?? '', variant: variant ?? null };
}

export function parseVariantMessage(data) {
  if (data?.type !== 'gpui-ai-variant') return undefined;
  if (typeof data.story !== 'string') return undefined;
  if (data.variant !== null && parseVariant(data.variant) === undefined) return undefined;
  return { story: data.story, variant: data.variant };
}

export function parseStatusMessage(data) {
  if (data?.type !== 'gpui-ai-status') return undefined;
  if (typeof data.story !== 'string' || !STATES.has(data.state)) return undefined;
  return { story: data.story, state: data.state };
}
