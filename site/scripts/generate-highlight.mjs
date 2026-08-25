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
      // `entity.name.section` is TOML's `[dependencies]`. It is a name that
      // introduces a block, which is what `type` already means here.
      scope: [
        "entity.name.type",
        "support.type",
        "entity.name.namespace",
        "meta.generic",
        "entity.name.section",
      ],
      settings: { foreground: CATEGORIES.type.sentinel },
    },
    {
      scope: ["constant.numeric", "constant.language.boolean"],
      settings: { foreground: CATEGORIES.number.sentinel },
    },
  ],
};

const highlighter = await createHighlighter({ themes: [THEME], langs: ["rust", "toml"] });

const snippets = JSON.parse(readFileSync(join(GENERATED, "snippets.json"), "utf8")).snippets;
const build = JSON.parse(readFileSync(join(GENERATED, "build.json"), "utf8"));

/**
 * The dependency lines the home page shows, highlighted like everything else.
 *
 * Composed here rather than in the page so the version, this repository, and
 * the GPUI pin all come from `build.json` — the same file the rest of the
 * release information is read from — and so the only copy of the text is the
 * one that was highlighted.
 *
 * That makes this step depend on `build.json` being current, which is why the
 * repository's `generate:highlight` runs `generate:build-info` first. Running
 * this script on its own, after a version bump, emits the previous version.
 */
function installSnippet() {
  const pin = (id) => {
    const found = build.upstream.find((entry) => entry.id === id);
    if (!found) throw new Error(`build.json names no ${id} pin to install alongside`);
    return found.repository;
  };

  // All four, because an application uses all four directly: gpui and
  // gpui-component are what it writes UI against, and gpui_platform opens the
  // window. Two lines would compile here and not in anyone else's project.
  return [
    "[dependencies]",
    `gpui-ai = { git = "${build.repository}", tag = "v${build.version}" }`,
    `gpui = { git = "${pin("gpui")}" }`,
    `gpui-component = { git = "${pin("gpui-component")}" }`,
    `gpui_platform = { git = "${pin("gpui")}", features = ["font-kit", "x11", "wayland"] }`,
  ].join("\n");
}

/** One snippet, as lines of `[text]` or `[text, class]`. */
function tokenize(code, lang = "rust") {
  const { tokens } = highlighter.codeToTokens(code, {
    lang,
    theme: "gpui-ai-sentinels",
  });
  return tokens.map((line) =>
    line.map((token) => {
      const category = SENTINELS.get((token.color ?? "").toLowerCase());
      return category ? [token.content, category] : [token.content];
    }),
  );
}

/**
 * Tokenize, and refuse to write anything the highlighter altered.
 *
 * The one invariant that matters: whatever the highlighter did to it, the text
 * is still the code. The site copies from `snippets.json` rather than from this
 * file, and this is what keeps the two the same code.
 */
function checked(code, lang, label) {
  const tokenized = tokenize(code, lang);
  const recovered = tokenized.map((line) => line.map(([text]) => text).join("")).join("\n");
  if (recovered !== code) {
    throw new Error(
      `highlighting changed ${label}:\n--- expected ---\n${code}\n--- got ---\n${recovered}`,
    );
  }
  return tokenized;
}

const highlighted = {};
let lines = 0;

for (const [slug, variants] of Object.entries(snippets)) {
  highlighted[slug] = {};
  for (const [variant, code] of Object.entries(variants)) {
    const tokenized = checked(code, "rust", `${slug}/${variant}`);
    highlighted[slug][variant] = tokenized;
    lines += tokenized.length;
  }
}

// Code the site shows that is not cut from a story. Only the home page's
// dependency lines so far, and they are TOML rather than Rust.
const install = installSnippet();
const extras = {
  install: { lang: "toml", code: install, lines: checked(install, "toml", "extras/install") },
};
lines += extras.install.lines.length;

mkdirSync(GENERATED, { recursive: true });
writeFileSync(
  join(GENERATED, "highlight.json"),
  `${JSON.stringify(
    {
      $comment:
        "Generated by site/scripts/generate-highlight.mjs from snippets.json and build.json. Do not edit. Each token is [text] or [text, category]; the categories are styled from --ai-* properties in site/app/site.css.",
      categories: Object.fromEntries(
        Object.entries(CATEGORIES).map(([name, { token }]) => [name, token]),
      ),
      snippets: highlighted,
      extras,
    },
    null,
    2,
  )}\n`,
);

process.stdout.write(
  `site/generated/highlight.json: ${Object.keys(highlighted).length} snippets, ` +
    `${Object.keys(extras).length} extras, ${lines} lines, ` +
    `${Object.keys(CATEGORIES).length} token categories\n`,
);
