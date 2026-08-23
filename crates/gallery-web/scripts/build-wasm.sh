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
cargo build -p gallery-web --target wasm32-unknown-unknown "${RELEASE_FLAG[@]}"

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

# Binaryen shrinks the bindgen output further. CI installs it; locally it is
# optional, and the build still produces a working artifact without it — just a
# larger one. Run it after wasm-bindgen so its custom sections are already in
# place, and never fail the build on it: a feature-flag mismatch with an older
# binaryen should cost size, not the artifact.
if [[ "$PROFILE" == "wasm-release" ]]; then
  BUNDLE="$OUT/gallery_web_bg.wasm"
  if command -v wasm-opt >/dev/null 2>&1; then
    BEFORE="$(wc -c < "$BUNDLE")"
    if wasm-opt -Oz -all -o "$BUNDLE.opt" "$BUNDLE" 2>/dev/null; then
      mv "$BUNDLE.opt" "$BUNDLE"
      AFTER="$(wc -c < "$BUNDLE")"
      printf 'wasm-opt -Oz: %s -> %s bytes\n' "$BEFORE" "$AFTER"
    else
      rm -f "$BUNDLE.opt"
      printf 'warning: wasm-opt failed; keeping the unoptimized artifact (%s bytes)\n' "$BEFORE" >&2
    fi
  else
    printf 'wasm-opt not found; skipping size optimization. Install binaryen for release-sized output.\n'
  fi
fi
