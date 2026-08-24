import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { after, test } from "node:test";

import { buildSite } from "../scripts/build.mjs";
import catalog from "../generated/catalog.json" with { type: "json" };

// The assertions the S-02 rewrite parked in the order's plan under "S-04 (app
// shell)", restored against the shell that now exists. They were written for a
// static generator and a hand-rolled progressive-enhancement script; the DOM
// they describe is the contract, not the implementation, so the selectors and
// ARIA names carry over and the mechanism does not.
//
// These check the markup the build writes. Whether the drawer *behaves* — focus
// moving into it, the page going inert, Escape putting focus back — is not
// something HTML can show, and lives in the release browser gate.
const { components } = catalog;

let built;
function site() {
  built ??= (async () => {
    const root = await mkdtemp(path.join(tmpdir(), "mighty-shell-"));
    const galleryDir = path.join(root, "gallery-input");
    const outDir = path.join(root, "site-output");
    await mkdir(path.join(galleryDir, "assets"), { recursive: true });
    await Promise.all([
      writeFile(path.join(galleryDir, "index.html"), "gallery index"),
      writeFile(path.join(galleryDir, "embed.html"), "gallery fixture"),
      writeFile(path.join(galleryDir, "assets", "gallery_bg-fixture.wasm"), "wasm"),
    ]);
    await buildSite({ galleryDir, outDir });
    return { root, outDir };
  })();
  return built;
}

after(async () => {
  if (!built) return;
  const { root } = await built;
  await rm(root, { force: true, recursive: true });
});

const ROUTES = ["/", "/components/", "/themes/", `/components/${components[0].slug}/`];

async function page(route) {
  const { outDir } = await site();
  return readFile(path.join(outDir, ...route.split("/").filter(Boolean), "index.html"), "utf8");
}

const count = (html, pattern) => (html.match(pattern) ?? []).length;

