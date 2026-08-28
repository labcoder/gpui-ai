import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { NAMED_MODES } from './scheme.js';

// The inline pin in embed.html cannot import scheme.js — running before any
// module loads is its whole job — so it re-states the precedence by hand.
// This suite executes the authored source; the required post-build artifact
// suite executes the bundled source. Both use the same matrix and compare to
// schemeFor's. A drift between the copies paints the first frame in one
// scheme and every later frame in another, which composites a transparent
// iframe opaque over its host.

import { inlinePin, assertParity } from "../test-support/scheme-parity.mjs";

test('the authored inline pin answers exactly as scheme.js does', () => {
  const html = readFileSync(new URL('../embed.html', import.meta.url), 'utf8');
  assertParity(inlinePin(html, 'embed.html'), 'authored');
});

test('the named-mode table matches what the stylesheet can paint', () => {
  // The stylesheet defines exactly :root, :root.dark, and :root.contrast.
  // A name added here without a class there would pin a scheme no rule backs.
  assert.deepEqual(Object.keys(NAMED_MODES).sort(), ['contrast', 'dark', 'light']);
});
