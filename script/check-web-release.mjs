import { spawn } from "node:child_process";

function run(command, args, environment = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      env: { ...process.env, ...environment },
      stdio: "inherit",
    });
    child.once("error", reject);
    child.once("exit", (code, signal) => {
      if (code === 0) {
        resolve();
        return;
      }
      reject(new Error(`${command} ${args.join(" ")} failed with ${signal ?? `exit code ${code}`}`));
    });
  });
}

const npmCli = process.env.npm_execpath;
if (!npmCli) throw new Error("npm_execpath is required; run this gate through npm run check:web:release");
await run(process.execPath, [npmCli, "run", "build:wasm"]);
await run(process.execPath, [npmCli, "--prefix", "crates/gallery-web/www", "run", "build"]);
// Both browser gates, by name. The pattern exists so the unit tests in the
// same file — which need no browser — are not run twice; it is not a way of
// choosing one gate over the other, and a new gate added to that file without
// a name matching here would silently never run.
await run(
  process.execPath,
  [
    "--test",
    "--test-name-pattern=release WASM|every theme the site offers",
    "site/test/browser.test.mjs",
  ],
  { GPUI_AI_RELEASE_INTEGRATION: "1" },
);
