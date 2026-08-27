#!/usr/bin/env bash

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
PROFILE="debug"
RELEASE_FLAG=()
if [[ "${1:-}" == "--release" ]]; then
  # The size-tuned browser profile, not the frame-time-tuned native one.
  PROFILE="wasm-release"
  RELEASE_FLAG=(--profile wasm-release)
elif [[ "$#" -gt 0 ]]; then
  printf 'usage: %s [--release]\n' "$0" >&2
  exit 2
fi

cd "$ROOT/crates/gallery-web"
cargo build --locked -p gallery-web --target wasm32-unknown-unknown "${RELEASE_FLAG[@]}"

WASM="$ROOT/target/wasm32-unknown-unknown/$PROFILE/gallery_web.wasm"
OUT="$ROOT/crates/gallery-web/www/src/wasm"
if [[ ! -f "$WASM" ]]; then
  printf 'error: expected WASM artifact at %s\n' "$WASM" >&2
  exit 1
fi

BINDGEN_WASM="$WASM"
BINDGEN_OUT="$OUT"
if command -v cygpath >/dev/null 2>&1; then
  BINDGEN_WASM="$(cygpath -w "$WASM")"
  BINDGEN_OUT="$(cygpath -w "$OUT")"
fi

wasm-bindgen "$BINDGEN_WASM" --out-dir "$BINDGEN_OUT" --target web --no-typescript
printf 'generated browser bindings in %s\n' "$OUT"

# The tested artifact is exactly wasm-bindgen's output. Binaryen 108 and 132
# both produced non-instantiable modules here. Never select a different build
# pipeline implicitly because an optional executable happens to be on PATH.
# Re-enable optimization only as an explicit, pinned, browser-verified change.
printf 'wasm-opt disabled; using the same bindgen artifact locally and in CI.\n'
