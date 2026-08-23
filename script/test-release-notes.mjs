import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';

function releaseNotes(...args) {
  return spawnSync(process.execPath, ['script/release-notes.mjs', ...args], {
    cwd: process.cwd(),
    encoding: 'utf8',
  });
}

function check(condition, message) {
  if (condition) return;
  process.stderr.write(`${message}\n`);
  process.exit(1);
}

const section = releaseNotes('0.1.0');
check(section.status === 0, `release-notes 0.1.0 exited ${section.status}: ${section.stderr}`);
check(
  section.stdout.includes('Thirty-four components'),
  'The 0.1.0 section is missing its component summary.',
);
check(
  section.stdout.includes('### Known limitations'),
  'The 0.1.0 section is missing its known limitations.',
);
check(
  !section.stdout.includes('## ['),
  'The printed section leaked an adjacent version heading.',
);
check(
  !/^\[[^\]]+\]:\s/m.test(section.stdout),
  'The printed section leaked the changelog link references.',
);

// The warning is conditional, so it keeps passing once the release is dated.
const changelog = readFileSync(new URL('../CHANGELOG.md', import.meta.url), 'utf8');
const dated = /^## \[0\.1\.0\](?:\s*-\s*(.+))?$/m.exec(changelog)?.[1]?.trim().toLowerCase();
if (dated === 'unreleased') {
  check(
    section.stderr.includes('still marked unreleased'),
    'An undated release must warn before it can be tagged.',
  );
}

const prefixed = releaseNotes('v0.1.0');
check(prefixed.status === 0, 'A leading v must resolve to the same section.');
check(prefixed.stdout === section.stdout, 'v0.1.0 and 0.1.0 printed different sections.');

const missing = releaseNotes('9.9.9');
check(missing.status !== 0, 'An unknown version must fail.');
check(
  missing.stderr.includes('9.9.9') && missing.stderr.includes('0.1.0'),
  'An unknown version must report what the changelog does contain.',
);

const body = releaseNotes('0.1.0', '--release-body');
check(body.status === 0, `release-body exited ${body.status}: ${body.stderr}`);
check(body.stdout.includes('# gpui-ai 0.1.0'), 'The release body did not resolve the version.');
check(body.stdout.includes('tag = "v0.1.0"'), 'The release body did not resolve the install tag.');
check(
  body.stdout.includes('### Known limitations'),
  'The release body did not include the changelog section.',
);
check(
  (body.stdout.match(/\b[0-9a-f]{40}\b/g) ?? []).length === 2,
  'The release body must name the gpui-component and zed revisions.',
);
check(!body.stdout.includes('<!--'), 'The release body left an unfilled template marker.');

const unknownOption = releaseNotes('0.1.0', '--publish');
check(unknownOption.status !== 0, 'An unknown option must fail rather than be ignored.');

process.stdout.write('Release notes tests passed\n');
