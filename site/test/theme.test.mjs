import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { DEFAULT, SYSTEM, appliedTheme, resolveChoice } from "../app/theme-resolve.mjs";
import themeFile from "../generated/themes.json" with { type: "json" };

const siteRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const themes = themeFile.groups.flatMap((group) => group.themes);

test("a theme in the URL beats a stored one, and a stored one beats nothing", () => {
  // The order is the point: a link that says "look at this in Ember Dusk" has
  // to win for that visit, without quietly replacing what the visitor chose
  // for themselves.
  assert.equal(resolveChoice({ param: "ember-dusk", stored: "solstice" }), "ember-dusk");
  assert.equal(resolveChoice({ param: undefined, stored: "solstice" }), "solstice");
  assert.equal(resolveChoice({}), DEFAULT);
  assert.equal(resolveChoice(), DEFAULT);
});

test("the default is a theme the registry actually ships", () => {
  // The site opens on this name, and themes are files under `themes/`, so a
  // rename could take it away without touching any code that mentions it.
  // `app/theme.ts` falls back to following the system if that ever happens;
  // this is how we find out rather than shipping the fallback unnoticed.
  assert.ok(
    themes.some((theme) => theme.slug === DEFAULT),
    `the registry has no ${DEFAULT}; it ships ${themes.length} themes`,
  );
});

test("a caller that can see the registry can override what nothing resolves to", () => {
  assert.equal(resolveChoice({ fallback: SYSTEM }), SYSTEM);
  assert.equal(resolveChoice({ fallback: "solstice" }), "solstice");
  // A fallback the rule cannot accept is not applied either: following the
  // system is the last thing left that is always true.
  assert.equal(resolveChoice({ fallback: "NOT A SLUG" }), SYSTEM);
  assert.equal(resolveChoice({ fallback: undefined }), DEFAULT);
  // And it never outranks a real choice.
  assert.equal(resolveChoice({ stored: "graphite", fallback: SYSTEM }), "graphite");
});

test("a name the registry could never contain is ignored, not applied", () => {
  for (const rubbish of ["", "  ", "-leading", "Upper", "has space", "semi;colon", null, 42, {}]) {
    assert.equal(
      resolveChoice({ param: rubbish, stored: rubbish }),
      DEFAULT,
      `${JSON.stringify(rubbish)} must not survive`,
    );
  }
  // Shape only, deliberately: theme identity lives in the Rust registry, and a
  // slug this does not recognise matches no [data-theme] rule, so :root keeps
  // the page on the default. Adding a theme file must never mean editing a
  // list in the site.
  assert.equal(resolveChoice({ param: "a-theme-that-does-not-exist" }), "a-theme-that-does-not-exist");
});

test("system resolves to whichever palette the machine asks for", () => {
  assert.equal(appliedTheme(SYSTEM, true), "dark");
  assert.equal(appliedTheme(SYSTEM, false), "light");
  // Anything explicit ignores the machine entirely.
  assert.equal(appliedTheme("solstice", true), "solstice");
  assert.equal(appliedTheme("tokyo-night", false), "tokyo-night");
});

test("every theme the registry ships is a name this rule accepts", () => {
  for (const theme of themes) {
    assert.equal(resolveChoice({ param: theme.slug }), theme.slug, `${theme.slug} was rejected`);
    assert.equal(appliedTheme(theme.slug, false), theme.slug);
  }
  assert.ok(themes.length > 40, `only ${themes.length} themes were found`);
});

/**
 * Runs the inline script from the document with a fake browser around it.
 *
 * The script cannot import the module above, so it restates the rule. This
 * executes the real text out of `index.html` and returns what it decided, so
 * the two are compared rather than assumed to agree.
 */
function runInlineScript(source, { search = "", stored = undefined, prefersDark = false, storageThrows = false }) {
  const root = { dataset: {} };
  const scope = {
    document: { documentElement: root },
    window: {
      location: { search },
      localStorage: {
        getItem(key) {
          if (storageThrows) throw new Error("storage is unavailable");
          return key === "gpui-ai:theme" ? (stored ?? null) : null;
        },
      },
      matchMedia: (query) => ({ matches: query.includes("dark") && prefersDark }),
    },
    URLSearchParams,
  };
  // eslint-disable-next-line no-new-func -- the input is this repository's own index.html
  new Function(...Object.keys(scope), source)(...Object.values(scope));
  return root.dataset.theme;
}

test("the inline script agrees with the rule, for every combination", async () => {
  const html = await readFile(path.join(siteRoot, "index.html"), "utf8");
  const source = /<script>([\s\S]*?)<\/script>/.exec(html)?.[1];
  assert.ok(source, "index.html has no inline script");
  assert.match(source, /prefers-color-scheme/, "the script never asks about the system");
  assert.match(source, /gpui-ai:theme/, "the script never reads the stored choice");

  // `system` appears in both lists because it is now a choice like any other:
  // the site opens on a named theme, so a stored or linked `system` has to
  // send the script back to the machine rather than to the default.
  const params = [undefined, "ember-dusk", "tokyo-night", SYSTEM, "not a slug", ""];
  const stores = [undefined, "solstice", "graphite", SYSTEM, "ALSO NOT A SLUG"];

  for (const param of params) {
    for (const stored of stores) {
      for (const prefersDark of [false, true]) {
        const search = param === undefined ? "" : `?theme=${encodeURIComponent(param)}`;
        const expected = appliedTheme(resolveChoice({ param, stored }), prefersDark);
        assert.equal(
          runInlineScript(source, { search, stored, prefersDark }),
          expected,
          `param=${param} stored=${stored} prefersDark=${prefersDark}`,
        );
      }
    }
  }
});

test("the inline script survives a browser that refuses it storage", async () => {
  // A private window throws on localStorage. Throwing here would leave the
  // document with no theme at all and every later script on the page unrun.
  const html = await readFile(path.join(siteRoot, "index.html"), "utf8");
  const source = /<script>([\s\S]*?)<\/script>/.exec(html)?.[1] ?? "";

  assert.equal(runInlineScript(source, { storageThrows: true }), DEFAULT);
  assert.equal(runInlineScript(source, { storageThrows: true, prefersDark: true }), DEFAULT);
  assert.equal(
    runInlineScript(source, { storageThrows: true, search: "?theme=graphite" }),
    "graphite",
  );
});

test("the inline script is classic and inline, or it cannot beat first paint", async () => {
  const html = await readFile(path.join(siteRoot, "index.html"), "utf8");
  const head = html.slice(0, html.indexOf("</head>"));

  // A module script is deferred and the stylesheet paints before it, so the
  // page would show the default palette and then swap. It also has to come
  // before anything that paints.
  assert.match(head, /<script>\s*\(function/, "the theme script is not inline in the head");
  assert.doesNotMatch(head, /<script type="module"/, "a deferred script cannot beat first paint");
});
