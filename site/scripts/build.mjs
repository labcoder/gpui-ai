import { spawn } from "node:child_process";
import { access, cp, mkdir, mkdtemp, readFile, readdir, rename, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import buildInfo from "../generated/build.json" with { type: "json" };
import { socialCardName } from "../app/route-path.mjs";


const siteRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const repositoryRoot = path.resolve(siteRoot, "..");
const VITE_BIN = path.join(siteRoot, "node_modules", "vite", "bin", "vite.js");
/** Where React's markup goes; Vite copies it through from index.html. */
const APP_MARKER = "<!--app-html-->";

/**
 * Where the site is published, from the crate's own `homepage`.
 *
 * Absolute URLs are not a style choice here: a canonical link, a sitemap entry
 * and an Open Graph image are all read by something that is not the browser
 * showing the page, and a relative path means nothing to any of them.
 */
const ORIGIN = buildInfo.homepage.replace(/\/$/, "");

/** Where the social cards are written, relative to the site root. */
const socialCard = (routePath) => `/og/${socialCardName(routePath)}.png`;

export async function buildSite({
  outDir = path.join(siteRoot, "dist"),
  galleryDir = path.join(repositoryRoot, "crates", "gallery-web", "www", "dist"),
  renamePath = rename,
} = {}) {
  await validateGallery(galleryDir);
  const parentDir = path.dirname(outDir);
  const outputName = path.basename(outDir);
  await mkdir(parentDir, { recursive: true });
  const stageDir = await mkdtemp(path.join(parentDir, `.${outputName}.stage-`));
  const backupRoot = await mkdtemp(path.join(parentDir, `.${outputName}.backup-`));
  const backupDir = path.join(backupRoot, "previous");
  let promoted = false;
  let backedUp = false;
  let preserveBackup = false;
  try {
    await generateInto(stageDir, galleryDir);
    try {
      await access(outDir);
      await renamePath(outDir, backupDir);
      backedUp = true;
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
    }

    try {
      await renamePath(stageDir, outDir);
      promoted = true;
    } catch (promotionError) {
      if (backedUp) {
        try {
          await renamePath(backupDir, outDir);
        } catch (rollbackError) {
          preserveBackup = true;
          throw new AggregateError(
            [promotionError, rollbackError],
            `Site promotion and rollback failed; prior output backup preserved at ${backupDir}`,
          );
        }
      }
      throw promotionError;
    }
  } finally {
    if (!promoted) await rm(stageDir, { force: true, recursive: true });
    if (promoted) {
      await rm(backupRoot, { force: true, recursive: true }).catch((error) => {
        process.emitWarning(`Built the site, but could not remove ${backupRoot}: ${error.message}`);
      });
    } else if (!preserveBackup) {
      await rm(backupRoot, { force: true, recursive: true });
    }
  }
}

// Builds the client bundle, then writes every route as real HTML.
//
// GitHub Pages serves files and nothing else, so a route that exists only in
// JavaScript returns a hard 404 on a deep link or a refresh. Pre-rendering each
// route to <path>/index.html is what makes the site work there — it is not an
// optimisation, and the usual 404.html rewrite hack is what it replaces.
async function generateInto(stageDir, galleryDir) {
  // Built inside site/ rather than the staging directory, because the bundle
  // imports react and Node resolves that from site/node_modules. The directory
  // is unique per build so two builds cannot empty each other's output, and it
  // is removed whether or not this succeeds.
  const ssrDir = await mkdtemp(path.join(siteRoot, ".ssr-"));
  try {
    await generateWithSsr(stageDir, galleryDir, ssrDir);
  } finally {
    await rm(ssrDir, { force: true, recursive: true });
  }
}

async function generateWithSsr(stageDir, galleryDir, ssrDir) {
  await viteBuild(["--outDir", stageDir, "--emptyOutDir"]);
  await viteBuild(["--ssr", "prerender.tsx", "--outDir", ssrDir, "--emptyOutDir"]);

  const template = await readFile(path.join(stageDir, "index.html"), "utf8");
  if (!template.includes(APP_MARKER)) {
    throw new Error(`the built index.html lost its ${APP_MARKER} placeholder`);
  }

  const { render, renderNotFound, routes } = await import(
    pathToFileURL(path.join(ssrDir, "prerender.js")).href
  );

  const preloads = await fontPreloads(stageDir, template);

  for (const route of routes) {
    const html = template
      .replace(APP_MARKER, render(route.path))
      .replace("<title>gpui-ai</title>", `<title>${escapeHtml(route.title)}</title>`)
      .replace("</head>", `${head(route)}${preloads}  </head>`);
    const directory = path.join(stageDir, ...route.path.split("/").filter(Boolean));
    await mkdir(directory, { recursive: true });
    await writeFile(path.join(directory, "index.html"), html);
  }

  await writeFile(path.join(stageDir, "sitemap.xml"), sitemap(routes));
  await writeFile(path.join(stageDir, "robots.txt"), robots());
  await writeFile(path.join(stageDir, "404.html"), notFound(template, renderNotFound, preloads));

  await cp(galleryDir, path.join(stageDir, "gallery"), { recursive: true });
}

/**
 * The metadata every page carries, beyond its title.
 *
 * A canonical link, because the same page answers at `/components/chat/` and
 * at `/components/chat/index.html` and a crawler that sees both counts them as
 * two. The Open Graph and Twitter tags are what a link to this site expands
 * into in a chat window or a timeline; without them the unfurl is the URL and
 * nothing else, which is what every share of this site produced until now.
 *
 * `og:image` names a file `npm run generate:og` writes after this build. The
 * pair is checked by the site's own tests rather than trusted: a tag pointing
 * at a card nobody rendered is worse than no tag, because the unfurl breaks
 * instead of degrading.
 */
function head(route) {
  const url = `${ORIGIN}${route.path}`;
  const tags = [
    ["link", { rel: "canonical", href: url }],
    ["meta", { name: "description", content: route.description }],
    ["meta", { property: "og:type", content: "website" }],
    ["meta", { property: "og:site_name", content: "gpui-ai" }],
    ["meta", { property: "og:title", content: route.title }],
    ["meta", { property: "og:description", content: route.description }],
    ["meta", { property: "og:url", content: url }],
    ["meta", { property: "og:image", content: `${ORIGIN}${socialCard(route.path)}` }],
    ["meta", { property: "og:image:width", content: String(CARD.width) }],
    ["meta", { property: "og:image:height", content: String(CARD.height) }],
    ["meta", { property: "og:image:alt", content: route.description }],
    ["meta", { name: "twitter:card", content: "summary_large_image" }],
    ["meta", { name: "twitter:title", content: route.title }],
    ["meta", { name: "twitter:description", content: route.description }],
    ["meta", { name: "twitter:image", content: `${ORIGIN}${socialCard(route.path)}` }],
  ];
  return tags
    .map(([tag, attributes]) => {
      const written = Object.entries(attributes)
        .map(([name, value]) => `${name}="${escapeHtml(value)}"`)
        .join(" ");
      return `  <${tag} ${written}>\n`;
    })
    .join("");
}

/** The size every social card is rendered at. */
export const CARD = { width: 1_200, height: 630 };

/**
 * Every page, for a crawler that would otherwise have to find them by link.
 *
 * No `lastmod`: the only honest value would be the commit each page's content
 * came from, and writing the build date instead tells a crawler every page
 * changed every time anything did.
 */
function sitemap(routes) {
  const urls = routes
    .map((route) => `  <url><loc>${escapeHtml(`${ORIGIN}${route.path}`)}</loc></url>`)
    .join("\n");
  return `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${urls}\n</urlset>\n`;
}

/**
 * Everything is public, and the sitemap says where it is.
 *
 * `/gallery/` is excluded: it is the demo embed, one page per story with no
 * prose of its own, and a crawler indexing seventy copies of a canvas would be
 * indexing noise. The component pages already link every story.
 */
function robots() {
  return `User-agent: *\nAllow: /\nDisallow: /gallery/\n\nSitemap: ${ORIGIN}/sitemap.xml\n`;
}

/**
 * The page a mistyped URL gets.
 *
 * GitHub Pages serves `404.html` for anything it cannot find, and without one
 * a visitor gets GitHub's own page with no way back. This is the site's own
 * chrome — masthead, rail, footer — around a short explanation, rendered by
 * the same component tree as every other page, so it cannot drift out of the
 * site's design.
 *
 * No canonical link and no social card: this page is not a destination, and
 * telling a crawler it is one is how a 404 ends up in search results.
 */
function notFound(template, renderNotFound, preloads) {
  return template
    .replace(APP_MARKER, renderNotFound())
    .replace("<title>gpui-ai</title>", "<title>Page not found · gpui-ai</title>")
    .replace(
      "</head>",
      `  <meta name="robots" content="noindex">\n${preloads}  </head>`,
    );
}

/**
 * `<link rel="preload">` for every face the stylesheet asks for.
 *
 * The faces arrive through `@import "@fontsource/…"` inside site.css, and Vite
 * does not preload what an @import pulled in — so the chrome painted in the
 * system fallback and moved when Plex and Lilex landed, about a second into a
 * cold visit. Discovering the names from the build rather than writing them
 * down is the only version that survives a hashed filename.
 *
 * Only `.woff2`: the `.woff` beside it is the fallback for browsers that will
 * never be asked for it here, and preloading both would double the bytes for
 * nothing. `crossorigin` is not optional — fonts are fetched in CORS mode even
 * same-origin, and a preload without it is fetched twice.
 */
async function fontPreloads(stageDir, template) {
  const base = /href="([^"]*)\/assets\//.exec(template)?.[1] ?? "";
  const assets = path.join(stageDir, "assets");
  const faces = (await readdir(assets).catch(() => []))
    .filter((name) => name.endsWith(".woff2"))
    .sort();

  if (faces.length === 0) throw new Error("the build produced no .woff2 faces to preload");

  return faces
    .map((name) => `  <link rel="preload" as="font" type="font/woff2" crossorigin href="${base}/assets/${name}">\n`)
    .join("");
}

