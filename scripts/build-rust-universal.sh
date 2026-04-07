#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")/.."  # repo root (diskmap/)

echo "Building dm-ffi for aarch64-apple-darwin..."
cargo build --release -p dm-ffi --target aarch64-apple-darwin

echo "Building dm-ffi for x86_64-apple-darwin..."
cargo build --release -p dm-ffi --target x86_64-apple-darwin

echo "Creating universal binary..."
mkdir -p target/universal/release
lipo -create \
    target/aarch64-apple-darwin/release/libdm_ffi.a \
    target/x86_64-apple-darwin/release/libdm_ffi.a \
    -output target/universal/release/libdm_ffi.a

echo "Verifying..."
lipo -info target/universal/release/libdm_ffi.a

echo "Done: target/universal/release/libdm_ffi.a"
echo "Header: crates/dm-ffi/dm_ffi.h"
