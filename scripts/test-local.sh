#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$REPO_ROOT"

if ! command -v cargo >/dev/null 2>&1; then
  echo "Error: cargo is not installed or not on PATH. Install Rust via rustup." >&2
  exit 1
fi

echo "==> Running format check"
cargo fmt --all -- --check

echo "==> Running clippy"
cargo clippy --all-targets --all-features -- -D warnings

echo "==> Running tests"
cargo test

echo "==> Building application"
cargo build

echo "==> Running application smoke check"
SMOKE_HOME="$(mktemp -d)"
trap 'rm -rf "$SMOKE_HOME"' EXIT

LLM_METER_HOME="$SMOKE_HOME" ./target/debug/llm-meter init
LLM_METER_HOME="$SMOKE_HOME" ./target/debug/llm-meter export --format json >/dev/null

echo "All local checks passed, build succeeded, and application smoke run completed."
