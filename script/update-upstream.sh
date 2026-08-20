#!/usr/bin/env bash
# Synchronize gpui-component, its assets crate, and GPUI to one compatible
# lockfile-backed revision set.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/script/upstream-lib.sh"
cd "$ROOT"

COMPONENT_REPO="https://github.com/longbridge/gpui-component"

usage() {
  cat <<'EOF'
Usage:
  script/update-upstream.sh                 update to gpui-component HEAD
  script/update-upstream.sh <full-rev>      update to a specific full commit
  script/update-upstream.sh --check         verify manifest, lockfile, and upstream GPUI agreement
EOF
}

download_upstream_lock() {
  local component_rev="$1"
  local destination="$2"
  curl -fsSL \
    "https://raw.githubusercontent.com/longbridge/gpui-component/$component_rev/Cargo.lock" \
    -o "$destination"
}

check_local_pair() {
  local manifest_rev
  manifest_rev="$(check_manifest_pair Cargo.toml)"

  local lock_revs=()
  mapfile -t lock_revs < <(lock_component_revs Cargo.lock)
  if [[ "${#lock_revs[@]}" -ne 3 ]]; then
    printf 'error: Cargo.lock does not contain the complete gpui-component stack\n' >&2
    return 1
  fi
  if [[ "${lock_revs[0]}" != "$manifest_rev" || "${lock_revs[1]}" != "$manifest_rev" || "${lock_revs[2]}" != "$manifest_rev" ]]; then
    printf 'error: Cargo.toml and Cargo.lock disagree on the gpui-component stack revision\n' >&2
    return 1
  fi

  local zed_rev
  zed_rev="$(zed_rev_from_lock Cargo.lock)"
  if [[ ! "$zed_rev" =~ ^[0-9a-f]{40}$ ]]; then
    printf 'error: Cargo.lock does not contain a full GPUI/Zed revision\n' >&2
    return 1
  fi

  printf '%s %s\n' "$manifest_rev" "$zed_rev"
}

check_upstream_pair() {
  local local_pair component_rev local_zed_rev
  local_pair="$(check_local_pair)"
  read -r component_rev local_zed_rev <<<"$local_pair"

  local upstream_lock
  upstream_lock="$(mktemp)"
  if ! download_upstream_lock "$component_rev" "$upstream_lock"; then
    rm -f "$upstream_lock"
    return 1
  fi

  local upstream_zed_rev
  upstream_zed_rev="$(zed_rev_from_lock "$upstream_lock")"
  rm -f "$upstream_lock"
  if [[ "$local_zed_rev" != "$upstream_zed_rev" ]]; then
    printf 'error: local GPUI revision %s differs from gpui-component %s pin %s\n' \
      "$local_zed_rev" "$component_rev" "$upstream_zed_rev" >&2
    return 1
  fi

  printf 'upstream revisions are synchronized\ngpui-component: %s\nzed/gpui: %s\n' \
    "$component_rev" "$local_zed_rev"
}

resolve_component_rev() {
  local requested="${1:-}"
  if [[ -n "$requested" ]]; then
    if [[ ! "$requested" =~ ^[0-9a-f]{40}$ ]]; then
      printf 'error: the requested gpui-component revision must be a full 40-character commit\n' >&2
      return 1
    fi
    printf '%s\n' "$requested"
    return
  fi

  git ls-remote "$COMPONENT_REPO" HEAD | cut -f1
}

update_pair() {
  local component_rev
  component_rev="$(resolve_component_rev "${1:-}")"
  if [[ ! "$component_rev" =~ ^[0-9a-f]{40}$ ]]; then
    printf 'error: could not resolve gpui-component HEAD\n' >&2
    return 1
  fi

  local upstream_lock
  upstream_lock="$(mktemp)"
  if ! download_upstream_lock "$component_rev" "$upstream_lock"; then
    rm -f "$upstream_lock"
    return 1
  fi

  local zed_rev
  zed_rev="$(zed_rev_from_lock "$upstream_lock")"
  rm -f "$upstream_lock"
  if [[ ! "$zed_rev" =~ ^[0-9a-f]{40}$ ]]; then
    printf 'error: upstream Cargo.lock does not contain a full GPUI/Zed revision\n' >&2
    return 1
  fi

  printf 'gpui-component: %s\nzed/gpui: %s\n' "$component_rev" "$zed_rev"

  update_manifest_component_revs Cargo.toml "$component_rev"

  cargo update -p gpui-component --precise "$component_rev"
  cargo update -p gpui-component-assets --precise "$component_rev"
  cargo update -p gpui-base --precise "$component_rev"
  cargo update -p gpui --precise "$zed_rev"
  check_local_pair >/dev/null
  cargo check --workspace

  printf 'updated Cargo.toml and Cargo.lock; review the changes before committing\n'
}

main() {
  case "${1:-}" in
    --check)
      if [[ "$#" -ne 1 ]]; then
        usage >&2
        return 2
      fi
      check_upstream_pair
      ;;
    -h|--help)
      usage
      ;;
    *)
      if [[ "$#" -gt 1 ]]; then
        usage >&2
        return 2
      fi
      update_pair "${1:-}"
      ;;
  esac
}

main "$@"
