// Refuses a hand-written colour, radius, or font family in the site's own CSS.
//
//   node scripts/token-lint.mjs
//
// The whole point of the theme registry is that one JSON file decides what the
// page looks like, and `site/generated/themes.css` turns that into `--ai-*`
// properties the chrome reads. A single `#0a0a0a` typed into a stylesheet is
// invisible until someone switches to Ember Dusk and one border stays the wrong
// colour. This runs in the site test suite, so the mistake fails a gate on the
// day it is written rather than being found by a visitor.
//
// Generated files are exempt: `themes.css` is nothing but literals, and that is
// what it is for. The lint reads authored files and never follows an @import.

import { readFile, readdir } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const siteRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

/** Keywords that are colour-valued but name no colour of their own. */
const COLOUR_KEYWORDS = new Set([
  "currentcolor",
  "transparent",
  "inherit",
  "initial",
  "unset",
  "revert",
  "revert-layer",
  "none",
  "auto",
]);

/** Every CSS named colour. `red` in a stylesheet is as unthemed as `#f00`. */
const NAMED_COLOURS = new Set(
  `aliceblue antiquewhite aqua aquamarine azure beige bisque black blanchedalmond
   blue blueviolet brown burlywood cadetblue chartreuse chocolate coral
   cornflowerblue cornsilk crimson cyan darkblue darkcyan darkgoldenrod darkgray
   darkgreen darkgrey darkkhaki darkmagenta darkolivegreen darkorange darkorchid
   darkred darksalmon darkseagreen darkslateblue darkslategray darkslategrey
   darkturquoise darkviolet deeppink deepskyblue dimgray dimgrey dodgerblue
   firebrick floralwhite forestgreen fuchsia gainsboro ghostwhite gold goldenrod
   gray green greenyellow grey honeydew hotpink indianred indigo ivory khaki
   lavender lavenderblush lawngreen lemonchiffon lightblue lightcoral lightcyan
   lightgoldenrodyellow lightgray lightgreen lightgrey lightpink lightsalmon
   lightseagreen lightskyblue lightslategray lightslategrey lightsteelblue
   lightyellow lime limegreen linen magenta maroon mediumaquamarine mediumblue
   mediumorchid mediumpurple mediumseagreen mediumslateblue mediumspringgreen
   mediumturquoise mediumvioletred midnightblue mintcream mistyrose moccasin
   navajowhite navy oldlace olive olivedrab orange orangered orchid
   palegoldenrod palegreen paleturquoise palevioletred papayawhip peachpuff peru
   pink plum powderblue purple rebeccapurple red rosybrown royalblue saddlebrown
   salmon sandybrown seagreen seashell sienna silver skyblue slateblue slategray
   slategrey snow springgreen steelblue tan teal thistle tomato turquoise violet
   wheat white whitesmoke yellow yellowgreen`
    .split(/\s+/)
    .filter(Boolean),
);

