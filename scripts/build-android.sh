#!/usr/bin/env bash
# Cross-compiles not-ffi for Android and generates Kotlin bindings into the app module.
set -euo pipefail
cd "$(dirname "$0")/.."

export ANDROID_HOME=${ANDROID_HOME:-$HOME/Library/Android/sdk}
APP=android/app/src/main
ABIS=${ABIS:-arm64-v8a}

ndk_args=()
for abi in $ABIS; do ndk_args+=(-t "$abi"); done
cargo ndk "${ndk_args[@]}" -P 29 -o "$APP/jniLibs" build -p not-ffi --release

# Bindings come from host-built metadata; they are identical for every target.
cargo build -q -p not-ffi --release
case "$(uname)" in Darwin) host_lib=libnot_ffi.dylib ;; *) host_lib=libnot_ffi.so ;; esac
rm -rf "$APP/java/dev/lysen/not/core"
cargo run -q -p uniffi-bindgen -- generate \
  --library "target/release/$host_lib" --language kotlin --no-format --out-dir "$APP/java"
echo "android: bindings and jniLibs ($ABIS) ready"
