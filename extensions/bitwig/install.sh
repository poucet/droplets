#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"

DEST="${BITWIG_EXTENSIONS_DIR:-$HOME/Documents/Bitwig Studio/Extensions}"
OUT="build/Droplets.bwextension"

./build.sh

mkdir -p "$DEST"
cp "$OUT" "$DEST/"
echo "✓ installed to $DEST/Droplets.bwextension"
echo "  reload: Bitwig → Settings → Controllers → (extension auto-reloads)"
