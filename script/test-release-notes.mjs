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
check(
  body.stdout.includes('cargo add gpui-ai@0.1.0'),
  'The release body did not resolve the install version.',
);
check(body.stdout.includes('gpui-component = "'), 'The release body omitted gpui-component.');
check(body.stdout.includes('package = "gpui-pre-platform"'), 'The release body omitted gpui_platform.');
check(body.stdout.includes('Rust 1.89 or newer'), 'The release body has the wrong Rust floor.');
check(body.stdout.includes('## Tested platforms'), 'The release body omitted tested platforms.');
check(body.stdout.includes('## Known limitations'), 'The release body omitted current limitations.');
check(body.stdout.includes('Windows 11'), 'The release body omitted the tested platform.');
check(
  body.stdout.includes('### Known limitations'),
  'The release body did not include the changelog section.',
);
// Each upstream version is named twice: once in the install snippet, once in
// the table. A resolver that quietly stopped filling one would still pass a
// "contains a version" check.
const lockfile = readFileSync(new URL('../Cargo.lock', import.meta.url), 'utf8');
for (const crate of ['gpui-component', 'gpui-pre']) {
  const locked = new RegExp(`\\[\\[package\\]\\]\\nname = "${crate}"\\nversion = "([^"]+)"`).exec(
    lockfile,
  );
  check(locked !== null, `Cargo.lock does not lock ${crate}.`);
  check(
    body.stdout.split(locked[1]).length - 1 >= 2,
    `The release body must name the locked ${crate} version in the snippet and the table.`,
  );
}
check(!body.stdout.includes('<!--'), 'The release body left an unfilled template marker.');

const unknownOption = releaseNotes('0.1.0', '--publish');
check(unknownOption.status !== 0, 'An unknown option must fail rather than be ignored.');

process.stdout.write('Release notes tests passed\n');
