#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

npm run build
npm --prefix frontend run check:content
npm --prefix frontend run check:viewport
npm --prefix frontend run check:music
npm --prefix frontend run check:qq
node --experimental-strip-types frontend/scripts/check-bokeh-optimization.mjs
node --experimental-strip-types frontend/scripts/check-cover-cache.mjs
node --experimental-strip-types frontend/scripts/check-instance-visibility.mjs
node --experimental-strip-types --test frontend/scripts/check-frame-work.mjs frontend/scripts/check-frame-candidate-validation.mjs
node --experimental-strip-types --test frontend/scripts/check-motion-resolution.mjs
./scripts/cargo.sh test --locked --manifest-path qq-connector/Cargo.toml
./scripts/cargo.sh test --locked --manifest-path src-tauri/Cargo.toml

# The opt-in packaging code also compiles/tests without opening an app or device.
./scripts/cargo.sh test --locked --manifest-path src-tauri/Cargo.toml --features preview

# Test the Swift empty-read policy without MusicKit authorization or user data.
apple_refresh_test_dir=$(mktemp -d)
trap 'rm -rf "$apple_refresh_test_dir"' EXIT
xcrun swiftc -swift-version 5 -parse-as-library src-tauri/native/AppleMusic.swift scripts/check-apple-refresh.swift -o "$apple_refresh_test_dir/check"
"$apple_refresh_test_dir/check"
