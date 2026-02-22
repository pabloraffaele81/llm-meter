# llm-meter

Terminal-first LLM usage and cost monitor with a live TUI, provider connection testing, and local snapshot storage.

## What it does
- Polls provider usage APIs (OpenAI, Anthropic)
- Calculates cost from pricing rules
- Stores snapshots in SQLite
- Shows dashboard + provider management in a Ratatui interface
- Exports cost data as JSON or CSV

## Prerequisites
- Rust toolchain (`rustup`, `cargo`)

## Quick Start
From repository root:

```bash
cargo run -- init
cargo run -- add-provider openai --api-key "$OPENAI_API_KEY"
cargo run -- tui
```

Optional helper script:

```bash
./scripts/run-app.sh
```

## Core Commands
```bash
cargo run -- init
cargo run -- add-provider <provider> --api-key <key> [--base-url <url>] [--organization-id <id>]
cargo run -- refresh --window 1d|7d|30d
cargo run -- export --format json|csv
cargo run -- tui
```

## Configuration and Secrets
- `LLM_METER_HOME` overrides app home (useful in CI or smoke runs).
- Default home uses OS app dirs; fallback is local `.llm-meter/` if needed.
- API key lookup order:
1. OS keychain entry (`llm-meter` service)
2. Env var (`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, etc.)

## TUI Highlights
- Dashboard: `r` refresh, `1/7/3` window, `a` focus actions, `q` quit prompt
- Provider manager: `n` add, `Enter` edit, `t` test, `e` enable/disable, `k` delete key, `d` remove provider
- Provider form: `t` test connection, `x` clear test logs, `v` advanced fields, `i` error details

## Local Validation
```bash
./scripts/test-local.sh
```
This runs format check, clippy, tests, build, and a smoke run.

## Documentation
- `docs/README.md` (index)
- `docs/cli.md`
- `docs/tui.md`
- `docs/configuration.md`
- `docs/architecture.md`
- `docs/development.md`
- `docs/troubleshooting.md`
