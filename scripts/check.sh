#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

npm run build
npm --prefix frontend run check:content
npm --prefix frontend run check:viewport
npm --prefix frontend run check:music
node --experimental-strip-types frontend/scripts/check-bokeh-optimization.mjs
node --experimental-strip-types frontend/scripts/check-cover-cache.mjs
node --experimental-strip-types frontend/scripts/check-instance-visibility.mjs
./scripts/cargo.sh test --locked --manifest-path qq-connector/Cargo.toml
./scripts/cargo.sh test --locked --manifest-path src-tauri/Cargo.toml
