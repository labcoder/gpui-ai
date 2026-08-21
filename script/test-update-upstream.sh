#!/usr/bin/env bash

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# This source is intentionally written before the helper exists: the first
# TDD run proves the updater contract is absent.
source "$ROOT/script/upstream-lib.sh"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

assert_eq() {
  local expected="$1"
  local actual="$2"
  local message="$3"
  if [[ "$expected" != "$actual" ]]; then
    printf 'FAIL: %s\nexpected: %s\nactual:   %s\n' "$message" "$expected" "$actual" >&2
    exit 1
  fi
}

assert_file_contains() {
  local file="$1"
  local expected="$2"
  local message="$3"
  if ! grep -Fqx "$expected" "$file"; then
    printf 'FAIL: %s\nexpected line: %s\n' "$message" "$expected" >&2
    exit 1
  fi
}

write_manifest() {
  local ui_rev="$1"
  local assets_rev="$2"
  local base_rev="$3"
  cat > "$TMP_DIR/Cargo.toml" <<EOF
[workspace.metadata.decoy]
gpui-base = { git = "https://github.com/longbridge/gpui-component", rev = "dddddddddddddddddddddddddddddddddddddddd" }

[workspace.dependencies]
# gpui-base = { git = "https://github.com/longbridge/gpui-component", rev = "dddddddddddddddddddddddddddddddddddddddd" }
gpui-component = { git = "https://github.com/longbridge/gpui-component", rev = "$ui_rev" }
gpui-component-assets = { git = "https://github.com/longbridge/gpui-component", rev = "$assets_rev" }
gpui-base = { git = "https://github.com/longbridge/gpui-component", rev = "$base_rev" }
custom-gpui-base = { git = "https://github.com/longbridge/gpui-component", rev = "dddddddddddddddddddddddddddddddddddddddd" }

[profile.dev.package.decoy]
gpui-component = { git = "https://github.com/longbridge/gpui-component", rev = "dddddddddddddddddddddddddddddddddddddddd" }
EOF
}

write_manifest "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
manifest_revs=()
while IFS= read -r revision; do
  manifest_revs+=("$revision")
done < <(manifest_component_revs "$TMP_DIR/Cargo.toml")
assert_eq "3" "${#manifest_revs[@]}" "all component-stack dependencies must be parsed"
assert_eq "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" "${manifest_revs[0]}" "ui revision must be returned"
assert_eq "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" "${manifest_revs[1]}" "assets revision must be returned"
assert_eq "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" "${manifest_revs[2]}" "base revision must be returned"

write_manifest "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb" "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
if check_manifest_pair "$TMP_DIR/Cargo.toml" >/dev/null 2>&1; then
  printf 'FAIL: mismatched component revisions were accepted\n' >&2
  exit 1
fi

write_manifest "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
if check_manifest_pair "$TMP_DIR/Cargo.toml" >/dev/null 2>&1; then
  printf 'FAIL: mismatched gpui-base revision was accepted\n' >&2
  exit 1
fi

write_manifest "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
update_manifest_component_revs "$TMP_DIR/Cargo.toml" "cccccccccccccccccccccccccccccccccccccccc"
updated_revs=()
while IFS= read -r revision; do
  updated_revs+=("$revision")
done < <(manifest_component_revs "$TMP_DIR/Cargo.toml")
assert_eq "cccccccccccccccccccccccccccccccccccccccc" "${updated_revs[0]}" "component revision must update"
assert_eq "cccccccccccccccccccccccccccccccccccccccc" "${updated_revs[1]}" "assets revision must update"
assert_eq "cccccccccccccccccccccccccccccccccccccccc" "${updated_revs[2]}" "base revision must update"
assert_file_contains "$TMP_DIR/Cargo.toml" '# gpui-base = { git = "https://github.com/longbridge/gpui-component", rev = "dddddddddddddddddddddddddddddddddddddddd" }' "comments must not be rewritten"
assert_file_contains "$TMP_DIR/Cargo.toml" 'custom-gpui-base = { git = "https://github.com/longbridge/gpui-component", rev = "dddddddddddddddddddddddddddddddddddddddd" }' "suffix-matched keys must not be rewritten"
assert_file_contains "$TMP_DIR/Cargo.toml" 'gpui-component = { git = "https://github.com/longbridge/gpui-component", rev = "dddddddddddddddddddddddddddddddddddddddd" }' "other TOML tables must not be rewritten"

cat > "$TMP_DIR/Cargo.lock" <<'EOF'
[[package]]
name = "gpui"
version = "0.2.2"
source = "git+https://github.com/zed-industries/zed#cccccccccccccccccccccccccccccccccccccccc"
EOF

assert_eq \
  "cccccccccccccccccccccccccccccccccccccccc" \
  "$(zed_rev_from_lock "$TMP_DIR/Cargo.lock")" \
  "the complete GPUI source revision must be parsed"

write_component_stack_lock() {
  local revision="$1"
  cat > "$TMP_DIR/component-stack.lock" <<EOF
[[package]]
name = "gpui-component"
source = "git+https://github.com/longbridge/gpui-component?rev=$revision#$revision"

[[package]]
name = "gpui-component-assets"
source = "git+https://github.com/longbridge/gpui-component?rev=$revision#$revision"

[[package]]
name = "gpui-base"
source = "git+https://github.com/longbridge/gpui-component?rev=$revision#$revision"
EOF
}

write_component_stack_lock "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
check_component_stack_lock "$TMP_DIR/component-stack.lock" "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"

cat >> "$TMP_DIR/component-stack.lock" <<'EOF'

[[package]]
name = "gpui-base"
source = "git+https://github.com/longbridge/gpui-component?rev=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa#aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
EOF
if check_component_stack_lock "$TMP_DIR/component-stack.lock" "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" >/dev/null 2>&1; then
  printf 'FAIL: duplicate gpui-base package source was accepted\n' >&2
  exit 1
fi

write_component_stack_lock "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
cat >> "$TMP_DIR/component-stack.lock" <<'EOF'

[[package]]
name = "gpui-base"
version = "0.5.2"
EOF
if check_component_stack_lock "$TMP_DIR/component-stack.lock" "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" >/dev/null 2>&1; then
  printf 'FAIL: source-less duplicate gpui-base package record was accepted\n' >&2
  exit 1
fi

write_component_stack_lock "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
sed -i.bak 's#https://github.com/longbridge/gpui-component?rev=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa#https://example.invalid/gpui-component?rev=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa#' "$TMP_DIR/component-stack.lock"
rm -f "$TMP_DIR/component-stack.lock.bak"
if check_component_stack_lock "$TMP_DIR/component-stack.lock" "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" >/dev/null 2>&1; then
  printf 'FAIL: split component source repository was accepted\n' >&2
  exit 1
fi

mkdir -p "$TMP_DIR/bin"
cat > "$TMP_DIR/bin/curl" <<EOF
#!/usr/bin/env bash
set -euo pipefail
destination=""
while [[ "\$#" -gt 0 ]]; do
  if [[ "\$1" == "-o" ]]; then
    destination="\$2"
    shift 2
  else
    shift
  fi
done
cp "$ROOT/Cargo.lock" "\$destination"
EOF
chmod +x "$TMP_DIR/bin/curl"

check_output="$(PATH="$TMP_DIR/bin:$PATH" bash "$ROOT/script/update-upstream.sh" --check)"
if [[ "$check_output" != *"upstream revisions are synchronized"* ]]; then
  printf 'FAIL: read-only check did not report synchronized revisions\n' >&2
  exit 1
fi

printf 'upstream script tests passed\n'
