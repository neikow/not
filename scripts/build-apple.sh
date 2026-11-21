#!/usr/bin/env bash
# Builds not-ffi for macOS and packages it into apple/NotCore (SwiftPM package wrapping an xcframework).
set -euo pipefail
cd "$(dirname "$0")/.."

TARGET=aarch64-apple-darwin
LIB_DIR=target/$TARGET/release
GEN=build/apple/gen
HEADERS=build/apple/headers
PKG=apple/NotCore

export MACOSX_DEPLOYMENT_TARGET=15.0
cargo build -p not-ffi --release --target "$TARGET"

rm -rf build/apple && mkdir -p "$GEN" "$HEADERS"
cargo run -q -p uniffi-bindgen -- generate \
  --library "$LIB_DIR/libnot_ffi.dylib" --language swift --out-dir "$GEN"
cp "$GEN/NotFFI.h" "$HEADERS/"
cp "$GEN/NotFFI.modulemap" "$HEADERS/module.modulemap"

rm -rf "$PKG/NotFFI.xcframework"
xcodebuild -create-xcframework -library "$LIB_DIR/libnot_ffi.a" -headers "$HEADERS" \
  -output "$PKG/NotFFI.xcframework" >/dev/null
# The folder holds only this generated (untracked) file, so a fresh checkout doesn't have it.
mkdir -p "$PKG/Sources/NotCore"
cp "$GEN/not_ffi.swift" "$PKG/Sources/NotCore/Generated.swift"
python3 scripts/licenses.py apple apple/NotMac/licenses.json
echo "apple: $PKG ready"
