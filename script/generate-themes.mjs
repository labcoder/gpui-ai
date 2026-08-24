// Turn the themes/ directory into the data the website needs.
//
//   npm run generate
//
// Writes site/generated/themes.json (the grouped picker and the token readout),
// site/generated/themes.css (the `--ai-*` custom properties the page chrome is
// painted from), and site/public/themes/<slug>.json (each theme on its own, for
// anyone who wants the file rather than the picture). All three are tracked, and
// CI regenerates them and fails on a diff, so they cannot drift from themes/.
//
// The output lives in site/generated rather than site/src because S-01
// replaces the static generator under site/src with a Vite app; the generated
// data outlives that rewrite.

import { mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = fileURLToPath(new URL("..", import.meta.url));
const THEMES = join(ROOT, "themes");
const OUTPUT = join(ROOT, "site", "generated");

const GROUPS = [
  { id: "gpui-ai", label: "gpui-ai", directory: join(THEMES, "gpui-ai") },
  {
    id: "gpui-component",
    label: "gpui-component",
    directory: join(THEMES, "upstream"),
    license: "Apache-2.0",
    source: "https://github.com/longbridge/gpui-component",
  },
];

// gpui-component resolves these two on its own; outside Rust we read the same
// files, vendored by script/vendor-themes.mjs.
const DEFAULTS = join(THEMES, "upstream", "defaults");

// Status colours are optional in a ThemeConfig — most packs omit them — so a
// token that nothing supplies falls back to a readable value for the mode
// rather than disappearing from the stylesheet.
const STATUS_FALLBACKS = {
  light: { danger: "#dc2626", success: "#059669", warning: "#d97706", info: "#0284c7" },
  dark: { danger: "#f87171", success: "#34d399", warning: "#fbbf24", info: "#38bdf8" },
};

const FONT_FALLBACK = {
  sans: '"IBM Plex Sans", ui-sans-serif, system-ui, sans-serif',
  mono: '"Lilex", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
};

const SHADOW = {
  light: "0 1px 2px 0 rgb(0 0 0 / 0.05), 0 1px 3px 0 rgb(0 0 0 / 0.1)",
  dark: "0 1px 2px 0 rgb(0 0 0 / 0.4), 0 1px 3px 0 rgb(0 0 0 / 0.5)",
};

function fail(message) {
  process.stderr.write(`${message}\n`);
  process.exit(1);
}

function readJson(path) {
  try {
    return JSON.parse(readFileSync(path, "utf8"));
  } catch (error) {
    fail(`cannot read ${path}: ${error.message}`);
  }
}

function kebab(value) {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

const palette = readJson(join(DEFAULTS, "default-colors.json"));

// A colour is either literal CSS or a name from gpui-component's palette.
// Single colours ("white") are objects with a hex; scaled families are arrays,
// so "neutral-500" means scale 500 of `neutral`.
function resolveColor(value) {
  if (typeof value !== "string" || value === "") return undefined;
  const name = value.trim();
  if (/^(#|rgb|hsl|oklch|var\()/i.test(name)) return name;

  const direct = palette[name];
  if (typeof direct === "string") return direct;
  if (direct && typeof direct.hex === "string") return direct.hex;

  const scaled = /^([a-z]+)-(\d+)$/.exec(name);
  if (scaled) {
    const family = palette[scaled[1]];
    if (Array.isArray(family)) {
      const step = family.find((entry) => entry.scale === Number(scaled[2]));
      if (step && typeof step.hex === "string") return step.hex;
    }
  }
  return undefined;
}

// Walks candidate keys in order and returns the first that resolves.
function pick(colors, ...candidates) {
  for (const candidate of candidates) {
    const resolved = resolveColor(colors[candidate]);
    if (resolved) return resolved;
  }
  return undefined;
}

// The registry's status colours are fills — `danger.background`,
// `success.background` — and using one as text on the code panel's accent
// surface is asking a background to do a foreground's job. On a light or
// mid-toned accent it lands around 2:1 and the code is unreadable.
//
// So the code colours are derived rather than borrowed: take the status hue the
// convention expects, then walk it toward black or white — whichever direction
// the accent is not — until it clears 4.5:1 against that accent. The hue
// survives, which is the part a reader recognises; the lightness moves, which
// is the part that has to.

function channel(value) {
  const c = value / 255;
  return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}

function luminance([r, g, b]) {
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}

function contrast(a, b) {
  const [hi, lo] = luminance(a) >= luminance(b) ? [a, b] : [b, a];
  return (luminance(hi) + 0.05) / (luminance(lo) + 0.05);
}

function toRgb(hex) {
  const match = /^#([0-9a-f]{6})$/i.exec(hex ?? "");
  if (!match) return undefined;
  const value = Number.parseInt(match[1], 16);
  return [(value >> 16) & 255, (value >> 8) & 255, value & 255];
}

function toHex([r, g, b]) {
  return `#${[r, g, b].map((c) => Math.round(c).toString(16).padStart(2, "0")).join("")}`;
}

const mix = (from, to, amount) => from.map((c, index) => c + (to[index] - c) * amount);

/**
 * The nearest version of a colour that can be read on a given background.
 *
 * Returns undefined when even pure black or white cannot reach the target,
 * which no theme in the registry does but a future one might.
 */
function readable(source, against, target = 4.5) {
  const from = toRgb(source);
  const behind = toRgb(against);
  if (!from || !behind) return undefined;
  if (contrast(from, behind) >= target) return toHex(from);

  const toward = luminance(behind) > 0.18 ? [0, 0, 0] : [255, 255, 255];
  for (let amount = 0.05; amount <= 1.0001; amount += 0.05) {
    const candidate = mix(from, toward, amount);
    if (contrast(candidate, behind) >= target) return toHex(candidate);
  }
  return undefined;
}

function tokensFor(theme) {
  const colors = theme.colors ?? {};
  const mode = theme.mode === "dark" ? "dark" : "light";
  const status = STATUS_FALLBACKS[mode];

  const background = pick(colors, "background") ?? (mode === "dark" ? "#0a0a0a" : "#ffffff");
  const foreground = pick(colors, "foreground") ?? (mode === "dark" ? "#fafafa" : "#0a0a0a");

  return {
    "--ai-background": background,
    "--ai-foreground": foreground,
    "--ai-muted": pick(colors, "muted.foreground", "muted") ?? foreground,
    "--ai-border": pick(colors, "border", "input.border") ?? foreground,
    "--ai-surface":
      pick(colors, "popover.background", "secondary.background", "muted.background") ?? background,
    "--ai-primary": pick(colors, "primary.background", "primary") ?? foreground,
    "--ai-primary-foreground": pick(colors, "primary.foreground") ?? background,
    "--ai-accent":
      pick(colors, "accent.background", "accent", "secondary.background") ?? background,
    "--ai-danger": pick(colors, "danger.background", "danger") ?? status.danger,
    "--ai-success": pick(colors, "success.background", "success") ?? status.success,
    "--ai-warning": pick(colors, "warning.background", "warning") ?? status.warning,
    "--ai-info": pick(colors, "info.background", "info") ?? status.info,
    "--ai-radius": `${theme.radius ?? 6}px`,
    "--ai-radius-lg": `${theme["radius.lg"] ?? 8}px`,
    "--ai-font-size": `${theme["font.size"] ?? 16}px`,
    "--ai-font-sans": theme["font.family"]
      ? `"${theme["font.family"]}", ${FONT_FALLBACK.sans}`
      : FONT_FALLBACK.sans,
    "--ai-font-mono": theme["mono_font.family"]
      ? `"${theme["mono_font.family"]}", ${FONT_FALLBACK.mono}`
      : FONT_FALLBACK.mono,
    "--ai-shadow": theme.shadow === false ? "none" : SHADOW[mode],
  };
}

/** Syntax colours, guaranteed legible on the surface the code panel uses. */
function codeTokensFor(tokens) {
  const surface = tokens["--ai-accent"];
  const fallback = tokens["--ai-foreground"];
  const derive = (source) => readable(source, surface) ?? fallback;

  return {
    "--ai-code-comment": derive(tokens["--ai-muted"]),
    "--ai-code-keyword": derive(tokens["--ai-danger"]),
    "--ai-code-string": derive(tokens["--ai-success"]),
    "--ai-code-type": derive(tokens["--ai-info"]),
    "--ai-code-number": derive(tokens["--ai-warning"]),
  };
}

// Every theme is also written out on its own, as a registry file a visitor can
// download and drop straight into themes/. Reconstructing one from the derived
// --ai-* values would produce something that looks right and is not the theme;
// this is the source object, verbatim, in the pack shape the registry reads.
// These land in site/public so Vite serves them at a real URL, and they are
// generated and diff-gated exactly like everything under site/generated.
const DOWNLOADS = join(ROOT, "site", "public", "themes");
const downloads = new Map();

function describe(theme, slug, group) {
  downloads.set(slug, {
    $comment: "Generated by script/generate-themes.mjs. The source theme, as the registry reads it.",
    name: theme.name,
    themes: [theme],
  });

  const mode = theme.mode === "dark" ? "dark" : "light";
  const base = tokensFor(theme);
  return {
    slug,
    // gpui-ai's own presets carry the project name in the registry so they do
    // not collide upstream; the picker does not need to repeat it.
    label: theme.name.startsWith("gpui-ai ") ? theme.name.slice("gpui-ai ".length) : theme.name,
    registryName: theme.name,
    group,
    mode,
    radius: theme.radius ?? 6,
    radiusLg: theme["radius.lg"] ?? 8,
    fontSize: theme["font.size"] ?? 16,
    shadow: theme.shadow !== false,
    tokens: { ...base, ...codeTokensFor(base) },
  };
}

const groups = [];

// Light and Dark are gpui-component's defaults, so they come from its own
// vendored default-theme.json rather than from a file of ours.
const defaultPack = readJson(join(DEFAULTS, "default-theme.json"));
const basics = [];
for (const theme of defaultPack.themes ?? []) {
  const slug = theme.mode === "dark" ? "dark" : "light";
  basics.push({
    ...describe(theme, slug, "gpui-ai"),
    label: slug === "dark" ? "Dark" : "Light",
  });
}
if (basics.length !== 2) fail("the vendored default theme must supply exactly Light and Dark");

for (const group of GROUPS) {
  const themes = group.id === "gpui-ai" ? [...basics] : [];

  const files = readdirSync(group.directory)
    .filter((name) => name.endsWith(".json"))
    .sort();
  for (const file of files) {
    const pack = readJson(join(group.directory, file));
    const bundled = pack.themes ?? [];
    if (group.id === "gpui-ai" && bundled.length !== 1) {
      fail(`${file} must hold exactly one theme so its file name can be the slug`);
    }
    for (const theme of bundled) {
      const slug = group.id === "gpui-ai" ? file.replace(/\.json$/, "") : kebab(theme.name);
      themes.push(describe(theme, slug, group.id));
    }
  }

  groups.push({
    id: group.id,
    label: group.label,
    ...(group.license ? { license: group.license, source: group.source } : {}),
    themes,
  });
}

const slugs = groups.flatMap((group) => group.themes.map((theme) => theme.slug));
if (new Set(slugs).size !== slugs.length) fail("theme slugs must be unique across both groups");

mkdirSync(OUTPUT, { recursive: true });

writeFileSync(
  join(OUTPUT, "themes.json"),
  `${JSON.stringify(
    {
      $comment: "Generated by script/generate-themes.mjs from themes/. Do not edit.",
      groups,
    },
    null,
    2,
  )}\n`,
);

const blocks = [
  "/* Generated by script/generate-themes.mjs from themes/. Do not edit. */",
  "/* The page chrome is painted only from these properties, so choosing a",
  "   theme reskins the site and every demo from the same numbers. */",
  "",
];
for (const group of groups) {
  for (const theme of group.themes) {
    const selector =
      theme.slug === "light" ? `:root,\n[data-theme="light"]` : `[data-theme="${theme.slug}"]`;
    const body = Object.entries(theme.tokens)
      .map(([name, value]) => `  ${name}: ${value};`)
      .join("\n");
    blocks.push(`/* ${theme.registryName} — ${group.label}, ${theme.mode} */`);
    blocks.push(`${selector} {\n  color-scheme: ${theme.mode};\n${body}\n}`);
    blocks.push("");
  }
}
writeFileSync(join(OUTPUT, "themes.css"), `${blocks.join("\n")}`);

// Rewritten from scratch each run, so a theme file that is deleted upstream
// does not leave a download behind that nothing in the picker points at.
rmSync(DOWNLOADS, { force: true, recursive: true });
mkdirSync(DOWNLOADS, { recursive: true });
for (const [slug, pack] of downloads) {
  writeFileSync(join(DOWNLOADS, `${slug}.json`), `${JSON.stringify(pack, null, 2)}\n`);
}

const total = groups.reduce((count, group) => count + group.themes.length, 0);
process.stdout.write(
  `generated site/generated/themes.json and themes.css: ${total} themes in ${groups.length} groups\n` +
    `generated site/public/themes: ${downloads.size} downloadable registry files\n`,
);
