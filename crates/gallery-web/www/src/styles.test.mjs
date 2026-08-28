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

test('the embed pins its colour scheme before its first paint', async () => {
  const embed = await readFile(new URL('../embed.html', import.meta.url), 'utf8');

  // A transparent iframe whose used colour scheme differs from its embedder's
  // is composited opaque — solid white over a dark host — and the module that
  // would correct it arrives whole network round-trips after the first paint.
  // So the head carries an inline pin, ahead of the module script.
  const pin = embed.indexOf('style.colorScheme');
  const module = embed.indexOf('type="module"');
  assert.ok(pin !== -1, 'the head must set an inline colour scheme');
  assert.ok(module !== -1 && pin < module, 'the pin must come before the module script');
  // It answers the way the module later does: address first, then the host's
  // own mark, then the viewer's preference.
  for (const source of ["get('theme')", "classList.contains('dark')", 'prefers-color-scheme']) {
    assert.ok(embed.includes(source), `the pin must consult ${source}`);
  }
});
