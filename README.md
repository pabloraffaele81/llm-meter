# llm-meter

A clean terminal dashboard for online LLM token and cost monitoring.

## Features (MVP)
- Rust + Ratatui TUI
- Online usage polling
- OpenAI + Anthropic adapters
- Provider-agnostic adapter trait for incremental "any model" support
- OS keychain API key storage
- SQLite snapshots
- JSON/CSV export

## Install prerequisites
- Rust toolchain (`rustup`)

## Build
```bash
cd tools/llm-meter
cargo build
```

## Quick start
```bash
cd tools/llm-meter
cargo run -- init
cargo run -- add-provider openai --api-key "$OPENAI_API_KEY"
cargo run -- add-provider anthropic --api-key "$ANTHROPIC_API_KEY"
cargo run -- tui
```

## Configuration paths
- Default: OS app data/config directories.
- Override with `LLM_METER_HOME=/path/to/dir` (useful for CI/sandboxes).
- If OS path is not writable, it falls back to local `.llm-meter/`.

## API key resolution order
1. OS keychain
2. Environment variable (`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, etc.)

## Commands
- `llm-meter init`
- `llm-meter add-provider <provider> --api-key <key> [--base-url <url>] [--organization-id <id>]`
- `llm-meter refresh --window 1d|7d|30d`
- `llm-meter export --format json|csv`
- `llm-meter tui`

## Keybindings
- `a` focus visible actions panel
- `r` refresh now
- `1` one-day window
- `7` seven-day window
- `3` thirty-day window
- `q` open quit confirmation
- `z` toggle compact mode
- `Tab` and `Shift+Tab` navigate form fields
- `Esc` close modal/back
- `Up`/`Down` + `Enter` run focused action

## In-TUI provider management
- Open Actions (`a`) and select `Manage providers/keys`
- `n` add provider
- `Enter` edit selected provider
- `t` enable/disable selected provider
- `k` delete selected provider key
- `d` remove provider and key

## Notes
- Provider usage API schemas can change; adapter parsing intentionally tolerates missing fields.
- Built-in pricing defaults are in `src/pricing.rs` and can be overridden in config.