/** Runs Vite's CLI from the site root and fails loudly. */
function viteBuild(args) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [VITE_BIN, "build", ...args], {
      cwd: siteRoot,
      stdio: "pipe",
      env: { ...process.env, NODE_ENV: "production" },
    });
    let output = "";
    child.stdout?.on("data", (chunk) => (output += chunk));
    child.stderr?.on("data", (chunk) => (output += chunk));
    child.once("error", reject);
    child.once("exit", (code) => {
      if (code === 0) resolve();
      else reject(new Error(`vite build ${args.join(" ")} failed with exit code ${code}\n${output}`));
    });
  });
}

function escapeHtml(value) {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

async function validateGallery(galleryDir) {
  try {
    await Promise.all([
      access(path.join(galleryDir, "index.html")),
      access(path.join(galleryDir, "embed.html")),
    ]);
    const entries = await readdir(galleryDir, { recursive: true, withFileTypes: true });
    if (!entries.some((entry) => entry.isFile() && entry.name.endsWith(".wasm"))) {
      throw new Error("missing WebAssembly artifact");
    }
  } catch (error) {
    throw new Error(`Gallery build is incomplete at ${galleryDir}: ${error.message}`, {
      cause: error,
    });
  }
}

const invokedPath = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : "";
if (import.meta.url === invokedPath) {
  await buildSite();
  process.stdout.write("Built the pre-rendered site into site/dist\n");
}
