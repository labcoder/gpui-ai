import { existsSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import path from 'node:path';

const [script, ...scriptArgs] = process.argv.slice(2);
if (!script) {
  process.stderr.write('usage: node script/run-bash.mjs <script> [args...]\n');
  process.exit(2);
}

function shellQuote(value) {
  return `'${value.replaceAll("'", `'"'"'`)}'`;
}

function gitPath(value) {
  return value
    .replace(/^([A-Za-z]):/, (_, drive) => `/${drive.toLowerCase()}`)
    .replaceAll('\\', '/');
}

let executable = 'bash';
let args = [script, ...scriptArgs];

if (process.platform === 'win32') {
  const candidates = [
    process.env.MIGHTY_GPUI_BASH,
    process.env.ProgramFiles && path.join(process.env.ProgramFiles, 'Git', 'bin', 'bash.exe'),
    process.env.LOCALAPPDATA &&
      path.join(process.env.LOCALAPPDATA, 'Programs', 'Git', 'bin', 'bash.exe'),
  ].filter(Boolean);
  executable = candidates.find(existsSync) ?? 'bash';

  const command = [
    `cd ${shellQuote(gitPath(process.cwd()))}`,
    '&&',
    'bash',
    shellQuote(gitPath(script)),
    ...scriptArgs.map((argument) => shellQuote(gitPath(argument))),
  ].join(' ');
  args = ['-lc', command];
}

const result = spawnSync(executable, args, {
  cwd: process.cwd(),
  stdio: 'inherit',
});

if (result.error) {
  process.stderr.write(`${result.error.message}\n`);
  process.exit(1);
}

process.exit(result.status ?? 1);
