// Turn the generated snippets into tokens the site can paint from its theme.
//
//   npm run generate
//
// Writes site/generated/highlight.json: one array of lines per snippet, each
// line an array of `[text]` or `[text, class]` pairs. The site renders those as
// spans and the stylesheet colours them from `--ai-*` properties, so code
// re-skins with everything else.
//
// Tokens rather than HTML, and classes rather than colours, for three reasons.
// A highlighter in the browser would be a large download to re-derive something
// that never changes. HTML strings would need `dangerouslySetInnerHTML` and put
// escaping back in our hands. And a colour baked into the output would be one
// the theme picker could not move — the whole point of the token registry.
//
// Shiki resolves scopes to colours, not to names, so the theme below paints
// each category a sentinel value that means nothing on screen and everything
// here: it is looked up in SENTINELS and becomes a class.

import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { createHighlighter } from "shiki";

const ROOT = fileURLToPath(new URL("../..", import.meta.url));
const GENERATED = join(ROOT, "site", "generated");

/**
 * What each kind of token becomes.
 *
 * The mapping is the conventional one — comments recede, strings are the
 * "good" hue, numbers the warm one, types the cool one, keywords the hot one —
 * so a Rust reader recognises the shape of the code before reading a word of
 * it. Every theme in the registry defines all five, which is why these five
 * and no others: a sixth would have to invent a colour the themes do not have.
 */
const CATEGORIES = {
  comment: { sentinel: "#000001", token: "--ai-code-comment" },
  keyword: { sentinel: "#000002", token: "--ai-code-keyword" },
  string: { sentinel: "#000003", token: "--ai-code-string" },
  type: { sentinel: "#000004", token: "--ai-code-type" },
  number: { sentinel: "#000005", token: "--ai-code-number" },
};

const SENTINELS = new Map(
  Object.entries(CATEGORIES).map(([name, { sentinel }]) => [sentinel.toLowerCase(), name]),
);

const THEME = {
  name: "gpui-ai-sentinels",
  type: "dark",
  colors: { "editor.foreground": "#000000", "editor.background": "#00000000" },
  tokenColors: [
    { scope: ["comment", "punctuation.definition.comment"], settings: { foreground: CATEGORIES.comment.sentinel } },
    {
      // Named scopes, not a bare `keyword`: that also matches
      // `keyword.operator`, which in Rust is `::` and `.` and `=`. Colouring
      // every path separator like a keyword paints the whole snippet red and
      // tells a reader nothing.
      scope: [
        "keyword.control",
        "keyword.other",
        "keyword.declaration",
        "storage",
        "storage.type",
        "storage.modifier",
        "variable.language.self",
        "constant.language",
      ],
      settings: { foreground: CATEGORIES.keyword.sentinel },
    },
    {
      scope: ["string", "string.quoted", "punctuation.definition.string", "constant.character.escape"],
      settings: { foreground: CATEGORIES.string.sentinel },
    },
    {
      scope: ["entity.name.type", "support.type", "entity.name.namespace", "meta.generic"],
      settings: { foreground: CATEGORIES.type.sentinel },
    },
    {
      scope: ["constant.numeric", "constant.language.boolean"],
      settings: { foreground: CATEGORIES.number.sentinel },
    },
  ],
};

const highlighter = await createHighlighter({ themes: [THEME], langs: ["rust"] });

const snippets = JSON.parse(readFileSync(join(GENERATED, "snippets.json"), "utf8")).snippets;

/** One snippet, as lines of `[text]` or `[text, class]`. */
function tokenize(code) {
  const { tokens } = highlighter.codeToTokens(code, {
    lang: "rust",
    theme: "gpui-ai-sentinels",
  });
  return tokens.map((line) =>
    line.map((token) => {
      const category = SENTINELS.get((token.color ?? "").toLowerCase());
      return category ? [token.content, category] : [token.content];
    }),
  );
}

const highlighted = {};
let lines = 0;

for (const [slug, variants] of Object.entries(snippets)) {
  highlighted[slug] = {};
  for (const [variant, code] of Object.entries(variants)) {
    const tokenized = tokenize(code);

    // The one invariant that matters: whatever the highlighter did to it, the
    // text is still the snippet. The site copies from `snippets.json` rather
    // than from this file, and this is what keeps the two the same code.
    const recovered = tokenized.map((line) => line.map(([text]) => text).join("")).join("\n");
    if (recovered !== code) {
      throw new Error(
        `highlighting changed ${slug}/${variant}:\n--- expected ---\n${code}\n--- got ---\n${recovered}`,
      );
    }

    highlighted[slug][variant] = tokenized;
    lines += tokenized.length;
  }
}

mkdirSync(GENERATED, { recursive: true });
writeFileSync(
  join(GENERATED, "highlight.json"),
  `${JSON.stringify(
    {
      $comment:
        "Generated by site/scripts/generate-highlight.mjs from snippets.json. Do not edit. Each token is [text] or [text, category]; the categories are styled from --ai-* properties in site/app/site.css.",
      categories: Object.fromEntries(
        Object.entries(CATEGORIES).map(([name, { token }]) => [name, token]),
      ),
      snippets: highlighted,
    },
    null,
    2,
  )}\n`,
);

process.stdout.write(
  `site/generated/highlight.json: ${Object.keys(highlighted).length} snippets, ${lines} lines, ` +
    `${Object.keys(CATEGORIES).length} token categories\n`,
);
