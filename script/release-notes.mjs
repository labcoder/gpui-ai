// Print the CHANGELOG section for one version, or the whole release body.
//
//   node script/release-notes.mjs 0.1.0
//   node script/release-notes.mjs 0.1.0 --release-body
//
// The plain form prints the section body, ready to paste. `--release-body`
// fills .github/release-template.md with that section and the upstream
// revision pair this release was built against.

import { readFileSync } from "node:fs";

const CHANGELOG = new URL("../CHANGELOG.md", import.meta.url);
const TEMPLATE = new URL("../.github/release-template.md", import.meta.url);
const MANIFEST = new URL("../Cargo.toml", import.meta.url);
const LOCKFILE = new URL("../Cargo.lock", import.meta.url);

const SECTION_HEADING = /^## \[([^\]]+)\](?:\s*-\s*(.+))?\s*$/;
const LINK_REFERENCE = /^\[[^\]]+\]:\s/;

function fail(message) {
  process.stderr.write(`${message}\n`);
  process.exit(1);
}

function parseArguments(argv) {
  const flags = new Set(argv.filter((argument) => argument.startsWith("--")));
  const positional = argv.filter((argument) => !argument.startsWith("--"));

  if (positional.length !== 1) {
    fail("usage: node script/release-notes.mjs <version> [--release-body]");
  }
  for (const flag of flags) {
    if (flag !== "--release-body") fail(`unknown option: ${flag}`);
  }

  return { version: positional[0].replace(/^v/, ""), releaseBody: flags.has("--release-body") };
}

// Returns the section body plus the date recorded in its heading.
function changelogSection(version) {
  const lines = readFileSync(CHANGELOG, "utf8").split(/\r?\n/);
  const headings = lines
    .map((line, index) => ({ match: SECTION_HEADING.exec(line), index }))
    .filter((entry) => entry.match !== null);

  const heading = headings.find((entry) => entry.match[1] === version);
  if (!heading) {
    const known = headings.map((entry) => entry.match[1]).join(", ");
    fail(`no section for ${version} in CHANGELOG.md (found: ${known})`);
  }

  const next = headings.find((entry) => entry.index > heading.index);
  const body = lines.slice(heading.index + 1, next ? next.index : lines.length);

  while (body.length > 0 && (body.at(-1).trim() === "" || LINK_REFERENCE.test(body.at(-1)))) {
    body.pop();
  }
  while (body.length > 0 && body[0].trim() === "") body.shift();

  return { body: body.join("\n"), date: heading.match[2]?.trim() ?? "" };
}

// The upstream versions a release supports, read from the resolved graph.
// `Cargo.lock` rather than the manifest, because the manifest states a
// requirement ("0.6") and the lockfile states what was actually built.
function upstreamPin() {
  const lockfile = readFileSync(LOCKFILE, "utf8");
  const locked = (name) => {
    const found = new RegExp(`\\[\\[package\\]\\]\\nname = "${name}"\\nversion = "([^"]+)"`).exec(
      lockfile,
    );
    if (!found) fail(`could not read the locked ${name} version from Cargo.lock`);
    return found[1];
  };

  return { component: locked("gpui-component"), gpui: locked("gpui-pre") };
}

const { version, releaseBody } = parseArguments(process.argv.slice(2));
const section = changelogSection(version);

if (section.date.toLowerCase() === "unreleased") {
  process.stderr.write(
    `warning: ${version} is still marked unreleased in CHANGELOG.md; set its date before tagging\n`,
  );
}

if (!releaseBody) {
  process.stdout.write(`${section.body}\n`);
} else {
  const pin = upstreamPin();
  const filled = readFileSync(TEMPLATE, "utf8")
    .replace(/<!-- version -->/g, version)
    .replace(/<!-- release-notes -->/g, section.body)
    .replace(/<!-- gpui-component-version -->/g, pin.component)
    .replace(/<!-- gpui-pre-version -->/g, pin.gpui);

  const unfilled = /<!-- (version|release-notes|gpui-component-version|gpui-pre-version) -->/.exec(
    filled,
  );
  if (unfilled) fail(`release template still contains ${unfilled[0]}`);

  process.stdout.write(filled.endsWith("\n") ? filled : `${filled}\n`);
}
