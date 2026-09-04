import { spawnSync } from 'node:child_process';
import path from 'node:path';

// A native path with the platform's own separators, which is what a caller
// actually has. On Windows this is the thing Git Bash cannot open, so handing
// it through the runner is the test.
const nativePath = path.join(process.cwd(), 'package.json');

const result = spawnSync(
  process.execPath,
  ['script/run-bash.mjs', 'script/test-run-bash.sh', nativePath],
  { cwd: process.cwd(), encoding: 'utf8' },
);

if (result.status !== 0) {
  process.stderr.write(result.stderr || result.stdout);
  process.exit(result.status ?? 1);
}

if (!result.stdout.includes('bash runner fixture passed')) {
  process.stderr.write('Bash runner did not execute the requested repository script.\n');
  process.exit(1);
}

process.stdout.write('Bash runner test passed\n');