const COLOUR_FUNCTIONS =
  /\b(rgba?|hsla?|hwb|lab|lch|oklab|oklch|color|color-mix|device-cmyk)\s*\(/i;
const HEX = /#[0-9a-fA-F]{3,8}\b/;

/**
 * Border radii the tokens do not describe.
 *
 * `--ai-radius` and `--ai-radius-lg` are the theme's corner radii. A circle and
 * a pill are shapes, not corner styling, so they are spelled as themselves.
 */
const SHAPE_RADII = new Set(["0", "50%", "100%", "9999px"]);

/** Font stacks the site is allowed to name directly. */
const FONT_KEYWORDS = new Set(["inherit", "initial", "unset", "revert"]);

/**
 * The only font stacks the site may write out.
 *
 * Body and mono come from the theme registry, which is why they are not here:
 * choosing a theme has to be able to change them. Titles do not — the display
 * face is the site's own identity (decision 3), so it is defined once, here, by
 * name. Anything else has to go through a token.
 */
const AUTHORED_FONT_TOKENS = new Set(["--site-font-serif"]);

/**
 * Splits a stylesheet into its declarations, with the line each one starts on.
 *
 * Deliberately not a CSS parser: it tracks brace depth and quoting well enough
 * to tell a declaration from a selector, which is all the rules below need.
 */
export function declarations(css) {
  const found = [];
  let buffer = "";
  let bufferLine = 1;
  let line = 1;
  let depth = 0;
  let quote = "";
  let inComment = false;

  for (let index = 0; index < css.length; index += 1) {
    const character = css[index];
    if (character === "\n") line += 1;

    if (inComment) {
      if (character === "*" && css[index + 1] === "/") {
        inComment = false;
        index += 1;
      }
      continue;
    }
    if (!quote && character === "/" && css[index + 1] === "*") {
      inComment = true;
      index += 1;
      continue;
    }
    if (quote) {
      if (character === quote) quote = "";
      buffer += character;
      continue;
    }
    if (character === '"' || character === "'") {
      quote = character;
      buffer += character;
      continue;
    }

    if (character === "{") {
      depth += 1;
      buffer = "";
      bufferLine = line;
      continue;
    }
    if (character === "}" || character === ";") {
      const text = buffer.trim();
      const colon = text.indexOf(":");
      // Inside a block and shaped like `property: value`. At depth 0 the same
      // shape is an at-rule prelude, which declares nothing.
      if (depth > 0 && colon > 0) {
        found.push({
          property: text.slice(0, colon).trim().toLowerCase(),
          value: text.slice(colon + 1).trim(),
          line: bufferLine,
        });
      }
      if (character === "}") depth = Math.max(0, depth - 1);
      buffer = "";
      bufferLine = line;
      continue;
    }

    if (!buffer) bufferLine = line;
    buffer += character;
  }

  return found;
}

/** Properties whose quoted text is a name or a string, never a colour. */
const QUOTES_ARE_TEXT = new Set(["font-family", "font", "content", "quotes", "src"]);

/**
 * Resolves CSS character escapes, so the lint reads what the browser reads.
 *
 * A backslash escape spells any character in hex, which means a colour can be
 * written in a form no plain search finds. Nobody types that by hand, but a
 * rule that one backslash steps around reports what it happens to notice
 * rather than what it says it enforces.
 */
function unescapeCss(value) {
  return value.replace(/\\([0-9a-fA-F]{1,6})[ \t\n]?/g, (_match, hex) =>
    String.fromCodePoint(Number.parseInt(hex, 16)),
  );
}

/**
 * Strips the parts of a value that cannot hold an authored colour.
 *
 * Only the custom-property *names* go: `--ai-border` would otherwise read as
 * the colour word `border` is not, but `--ai-red-line` would read as `red`. A
 * var() fallback is kept, because `var(--x, #fff)` really does hard-code white.
 *
 * Quoted text is dropped only where a quote means a name — a font family,
 * `content`, `quotes`, an @font-face `src`. Everywhere else it stays, because
 * the usual way a colour reaches a stylesheet unnoticed is inside a quoted SVG
 * data URI, and a fill in there paints exactly as hard-coded a colour as one
 * written in the open.
 */
function withoutReferences(value, property) {
  const resolved = unescapeCss(value).replace(/--[\w-]+/g, " ");
  return QUOTES_ARE_TEXT.has(property) ? resolved.replace(/"[^"]*"|'[^']*'/g, " ") : resolved;
}

function isColourViolation(value, property) {
  const bare = withoutReferences(value, property);
  if (HEX.test(bare) || COLOUR_FUNCTIONS.test(bare)) return true;
  return bare
    .split(/[^a-zA-Z-]+/)
    .some((word) => NAMED_COLOURS.has(word.toLowerCase()) && !COLOUR_KEYWORDS.has(word.toLowerCase()));
}

/**
 * Every rule the site's stylesheets have to satisfy.
 *
 * Font *size* is deliberately absent: `html` takes its size from
 * `--ai-font-size`, so a `rem` in the type scale is already theme-relative.
 */
const RULES = [
  {
    id: "literal-colour",
    // Any property can carry a colour — box-shadow, border, background, and
    // outline all hide one inside a longer value — so this rule reads them all.
    applies: () => true,
    violated: (property, value) => isColourViolation(value, property),
    explain: (property, value) =>
      `\`${property}: ${value}\` names a colour. Use a --ai-* property from site/generated/themes.css.`,
  },
  {
    id: "literal-radius",
    applies: (property) => property === "border-radius" || property.startsWith("border-") && property.endsWith("-radius"),
    violated: (_property, value) =>
      !value.includes("var(--ai-radius") && !value.split(/\s+/).every((part) => SHAPE_RADII.has(part)),
    explain: (property, value) =>
      `\`${property}: ${value}\` sets a corner radius the theme does not know. Use var(--ai-radius) or var(--ai-radius-lg); 0, 50% and 9999px are shapes and are allowed.`,
  },
  {
    id: "literal-font-family",
    // Custom properties are covered too, or a stack could be smuggled in under
    // a name of its own and read back through var().
    applies: (property) =>
      property === "font-family" ||
      property === "font" ||
      (property.startsWith("--") && property.includes("font")),
    violated: (property, value) => {
      if (property.startsWith("--")) return !AUTHORED_FONT_TOKENS.has(property);
      return !/var\(--(ai|site)-font/.test(value) && !FONT_KEYWORDS.has(value.toLowerCase());
    },
    explain: (property, value) =>
      property.startsWith("--")
        ? `\`${property}\` defines a font stack the site does not sanction. The body and mono faces come from the theme registry; ${[...AUTHORED_FONT_TOKENS].join(", ")} is the only face the site names itself.`
        : `\`${property}: ${value}\` names a font stack. Use var(--ai-font-sans), var(--ai-font-mono), or var(--site-font-serif).`,
  },
];

/** Checks one stylesheet's text and returns every rule it breaks. */
export function lintStylesheet(css, file = "<stylesheet>") {
  const findings = [];
  for (const { property, value, line } of declarations(css)) {
    // A custom property's own name says nothing; its value still must not be a
    // hand-picked colour, so the rules run on it like any other declaration.
    for (const rule of RULES) {
      if (!rule.applies(property)) continue;
      if (!rule.violated(property, value)) continue;
      findings.push({ file, line, rule: rule.id, message: rule.explain(property, value) });
    }
  }
  return findings;
}

/** Every authored stylesheet under site/app. */
export async function authoredStylesheets(directory = path.join(siteRoot, "app")) {
  const entries = await readdir(directory, { recursive: true, withFileTypes: true });
  return entries
    .filter((entry) => entry.isFile() && entry.name.endsWith(".css"))
    .map((entry) => path.join(entry.parentPath ?? entry.path ?? directory, entry.name))
    .sort();
}

/** Runs every rule over every authored stylesheet. */
export async function lintSite() {
  const findings = [];
  for (const file of await authoredStylesheets()) {
    const css = await readFile(file, "utf8");
    findings.push(...lintStylesheet(css, path.relative(siteRoot, file).replaceAll("\\", "/")));
  }
  return findings;
}

const invokedPath = process.argv[1] ? path.resolve(process.argv[1]) : "";
if (fileURLToPath(import.meta.url) === invokedPath) {
  const findings = await lintSite();
  if (findings.length > 0) {
    for (const { file, line, rule, message } of findings) {
      process.stderr.write(`${file}:${line} [${rule}] ${message}\n`);
    }
    process.stderr.write(`\n${findings.length} value(s) bypass the theme tokens.\n`);
    process.exit(1);
  }
  const files = await authoredStylesheets();
  process.stdout.write(`${files.length} authored stylesheet(s) take every colour, radius, and face from tokens\n`);
}
