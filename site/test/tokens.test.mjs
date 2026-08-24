import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { authoredStylesheets, declarations, lintSite, lintStylesheet } from "../scripts/token-lint.mjs";

// The lint is only worth having if it catches what it claims to. These cases
// exist so it cannot quietly decay into a function that returns an empty array:
// every rule is shown failing on a value that breaks it and passing on one that
// does not, and the real stylesheet is checked with the same code.
const siteRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

const wrap = (declaration) => `.probe {\n  ${declaration};\n}\n`;

const CAUGHT = [
  ["color: #0a0a0a", "literal-colour"],
  ["color: #fff", "literal-colour"],
  ["background: #0a0a0aff", "literal-colour"],
  ["background: rgb(10 10 10)", "literal-colour"],
  ["background: rgba(0, 0, 0, 0.4)", "literal-colour"],
  ["background: oklch(0.7 0.1 200)", "literal-colour"],
  ["background: red", "literal-colour"],
  ["outline-color: rebeccapurple", "literal-colour"],
  // Hidden inside a longer value, which is how it usually gets in.
  ["border: 1px solid #e5e5e5", "literal-colour"],
  ["box-shadow: 0 1px 2px rgba(0, 0, 0, 0.1)", "literal-colour"],
  ["background-image: linear-gradient(to right, #fff, #000)", "literal-colour"],
  // A var() fallback is a hard-coded colour like any other.
  ["background: var(--ai-surface, #ffffff)", "literal-colour"],
  // Defining a custom property does not launder the value.
  ["--brand: #ff0000", "literal-colour"],
  ["border-radius: 4px", "literal-radius"],
  ["border-top-left-radius: 0.5rem", "literal-radius"],
  ['font-family: "Inter", sans-serif', "literal-font-family"],
  // A stack smuggled in under a name of its own and read back through var().
  ['--brand-font: "Inter", sans-serif', "literal-font-family"],
];

const ALLOWED = [
  "background: var(--ai-background)",
  "color: var(--ai-foreground)",
  "border: 1px solid var(--ai-border)",
  "box-shadow: var(--ai-shadow)",
  "color: inherit",
  "text-decoration-color: currentColor",
  "background: transparent",
  "list-style: none",
  "border-radius: var(--ai-radius)",
  "border-radius: var(--ai-radius-lg)",
  // Shapes, not corner styling.
  "border-radius: 50%",
  "border-radius: 0",
  "font-family: var(--ai-font-sans)",
  "font-family: var(--ai-font-mono)",
  "font-family: var(--site-font-serif)",
  '--site-font-serif: "IBM Plex Serif", ui-serif, Georgia, serif',
  "font: inherit",
  // Colour words must not be found in things that are not colours.
  "color-scheme: light dark",
  "transition: top 120ms ease",
  "-webkit-font-smoothing: antialiased",
  "text-wrap: balance",
  "grid-template-columns: minmax(0, 1fr) 15rem",
  "font-size: 0.875rem",
  "--space-4: 1rem",
  "--demo-width: 900px",
  // Quoted text is content or a family name, never an authored colour.
  'content: "#ff0000"',
];

test("every rule catches the value it exists for", () => {
  for (const [declaration, rule] of CAUGHT) {
    const findings = lintStylesheet(wrap(declaration));
    assert.ok(
      findings.some((finding) => finding.rule === rule),
      `${declaration} should break ${rule}, got ${JSON.stringify(findings)}`,
    );
  }
});

test("no rule fires on a value that reads its token", () => {
  for (const declaration of ALLOWED) {
    assert.deepEqual(
      lintStylesheet(wrap(declaration)),
      [],
      `${declaration} is correct and must not be reported`,
    );
  }
});

test("a violation is reported at the line it is written on", () => {
  const css = [
    ".first {",
    "  color: var(--ai-foreground);",
    "}",
    "",
    ".second {",
    "  color: #123456;",
    "}",
  ].join("\n");

  const [finding, ...rest] = lintStylesheet(css, "site.css");
  assert.deepEqual(rest, [], "only the second rule breaks");
  assert.equal(finding.line, 6);
  assert.equal(finding.file, "site.css");
  assert.match(finding.message, /#123456/);
});

test("selectors, at-rules, and comments are not mistaken for declarations", () => {
  // `a:hover` and `@media (min-width: 60rem)` both contain a colon, and a
  // commented-out colour is not shipped. A parser that reported any of them
  // would make the lint noisy enough to be turned off.
  const css = [
    "/* color: #ff0000 was here */",
    "@media (min-width: 60rem) {",
    "  .card a:hover {",
    "    color: var(--ai-primary);",
    "  }",
    "}",
    "@supports (color: oklch(0 0 0)) {",
    "  .card {",
    "    background: var(--ai-surface);",
    "  }",
    "}",
  ].join("\n");

  assert.deepEqual(lintStylesheet(css), []);
  assert.deepEqual(
    declarations(css).map(({ property }) => property),
    ["color", "background"],
  );
});

test("the site's own stylesheets take every colour, radius, and face from tokens", async () => {
  assert.deepEqual(await lintSite(), []);
});

test("the lint reads the stylesheets the site actually ships", async () => {
  // A lint pointed at nothing passes forever. Prove it found the real file and
  // that the file it found is the one the app imports.
  const files = await authoredStylesheets();
  assert.ok(files.length > 0, "no authored stylesheet was found");
  assert.ok(
    files.some((file) => file.endsWith(path.join("app", "site.css"))),
    `site.css is not among ${JSON.stringify(files)}`,
  );

  const app = await readFile(path.join(siteRoot, "app", "App.tsx"), "utf8");
  assert.match(app, /import "\.\/site\.css"/, "the app must import the stylesheet the lint checks");

  const css = await readFile(path.join(siteRoot, "app", "site.css"), "utf8");
  assert.ok(declarations(css).length > 50, "the parser found almost no declarations in site.css");
});

test("generated stylesheets are exempt, because literals are what they are for", async () => {
  const generated = await readFile(path.join(siteRoot, "generated", "themes.css"), "utf8");
  const findings = lintStylesheet(generated, "generated/themes.css");

  // Sanity: themes.css is nothing but literal colours, so if the lint were run
  // over it there would be hundreds of findings. That it is excluded from
  // lintSite is the behaviour under test.
  assert.ok(findings.length > 100, "themes.css should be full of literals");
  assert.deepEqual(
    (await lintSite()).filter((finding) => finding.file.startsWith("generated/")),
    [],
    "the lint must not read generated output",
  );
});
