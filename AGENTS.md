# Repository Guidelines

## Project Structure & Module Organization
This repository is a Rust CLI/TUI application for monitoring LLM token usage and cost.
- `src/main.rs`: CLI entrypoint and subcommands (`init`, `add-provider`, `refresh`, `export`, `tui`).
- `src/ui/`: Ratatui app state and runtime (`app.rs`, `run.rs`).
- `src/providers/`: provider adapters (`openai.rs`, `anthropic.rs`) behind shared traits in `mod.rs`.
- `src/storage.rs`, `src/service.rs`, `src/models.rs`, `src/config.rs`: persistence, orchestration, data models, config/key handling.
- `.llm-meter/`: local runtime data for config and SQLite fallback.
- `target/`: build artifacts (do not edit/commit manually).

## Build, Test, and Development Commands
Use Cargo from repository root:
- `cargo build`: compile the project.
- `cargo run -- init`: initialize config/data directories.
- `cargo run -- tui`: launch the terminal UI.
- `cargo run -- refresh --window 7d`: fetch and persist usage snapshots.
- `cargo run -- export --format json`: export stored cost rows.
- `cargo test`: run unit/integration tests.
- `cargo fmt && cargo clippy -- -D warnings`: enforce formatting and lint quality before PRs.

## Coding Style & Naming Conventions
- Follow Rust 2021 idioms and `rustfmt` defaults (4-space indentation, trailing commas where formatter adds them).
- File/module names: `snake_case` (example: `storage.rs`).
- Types/traits: `PascalCase`; functions/variables: `snake_case`; constants: `SCREAMING_SNAKE_CASE`.
- Keep provider-specific logic inside `src/providers/*`; keep UI state updates in `src/ui/*`.

## Testing Guidelines
- Prefer focused unit tests near implementation using `#[cfg(test)] mod tests`.
- Add integration tests under `tests/` for CLI flows where practical.
- Name tests for observable behavior (example: `refresh_parses_7d_window`).
- Cover new parsing, pricing, storage, and provider error paths for every feature change.

## Commit & Pull Request Guidelines
Git history is not available in this checkout, so use Conventional Commits:
- `feat: add anthropic usage pagination`
- `fix: handle empty export rows`

PRs should include:
- Clear summary and rationale.
- Test evidence (`cargo test`, `cargo clippy`, manual CLI/TUI checks).
- Linked issue (if any) and screenshots/gifs for UI-visible changes.
- Notes on config, keychain, or data migration impacts.

## Security & Configuration Tips
- Never commit API keys or local `.llm-meter` runtime data.
- Prefer keychain-backed credentials; use env vars only for local/CI override.
- Validate provider base URLs and organization IDs before merging config-related changes.
