#!/usr/bin/env bash

# Fixture for script/test-run-bash.mjs.
#
# The runner exists for one reason: on Windows, `bash` is Git Bash, which cannot
# open `C:\dev\...` — so run-bash.mjs translates the working directory, the
# script, and every argument into paths it can. That is what this checks. It
# has to be a real script run through the real runner; nothing about the
# translation is observable from Node's side.

set -euo pipefail

if [[ ! -f package.json ]]; then
  printf 'FAIL: the runner did not start at the repository root (pwd: %s)\n' "$PWD" >&2
  exit 1
fi

path="${1:?FAIL: no path argument}"
if [[ ! -f "$path" ]]; then
  printf 'FAIL: the runner did not translate the argument: %s\n' "$path" >&2
  exit 1
fi

printf 'bash runner fixture passed\n'
