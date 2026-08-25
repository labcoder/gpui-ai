import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

test('the hidden fallback layer stays out of layout and accessibility', async () => {
  const styles = await readFile(new URL('./styles.css', import.meta.url), 'utf8');

  assert.match(styles, /\[hidden\]\s*\{[^}]*display:\s*none\s*!important/s);
});

test('an embedded example paints no background of its own', async () => {
  const styles = await readFile(new URL('./styles.css', import.meta.url), 'utf8');

  // The demo window on the site has already painted the theme's surface behind
  // this frame. A background here means a reader watches that surface turn
  // into this page's own colour and then into the canvas — the flash the
  // window exists to avoid.
  assert.doesNotMatch(
    styles,
    /html,\s*\n?body\s*\{[^}]*background:/s,
    'html/body must not paint a background: it is what the embed shows through',
  );
  assert.match(
    styles,
    /body:not\(\[data-page='embed'\]\)\s*\{\s*background:\s*var\(--page\)/,
    'the standalone pages still need one',
  );
});

test('nothing is left of the loading card the window replaced', async () => {
  const [styles, embed] = await Promise.all([
    readFile(new URL('./styles.css', import.meta.url), 'utf8'),
    readFile(new URL('../embed.html', import.meta.url), 'utf8'),
  ]);

  assert.doesNotMatch(embed, /id="loading"/, 'the embed still has a loading layer');
  assert.doesNotMatch(embed, /Loading GPUI example/, 'the embed still announces itself');
  for (const rule of ['.loading', '.loading-card', '.pulse']) {
    assert.ok(!styles.includes(`${rule} `) && !styles.includes(`${rule},`), `${rule} survives`);
  }
  // The fallback is not the loading card and does not go with it.
  assert.match(embed, /id="fallback"/);
  assert.match(styles, /\.fallback-card\s*\{/);
});