test("every page exposes keyboard-operable theme controls", async () => {
  for (const route of ROUTES) {
    const html = await page(route);

    // A labelled group, so the three buttons are announced as one control
    // rather than three unrelated ones.
    assert.match(html, /role="group" aria-label="Theme"/, `${route} has no theme group`);
    assert.equal(
      count(html, /data-theme-choice="/g),
      3,
      `${route} should offer light, dark, and contrast`,
    );
    for (const mode of ["light", "dark", "contrast"]) {
      assert.match(html, new RegExp(`data-theme-choice="${mode}"`), `${route} is missing ${mode}`);
    }

    // Real buttons. A div with a click handler looks the same and cannot be
    // reached by Tab or activated by Enter or Space.
    const controls = html.match(/<[a-z]+[^>]*data-theme-choice="[a-z]+"[^>]*>/g) ?? [];
    assert.equal(controls.length, 3);
    for (const control of controls) {
      assert.match(control, /^<button /, `a theme control is not a button: ${control}`);
      assert.match(control, /type="button"/, `a theme control would submit a form: ${control}`);
      assert.match(control, /aria-pressed="(true|false)"/, `${control} states no pressed state`);
    }
    assert.equal(
      count(html, /aria-pressed="true"/g),
      1,
      `${route} must show exactly one mode as current`,
    );
  }
});

test("the skip link lands on a focusable main", async () => {
  for (const route of ROUTES) {
    const html = await page(route);

    assert.match(html, /class="skip-link" href="#content"/, `${route} has no skip link`);
    // Without tabindex the target is not focusable, so the link scrolls but
    // leaves the keyboard where it was — the next Tab returns to the header.
    assert.match(html, /<main id="content" tabindex="-1"/, `${route} has no focusable main`);
  }
});

test("the drawer is a hidden modal wired to its toggle", async () => {
  for (const route of ROUTES) {
    const html = await page(route);

    assert.match(
      html,
      /data-nav-toggle="" aria-expanded="false" aria-controls="site-nav-panel"/,
      `${route}'s toggle does not describe the panel it opens`,
    );
    assert.match(
      html,
      /id="site-nav-panel"[^>]*role="dialog"[^>]*aria-modal="true"/,
      `${route} has no modal panel`,
    );
    assert.match(html, /id="site-nav-panel"[^>]*hidden/, `${route} ships the drawer open`);
    assert.match(html, /aria-labelledby="site-nav-title"/);
    assert.match(html, /id="site-nav-title"/);
  }
});

test("the drawer backdrop is pointer-only while its named close button stays focusable", async () => {
  const html = await page("/");

  // The backdrop exists to be clicked, not tabbed to. As a button it would be
  // an unnamed control sitting between the visitor and the panel, and it would
  // appear in the accessibility tree as something to act on.
  assert.match(html, /<div class="nav-backdrop" data-nav-close="" aria-hidden="true">/);
  assert.doesNotMatch(html, /<button[^>]*class="nav-backdrop"/);

  const closeButton = /<button[^>]*data-nav-close[^>]*>([^<]*)<\/button>/.exec(html);
  assert.ok(closeButton, "the drawer has no close button");
  assert.equal(closeButton[1], "Close", "the close button needs a name a visitor can say");
  assert.doesNotMatch(closeButton[0], /aria-hidden|tabindex="-1"/, "the way out must be reachable");
});

test("the catalog rail lists every component on every page", async () => {
  for (const route of ROUTES) {
    const html = await page(route);

    assert.match(
      html,
      /class="desktop-rail" aria-label="Component catalog"/,
      `${route} has no rail`,
    );
    // Once in the rail and once in the drawer: the same catalog, two
    // presentations, so a narrow screen loses nothing.
    assert.equal(
      count(html, /class="nav-component-link"/g),
      components.length * 2,
      `${route} does not carry the catalog in both the rail and the drawer`,
    );
    assert.match(html, /<nav aria-label="All components">/);
  }

  // A component page marks itself in both copies; other pages mark nothing.
  const component = await page(`/components/${components[3].slug}/`);
  assert.equal(count(component, /aria-current="page"/g), 2);
  assert.match(
    component,
    new RegExp(`href="/gpui-ai/components/${components[3].slug}/" aria-current="page"`),
  );
  assert.equal(count(await page("/"), /aria-current="page"/g), 0);
});

test("the rail's search is labelled, counted, and distinct from the catalog filter", async () => {
  const html = await page("/components/");

  for (const prefix of ["rail", "drawer"]) {
    assert.match(
      html,
      new RegExp(`<label for="${prefix}-component-search">Find a component</label>`),
      `the ${prefix} search has no label`,
    );
    assert.match(html, new RegExp(`id="${prefix}-component-search"`));
  }

  assert.match(html, new RegExp(`${components.length} shown`));
});

test("no page skips a heading level", async () => {
  for (const route of ROUTES) {
    const html = await page(route);
    const levels = [...html.matchAll(/<h([1-6])[^>]*>/g)].map((match) => Number(match[1]));

    assert.ok(levels.length > 3, `${route} has almost no headings`);
    assert.equal(levels.filter((level) => level === 1).length, 1, `${route} needs exactly one h1`);

    // Someone navigating by heading uses the levels as an outline. A jump from
    // one straight to three reads as a section that failed to load — which is
    // what the rail's category labels did before they had a heading above them.
    for (const [index, level] of levels.entries()) {
      if (index === 0) continue;
      const previous = levels[index - 1];
      assert.ok(
        level <= previous + 1,
        `${route} jumps from h${previous} to h${level} at heading ${index + 1}`,
      );
    }
  }
});

test("no page repeats an id", async () => {
  // Two searches on one page is fine; two elements sharing one id is not — the
  // second `<output for=…>` binds to the wrong input and announces nothing,
  // and a duplicate anchor target sends a fragment link to whichever came
  // first. The rail and the drawer render the same nav twice, so every route
  // is at risk, not only the one that also carries a filter.
  for (const route of ROUTES) {
    const html = await page(route);
    const ids = [...html.matchAll(/\bid="([^"]+)"/g)].map((match) => match[1]);
    const repeated = [...new Set(ids.filter((id, index) => ids.indexOf(id) !== index))];
    assert.deepEqual(repeated, [], `${route} repeats ${repeated.join(", ")}`);
  }
});

test("the pre-render carries no state only a browser could know", async () => {
  // A cheap guard, not a hydration test: whether the two renders agree is
  // something only a browser can answer, and the release gate answers it by
  // failing on the console error React logs for a mismatch. What this catches
  // is the shape of the mistake — a value that came from the machine doing the
  // build leaking into markup every visitor receives.
  for (const route of ROUTES) {
    const html = await page(route);

    assert.doesNotMatch(html, /<html[^>]*data-theme=/, `${route} bakes in a theme`);
    assert.doesNotMatch(html, /class="[^"]*\bdark\b/, `${route} bakes in a mode`);
    // An open drawer and an inert region are both runtime states. Shipping
    // either leaves a visitor without JavaScript on an unusable page.
    assert.doesNotMatch(html, /\binert\b/, `${route} ships part of itself inert`);
  }
});
