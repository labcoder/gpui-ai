#!/usr/bin/env bash

# Pure parsing and validation helpers for update-upstream.sh. Keeping these
# free of network and Cargo side effects makes the dependency contract easy to
# test with small fixture files.

manifest_component_revs() {
  local manifest="${1:-Cargo.toml}"
  manifest_component_records "$manifest" | awk '{ print $2 }'
}

manifest_component_records() {
  local manifest="${1:-Cargo.toml}"
  awk '
    /^[[:space:]]*\[workspace\.dependencies\][[:space:]]*(#.*)?$/ {
      in_workspace_dependencies = 1
      next
    }
    /^[[:space:]]*\[/ { in_workspace_dependencies = 0 }
    in_workspace_dependencies && /^[[:space:]]*(gpui-component(-assets)?|gpui-base)[[:space:]]*=/ { print }
  ' "$manifest" | sed -nE \
    's/^[[:space:]]*(gpui-component|gpui-component-assets|gpui-base)[[:space:]]*=[[:space:]]*\{[^}]*rev[[:space:]]*=[[:space:]]*"([0-9a-f]{40})".*/\1 \2/p'
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
  local temporary
  temporary="$(mktemp "${manifest}.XXXXXX")"
  awk -v revision="$revision" '
    /^[[:space:]]*\[workspace\.dependencies\][[:space:]]*(#.*)?$/ {
      in_workspace_dependencies = 1
    }
    /^[[:space:]]*\[/ && $0 !~ /^[[:space:]]*\[workspace\.dependencies\][[:space:]]*(#.*)?$/ {
      in_workspace_dependencies = 0
    }
    in_workspace_dependencies && /^[[:space:]]*(gpui-component(-assets)?|gpui-base)[[:space:]]*=/ {
      sub(/rev[[:space:]]*=[[:space:]]*"[0-9a-f]+"/, "rev = \"" revision "\"")
    }
    { print }
  ' "$manifest" > "$temporary"
  mv "$temporary" "$manifest"
}

lock_package_sources() {
  local lock_file="$1"
  local package="$2"
  awk -v package="$package" '
    $0 == "[[package]]" { in_package = 0 }
    $0 == "name = \"" package "\"" { in_package = 1 }
    in_package && /^source = / {
      source = $0
      sub(/^source = "/, "", source)
      sub(/".*$/, "", source)
      print source
    }
  ' "$lock_file"
}

lock_package_source() {
  local lock_file="$1"
  local package="$2"
  local sources=()
  mapfile -t sources < <(lock_package_sources "$lock_file" "$package")
  if [[ "${#sources[@]}" -ne 1 ]]; then
    printf 'error: expected exactly one %s source in %s\n' "$package" "$lock_file" >&2
    return 1
  fi
  printf '%s\n' "${sources[0]}"
}

lock_package_rev() {
  local source
  source="$(lock_package_source "$1" "$2")" || return 1
  printf '%s\n' "${source##*#}"
}

zed_rev_from_lock() {
  lock_package_rev "$1" "gpui"
}

component_stack_lock_source() {
  local revision="$1"
  printf 'git+https://github.com/longbridge/gpui-component?rev=%s#%s\n' "$revision" "$revision"
}

check_component_stack_lock() {
  local lock_file="$1"
  local revision="$2"
  local expected_source
  expected_source="$(component_stack_lock_source "$revision")"
  local package
  for package in gpui-component gpui-component-assets gpui-base; do
    local sources=()
    mapfile -t sources < <(lock_package_sources "$lock_file" "$package")
    if [[ "${#sources[@]}" -ne 1 ]]; then
      printf 'error: expected exactly one %s source in %s\n' "$package" "$lock_file" >&2
      return 1
    fi
    if [[ "${sources[0]}" != "$expected_source" ]]; then
      printf 'error: %s source %s does not match expected component stack source %s\n' \
        "$package" "${sources[0]}" "$expected_source" >&2
      return 1
    fi
  done
}
