import { spawnSync } from 'node:child_process';

const result = spawnSync(
  process.execPath,
  ['script/run-bash.mjs', 'script/test-update-upstream.sh'],
  { cwd: process.cwd(), encoding: 'utf8' },
);

if (result.status !== 0) {
  process.stderr.write(result.stderr || result.stdout);
  process.exit(result.status ?? 1);
}

if (!result.stdout.includes('upstream script tests passed')) {
  process.stderr.write('Bash runner did not execute the requested repository script.\n');
  process.exit(1);
}

process.stdout.write('Bash runner test passed\n');
