import { existsSync } from "node:fs";
import { mkdir, writeFile } from "node:fs/promises";
import { createWriteStream } from "node:fs";
import { pipeline } from "node:stream/promises";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import config from "./web-test-config.json" with { type: "json" };

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

function run(command, args, stdio = "inherit") {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { stdio });
    child.once("error", reject);
    child.once("exit", (code) => code === 0 ? resolve() : reject(new Error(`${command} failed: ${code}`)));
  });
}

async function allowBrowserSandbox(directory) {
  if (process.platform !== "win32") return;
  // A downloaded Chrome outside Program Files needs its AppContainer to read
  // its own binaries. Grant RX only on this versioned browser directory, never
  // on the repository, and keep Chrome's sandbox enabled.
  await run("icacls", [directory, "/grant", "*S-1-15-2-1:(OI)(CI)RX", "*S-1-15-2-2:(OI)(CI)RX", "/T", "/Q"], "ignore");
}

export function browserBuild(platform = process.platform, arch = process.arch) {
  const target = { "win32-x64": "win64", "linux-x64": "linux64", "darwin-x64": "mac-x64", "darwin-arm64": "mac-arm64" }[`${platform}-${arch}`];
  if (!target) throw new Error(`No pinned Chrome build for ${platform}-${arch}; use CHROME_PATH for a system-browser check`);
  const directory = path.join(root, "target/web-browser", `${config.chromeVersion}-${target}`);
  const executable = platform === "darwin"
    ? `chrome-${target}/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing`
    : `chrome-${target}/chrome${platform === "win32" ? ".exe" : ""}`;
  return {
    version: config.chromeVersion, directory, executable: path.join(directory, executable),
    url: `https://storage.googleapis.com/chrome-for-testing-public/${config.chromeVersion}/${target}/chrome-${target}.zip`,
  };
}

export async function installBrowser() {
  const build = browserBuild();
  const receipt = path.join(build.directory, "installed");
  if (existsSync(build.executable) && existsSync(receipt)) {
    await allowBrowserSandbox(build.directory);
    return build;
  }
  await mkdir(build.directory, { recursive: true });
  const archive = path.join(build.directory, "chrome.zip");
  const response = await fetch(build.url, { signal: AbortSignal.timeout(120_000) });
  if (!response.ok) throw new Error(`Chrome download failed: HTTP ${response.status}`);
  await pipeline(response.body, createWriteStream(archive));
  // Extract only the official, versioned archive, inside this workspace's ignored target/.
  const [command, args] = process.platform === "win32"
    ? ["tar", ["-xf", archive, "-C", build.directory]]
    : ["unzip", ["-qo", archive, "-d", build.directory]];
  await run(command, args);
  if (!existsSync(build.executable)) throw new Error(`Chrome archive did not contain ${build.executable}`);
  await allowBrowserSandbox(build.directory);
  await writeFile(receipt, `${build.version}\n`);
  return build;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const build = await installBrowser();
  console.log(`Chrome for Testing ${build.version}: ${build.executable}`);
}
