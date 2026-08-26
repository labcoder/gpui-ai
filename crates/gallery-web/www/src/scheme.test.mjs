import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import test from 'node:test';
import { NAMED_MODES, schemeFor } from './scheme.js';

// The inline pin in embed.html cannot import scheme.js — running before any
// module loads is its whole job — so it re-states the precedence by hand.
// This suite executes that real inline source, authored and (when built)
// bundled, across the complete input matrix and holds every answer to
// schemeFor's. A drift between the copies paints the first frame in one
// scheme and every later frame in another, which composites a transparent
// iframe opaque over its host.

/** The first inline script in a page's head. */
function inlinePin(html, label) {
  const match = /<script>([\s\S]*?)<\/script>/.exec(html);
  assert.ok(match, `${label} carries no inline script to pin the scheme with`);
  return match[1];
}

/** Runs the pin's source against stubbed browser globals. */
function runPin(source, { search, parent, systemDark }) {
  const classes = new Set();
  const root = {
    classList: {
      toggle(name, on) {
        if (on) classes.add(name);
        else classes.delete(name);
      },
      add(name) {
        classes.add(name);
      },
      contains: (name) => classes.has(name),
    },
    style: {},
  };
  const parentDocument =
    parent === 'throw'
      ? {
          get documentElement() {
            throw new Error('cross-origin');
          },
        }
      : {
          documentElement: {
            classList: { contains: (name) => name === 'dark' && parent === true },
          },
        };
  const window = {
    location: { search },
    matchMedia: () => ({ matches: systemDark }),
  };
  window.parent = parent === 'none' ? window : { document: parentDocument };
  const document = { documentElement: root };
  new Function('window', 'document', 'URLSearchParams', source)(window, document, URLSearchParams);
  return {
    dark: classes.has('dark'),
    contrast: classes.has('contrast'),
    pinned: root.style.colorScheme,
  };
}

const THEMES = [undefined, 'light', 'dark', 'contrast', 'nord-frost'];
const PARENTS = ['none', true, false, 'throw'];
const SYSTEMS = [true, false];

function assertParity(source, label) {
  for (const theme of THEMES) {
    for (const parent of PARENTS) {
      for (const systemDark of SYSTEMS) {
        const search = theme === undefined ? '' : `?theme=${theme}`;
        // 'none' and 'throw' are both "no host to ask" to the decision table.
        const parentIsDark = parent === true ? true : parent === false ? false : undefined;
        const expected = schemeFor(theme, parentIsDark, systemDark);
        const got = runPin(source, { search, parent, systemDark });
        const case_ = `${label}: theme=${theme ?? 'none'} parent=${parent} system=${systemDark}`;
        assert.equal(got.dark, expected.dark, `${case_} — dark diverged`);
        assert.equal(got.contrast, expected.contrast, `${case_} — contrast diverged`);
        assert.equal(
          got.pinned,
          expected.dark ? 'dark' : 'light',
          `${case_} — the inline colour-scheme pin diverged`,
        );
      }
    }
  }
}

test('the authored inline pin answers exactly as scheme.js does', () => {
  const html = readFileSync(new URL('../embed.html', import.meta.url), 'utf8');
  assertParity(inlinePin(html, 'embed.html'), 'authored');
});

test('the built inline pin still answers exactly as scheme.js does', (t) => {
  const built = new URL('../dist/embed.html', import.meta.url);
  if (!existsSync(built)) {
    // The release gate always builds before its checks; this variant guards
    // local runs that happen to have a dist. Absence is not failure.
    t.skip('no built embed present; the release gate covers the built form');
    return;
  }
  assertParity(inlinePin(readFileSync(built, 'utf8'), 'dist/embed.html'), 'built');
});

test('the runtime half resolves through the same table', () => {
  const main = readFileSync(new URL('./main.js', import.meta.url), 'utf8');
  assert.match(
    main,
    /from '\.\/scheme\.js'/,
    'main.js must import scheme.js rather than restating the precedence',
  );
  assert.doesNotMatch(
    main,
    /HOST_MODES/,
    'a second mode table in main.js is the drift this suite exists to prevent',
  );
});

test('the named-mode table matches what the stylesheet can paint', () => {
  // The stylesheet defines exactly :root, :root.dark, and :root.contrast.
  // A name added here without a class there would pin a scheme no rule backs.
  assert.deepEqual(Object.keys(NAMED_MODES).sort(), ['contrast', 'dark', 'light']);
});
