import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

test('hidden loading and fallback layers stay out of layout and accessibility', async () => {
  const styles = await readFile(new URL('./styles.css', import.meta.url), 'utf8');

  assert.match(styles, /\[hidden\]\s*\{[^}]*display:\s*none\s*!important/s);
});
