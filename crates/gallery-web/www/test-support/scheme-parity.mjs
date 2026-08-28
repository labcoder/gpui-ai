import assert from "node:assert/strict";
import { schemeFor } from "../src/scheme.js";

/** The first inline script in a page's head. */
export function inlinePin(html, label) {
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

export function assertParity(source, label) {
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
