#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$REPO_ROOT"

if ! command -v cargo >/dev/null 2>&1; then
  echo "Error: cargo is not installed or not on PATH. Install Rust via rustup." >&2
  exit 1
fi

echo "==> Initializing llm-meter"
cargo run -- init

echo "==> Launching llm-meter TUI"
cargo run -- tui
