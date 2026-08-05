#!/usr/bin/env bash
set -euo pipefail

# Builds midnight_telemetry_core for iOS device + simulator, generates its
# Swift bindings with uniffi's self-hosted bindgen binary, and assembles an
# XCFramework that the Xcode project links against directly.
#
# Requires macOS + Xcode command line tools. Run from anywhere; paths below
# are resolved relative to this script's location.

cd "$(dirname "$0")/.."

CRATE_NAME=midnight_telemetry_core
FFI_MODULE_NAME="${CRATE_NAME}FFI"
LIB_NAME="lib${CRATE_NAME}.a"

IOS_DIR="../ios"
XCFRAMEWORK_OUT="${IOS_DIR}/Frameworks/RustCoreFFI.xcframework"
GENERATED_SWIFT_DIR="${IOS_DIR}/MidnightTelemetry/Generated"
WORKDIR="target/ios-build"

echo "==> Ensuring iOS targets are installed"
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios

rm -rf "$WORKDIR" "$XCFRAMEWORK_OUT"
mkdir -p "$WORKDIR/headers" "$WORKDIR/sim-universal" "$GENERATED_SWIFT_DIR"

echo "==> Building for device (arm64)"
cargo build --release --target aarch64-apple-ios

echo "==> Building for simulator (arm64, Apple Silicon)"
cargo build --release --target aarch64-apple-ios-sim

echo "==> Building for simulator (x86_64, Intel)"
cargo build --release --target x86_64-apple-ios

echo "==> Generating Swift bindings from the device build's embedded metadata"
cargo run --quiet --features uniffi-bindgen --bin uniffi-bindgen -- generate \
  --library "target/aarch64-apple-ios/release/${LIB_NAME}" \
  --language swift \
  --out-dir "$WORKDIR/bindings"

# Xcode only auto-discovers a Clang module for a static-library XCFramework
# slice when its headers directory contains a file named exactly
# `module.modulemap` — so the uniffi-generated one has to be renamed.
cp "$WORKDIR/bindings/${FFI_MODULE_NAME}.h" "$WORKDIR/headers/"
cp "$WORKDIR/bindings/${FFI_MODULE_NAME}.modulemap" "$WORKDIR/headers/module.modulemap"
cp "$WORKDIR/bindings/${CRATE_NAME}.swift" "$GENERATED_SWIFT_DIR/"

echo "==> Merging simulator slices into one universal static library"
lipo -create \
  "target/aarch64-apple-ios-sim/release/${LIB_NAME}" \
  "target/x86_64-apple-ios/release/${LIB_NAME}" \
  -output "$WORKDIR/sim-universal/${LIB_NAME}"

echo "==> Assembling XCFramework"
xcodebuild -create-xcframework \
  -library "target/aarch64-apple-ios/release/${LIB_NAME}" -headers "$WORKDIR/headers" \
  -library "$WORKDIR/sim-universal/${LIB_NAME}" -headers "$WORKDIR/headers" \
  -output "$XCFRAMEWORK_OUT"

echo "==> Done. Open ios/MidnightTelemetry.xcodeproj and build."
