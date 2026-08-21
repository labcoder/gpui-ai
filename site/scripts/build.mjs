import { access, cp, mkdir, mkdtemp, readFile, readdir, rename, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import { components } from "../src/catalog.js";
import { catalogPage, componentPage, homePage } from "../src/templates.js";

const siteRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const repositoryRoot = path.resolve(siteRoot, "..");

export async function buildSite({
  outDir = path.join(siteRoot, "dist"),
  galleryDir = path.join(repositoryRoot, "crates", "gallery-web", "www", "dist"),
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
  try {
    await generateInto(stageDir, galleryDir);
    try {
      await access(outDir);
      await rename(outDir, backupDir);
      backedUp = true;
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
    }

    try {
      await rename(stageDir, outDir);
      promoted = true;
    } catch (promotionError) {
      if (backedUp) await rename(backupDir, outDir);
      throw promotionError;
    }
  } finally {
    if (!promoted) await rm(stageDir, { force: true, recursive: true });
    if (promoted) {
      await rm(backupRoot, { force: true, recursive: true }).catch((error) => {
        process.emitWarning(`Built the site, but could not remove ${backupRoot}: ${error.message}`);
      });
    } else {
      await rm(backupRoot, { force: true, recursive: true });
    }
  }
}

async function generateInto(stageDir, galleryDir) {
  await mkdir(path.join(stageDir, "components"), { recursive: true });
  await writeFile(path.join(stageDir, "index.html"), homePage());
  await writeFile(path.join(stageDir, "components", "index.html"), catalogPage());

  for (const item of components) {
    const directory = path.join(stageDir, "components", item.slug);
    await mkdir(directory, { recursive: true });
    await writeFile(path.join(directory, "index.html"), componentPage(item));
  }

  const assetsDir = path.join(stageDir, "assets");
  await mkdir(assetsDir, { recursive: true });
  await Promise.all([
    copySource("styles.css", path.join(assetsDir, "styles.css")),
    copySource("shell.js", path.join(assetsDir, "shell.js")),
    copySource("catalog.js", path.join(assetsDir, "catalog.js")),
    copySource("runtime.js", path.join(assetsDir, "runtime.js")),
  ]);
  await cp(galleryDir, path.join(stageDir, "gallery"), { recursive: true });
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

async function copySource(name, destination) {
  const contents = await readFile(path.join(siteRoot, "src", name));
  await writeFile(destination, contents);
}

const invokedPath = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : "";
if (import.meta.url === invokedPath) {
  await buildSite();
  process.stdout.write(`Built ${components.length} component pages in site/dist\n`);
}
