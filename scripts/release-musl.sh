#!/bin/sh
# Linux release binary: musl, statically linked.
# amd64 and arm64 share this recipe. darwin and windows are a later sync of it.
set -eu

host_arch() {
  case "$(uname -m)" in
    x86_64) echo amd64 ;;
    aarch64 | arm64) echo arm64 ;;
    *)
      echo "release: unsupported host architecture $(uname -m)" >&2
      exit 1
      ;;
  esac
}

if [ "${1:-}" = "" ]; then
  arch=$(host_arch)
else
  arch=$1
fi

case "$arch" in
  amd64) rust_arch=x86_64 ;;
  arm64) rust_arch=aarch64 ;;
  *)
    echo "usage: release-musl.sh [amd64|arm64]" >&2
    exit 2
    ;;
esac

triple=${rust_arch}-unknown-linux-musl
native=$(host_arch)

if [ -n "${MUSL_CC:-}" ]; then
  cc=$MUSL_CC
elif [ "$arch" = "$native" ]; then
  cc=musl-gcc
else
  cc=${rust_arch}-linux-musl-gcc
fi

if ! command -v "$cc" >/dev/null 2>&1; then
  echo "release: C compiler '$cc' not found for linux-$arch" >&2
  if [ "$arch" = "$native" ]; then
    echo "release: install musl-gcc (Debian/Ubuntu package: musl-tools)" >&2
  else
    echo "release: cross builds need ${rust_arch}-linux-musl-gcc, or set MUSL_CC" >&2
  fi
  exit 1
fi

rustup target add "$triple"

# libsqlite3-sys compiles sqlite3.c. Point that C compiler at musl.
cc_var=$(printf 'CC_%s' "$triple" | tr 'A-Z-' 'a-z_')
export "$cc_var=$cc"

cargo build --release --target "$triple"

dest="build/linux-${arch}/release"
mkdir -p "$dest"
cp "target/${triple}/release/loopback-mcp-proxy" "${dest}/loopback-mcp-proxy"
echo "release: ${dest}/loopback-mcp-proxy"
