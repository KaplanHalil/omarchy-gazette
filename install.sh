#!/usr/bin/env bash
# Build and install the Gazette engine into ~/.local/bin.
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_DIR="${GAZETTE_BIN_DIR:-$HOME/.local/bin}"

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is required (rustup or pacman -S rust)" >&2
  exit 1
fi

echo "Building gazette-engine…"
cargo build --release --locked --manifest-path "$DIR/Cargo.toml"

install -Dm755 "$DIR/target/release/gazette-engine" "$BIN_DIR/gazette-engine"
echo "Installed $BIN_DIR/gazette-engine"
echo
echo "Next:"
echo "  omarchy plugin add <this repository> --enable"
echo "  omarchy restart shell"
