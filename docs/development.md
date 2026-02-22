# Development Guide

## Prerequisites
- Rust toolchain (`rustup`)
- `cargo`

## Build and Run

```bash
cargo build
cargo run -- init
cargo run -- tui
```

Helper script:

```bash
./scripts/run-app.sh
```

## Local Quality Gate
Use the project script:

```bash
./scripts/test-local.sh
```

It runs:
1. `cargo fmt --all -- --check`
2. `cargo clippy --all-targets --all-features -- -D warnings`
3. `cargo test`
4. `cargo build`
5. smoke run (`init` + `export`) using temp `LLM_METER_HOME`

## Tests
- Unit tests in `src/*` modules
- CLI integration tests in `tests/cli.rs`

Run all tests directly:

```bash
cargo test
```

## Repository Basics
- Primary branch model currently includes `main` and `develop`.
- Keep commits focused and descriptive.
- Validate locally with `./scripts/test-local.sh` before opening a PR.

## Recommended Change Workflow
1. Branch from `develop`
2. Implement change with tests
3. Run local quality gate
4. Commit with clear message
5. Push and open PR
