#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
if ! command -v cargo >/dev/null 2>&1; then
  for candidate in "$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin" "$HOME/.cargo/bin"; do
    if [ -x "$candidate/cargo" ]; then export PATH="$candidate:$PATH"; break; fi
  done
fi
exec cargo "$@"

