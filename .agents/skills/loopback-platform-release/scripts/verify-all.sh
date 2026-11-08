#!/bin/sh
# Standard verification workflow for loopback-mcp-proxy:
# 1. Frontend TypeScript check & Vite bundle build (web/dist)
# 2. Host toolchain Rust unit & integration test suite (cargo test)
# 3. Optional (--release): Static musl Linux release build & binary inspection
set -eu

ROOT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")/../../../.." && pwd)
cd "$ROOT_DIR"

echo "==> [1/3] Verifying Web UI TypeScript types and building web/dist..."
(
  cd web
  npx tsc --noEmit
  npm run build
)

if [ ! -f "web/dist/index.html" ]; then
  echo "error: web/dist/index.html was not generated" >&2
  exit 1
fi

echo "==> [2/3] Running Rust unit & integration tests on host toolchain..."
cargo test

if [ "${1:-}" = "--release" ]; then
  echo "==> [3/3] Building static Linux musl release via scripts/release-musl.sh..."
  ./scripts/release-musl.sh
  for bin in build/linux-*/release/loopback-mcp-proxy; do
    if [ -f "$bin" ]; then
      echo "==> Built artifact: $bin"
      if command -v file >/dev/null 2>&1; then
        file "$bin"
      fi
      if command -v ldd >/dev/null 2>&1; then
        ldd "$bin" 2>&1 || true
      fi
    fi
  done
else
  echo "==> [3/3] Skipping musl release build (pass --release to run scripts/release-musl.sh)."
fi

echo "==> All verification checks passed."
