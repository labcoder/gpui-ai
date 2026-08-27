import { existsSync } from "node:fs";
import { appendFile, lstat, mkdir, writeFile } from "node:fs/promises";
import { createWriteStream } from "node:fs";
import { pipeline } from "node:stream/promises";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import config from "./web-test-config.json" with { type: "json" };

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

function run(command, args, stdio = "inherit", env = process.env) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { stdio, env });
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

export async function installLinuxSandbox(build) {
  if (process.platform !== "linux") throw new Error("--linux-sandbox requires Linux");
  if (!/^\d+\.\d+\.\d+\.\d+$/.test(build.version)) throw new Error("Invalid pinned Chrome version");
  // CfT ships this helper itself. Do not assume the runner's unrelated Chrome
  // installation contains /opt/google/chrome/chrome-sandbox or has its SUID bit.
  const source = path.join(path.dirname(build.executable), "chrome_sandbox");
  if (!(await lstat(source)).isFile()) throw new Error(`Chrome sandbox is not a regular file: ${source}`);
  const directory = `/usr/local/lib/gpui-ai-chrome/${build.version}`;
  const sandbox = `${directory}/chrome-sandbox`;
  // Copy out of the writable checkout/cache before granting SUID. Neither the
  // helper nor its parent is writable by the browser's unprivileged user.
  await run("sudo", ["install", "-d", "-o", "root", "-g", "root", "-m", "0755", directory]);
  await run("sudo", ["install", "-o", "root", "-g", "root", "-m", "4755", source, sandbox]);
  await verifyLinuxSandbox(build, sandbox);
  if (process.env.GITHUB_ENV) {
    await appendFile(process.env.GITHUB_ENV, `CHROME_DEVEL_SANDBOX=${sandbox}\nCHROME_PATH=${build.executable}\n`);
  }
  return sandbox;
}

export async function verifyLinuxSandbox(build, sandbox) {
  const installed = await lstat(sandbox);
  if (!installed.isFile() || installed.uid !== 0 || (installed.mode & 0o7777) !== 0o4755) {
    throw new Error(`Chrome sandbox must be a root-owned 4755 file: ${sandbox}`);
  }
  await run(process.execPath, [path.join(root, "script/check-browser-sandbox.mjs")], "inherit", {
    ...process.env, CHROME_DEVEL_SANDBOX: sandbox, CHROME_PATH: build.executable,
    GPUI_AI_WEB_CHROME_VERSION: build.version,
  });
  console.log(`Sandbox renderer probe passed: ${sandbox}`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2);
  if (args.length && (args.length !== 1 || args[0] !== "--linux-sandbox")) throw new Error("Usage: web-browser.mjs [--linux-sandbox]");
  const build = await installBrowser();
  console.log(`Chrome for Testing ${build.version}: ${build.executable}`);
  if (args[0] === "--linux-sandbox") await installLinuxSandbox(build);
}
