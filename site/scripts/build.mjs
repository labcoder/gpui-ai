import { spawn } from "node:child_process";
import { access, cp, mkdir, mkdtemp, readFile, readdir, rename, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";


const siteRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const repositoryRoot = path.resolve(siteRoot, "..");
const VITE_BIN = path.join(siteRoot, "node_modules", "vite", "bin", "vite.js");
/** Where React's markup goes; Vite copies it through from index.html. */
const APP_MARKER = "<!--app-html-->";

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
  // Built inside site/ rather than the staging directory: the bundle imports
  // react, and Node resolves that from site/node_modules.
  const ssrDir = path.join(siteRoot, ".ssr");
  await viteBuild(["--outDir", stageDir, "--emptyOutDir"]);
  await viteBuild(["--ssr", "prerender.tsx", "--outDir", ssrDir, "--emptyOutDir"]);

  const template = await readFile(path.join(stageDir, "index.html"), "utf8");
  if (!template.includes(APP_MARKER)) {
    throw new Error(`the built index.html lost its ${APP_MARKER} placeholder`);
  }

  const { render, routes } = await import(
    pathToFileURL(path.join(ssrDir, "prerender.js")).href
  );

  for (const route of routes) {
    const html = template
      .replace(APP_MARKER, render(route.path))
      .replace("<title>gpui-ai</title>", `<title>${escapeHtml(route.title)}</title>`)
      .replace(
        "</head>",
        `  <meta name="description" content="${escapeHtml(route.description)}">\n  </head>`,
      );
    const directory = path.join(stageDir, ...route.path.split("/").filter(Boolean));
    await mkdir(directory, { recursive: true });
    await writeFile(path.join(directory, "index.html"), html);
  }

  // The SSR bundle is a build artifact, not part of the site.
  await rm(ssrDir, { force: true, recursive: true });
  await cp(galleryDir, path.join(stageDir, "gallery"), { recursive: true });
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
