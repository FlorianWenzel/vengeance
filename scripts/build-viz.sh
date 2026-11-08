#!/usr/bin/env bash
# Builds the WebAssembly blueprint renderer (viz/) into web/src/viz/pkg/ for the Vue app.
# Requires: rustup target wasm32-unknown-unknown and the wasm-bindgen CLI matching the crate's
# pinned wasm-bindgen version (see viz/Cargo.toml). wasm-opt (binaryen) is used when available.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$ROOT/web/src/viz/pkg"
WASM="$ROOT/viz/target/wasm32-unknown-unknown/release/vengeance_blueprint.wasm"

WASM_BINDGEN="${WASM_BINDGEN:-$(command -v wasm-bindgen || echo "$HOME/.cargo/bin/wasm-bindgen")}"
PINNED="$(sed -n 's/^wasm-bindgen = "=\([0-9.]*\)".*/\1/p' "$ROOT/viz/Cargo.toml")"
if [ ! -x "$WASM_BINDGEN" ]; then
  echo "wasm-bindgen CLI not found (cargo install wasm-bindgen-cli --version $PINNED)" >&2
  exit 1
fi
CLI="$("$WASM_BINDGEN" --version | awk '{print $2}')"
if [ -n "$PINNED" ] && [ "$PINNED" != "$CLI" ]; then
  echo "wasm-bindgen CLI is $CLI but viz/Cargo.toml pins $PINNED" >&2
  exit 1
fi

cargo build --manifest-path "$ROOT/viz/Cargo.toml" --release --target wasm32-unknown-unknown
mkdir -p "$OUT"
"$WASM_BINDGEN" --target web --out-dir "$OUT" --out-name blueprint "$WASM"

if command -v wasm-opt >/dev/null 2>&1; then
  wasm-opt -Oz --all-features -o "$OUT/blueprint_bg.wasm" "$OUT/blueprint_bg.wasm"
fi

echo "blueprint: $(wc -c < "$OUT/blueprint_bg.wasm" | tr -d ' ') bytes -> ${OUT#"$ROOT"/}"
