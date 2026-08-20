#!/usr/bin/env bash

# Pure parsing and validation helpers for update-upstream.sh. Keeping these
# free of network and Cargo side effects makes the dependency contract easy to
# test with small fixture files.

manifest_component_revs() {
  local manifest="${1:-Cargo.toml}"
  sed -nE \
    '/^[[:space:]]*(gpui-component(-assets)?|gpui-base)[[:space:]]*=/ s/.*rev[[:space:]]*=[[:space:]]*"([0-9a-f]{40})".*/\1/p' \
    "$manifest"
}

check_manifest_pair() {
  local manifest="${1:-Cargo.toml}"
  local revisions=()
  mapfile -t revisions < <(manifest_component_revs "$manifest")

  if [[ "${#revisions[@]}" -ne 3 ]]; then
    printf 'error: expected exact revisions for gpui-component, gpui-component-assets, and gpui-base in %s\n' "$manifest" >&2
    return 1
  fi
  if [[ "${revisions[0]}" != "${revisions[1]}" || "${revisions[0]}" != "${revisions[2]}" ]]; then
    printf 'error: gpui-component revisions differ in %s\n' "$manifest" >&2
    return 1
  fi

  printf '%s\n' "${revisions[0]}"
}

update_manifest_component_revs() {
  local manifest="$1"
  local revision="$2"
  sed -i.bak -E \
    "s#((gpui-component(-assets)?|gpui-base)[[:space:]]*=[[:space:]]*\\{[^}]*rev[[:space:]]*=[[:space:]]*\")[0-9a-f]+(\")#\\1$revision\\4#" \
    "$manifest"
  rm -f "$manifest.bak"
}

lock_package_rev() {
  local lock_file="$1"
  local package="$2"
  awk -v package="$package" '
    $0 == "[[package]]" { in_package = 0 }
    $0 == "name = \"" package "\"" { in_package = 1 }
    in_package && /^source = / {
      source = $0
      sub(/^source = "[^"]*#/, "", source)
      sub(/".*$/, "", source)
      print source
      exit
    }
  ' "$lock_file"
}

zed_rev_from_lock() {
  lock_package_rev "$1" "gpui"
}

lock_component_revs() {
  local lock_file="${1:-Cargo.lock}"
  lock_package_rev "$lock_file" "gpui-component"
  lock_package_rev "$lock_file" "gpui-component-assets"
  lock_package_rev "$lock_file" "gpui-base"
}
