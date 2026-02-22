# Architecture Overview

`llm-meter` is a Rust CLI + TUI application with local snapshot persistence.

## Main Components
- CLI entrypoint: `src/main.rs`
- TUI runtime and screens: `src/ui/run.rs`, `src/ui/app.rs`
- Service orchestration: `src/service.rs`
- Provider adapters: `src/providers/`
- Pricing resolution: `src/pricing.rs`
- Storage layer (SQLite): `src/storage.rs`
- Config + key management: `src/config.rs`

## Data Flow
1. User runs CLI command or opens TUI.
2. Config is loaded and enabled providers are resolved.
3. Service builds provider contexts (api key, settings, time window).
4. Adapters fetch usage records from provider APIs.
5. Usage rows are transformed into cost rows via pricing rules.
6. Storage replaces snapshot rows for targeted providers and window.
7. TUI aggregates and renders totals, provider breakdown, model breakdown.
8. Export command serializes cost rows as JSON/CSV.

## Provider Model
Provider integration is trait-based:
- `ProviderAdapter::fetch_usage(...)`
- `ProviderAdapter::test_connection(...)`
- `ProviderAdapter::derive_costs(...)`

Current providers:
- OpenAI (`src/providers/openai.rs`)
- Anthropic (`src/providers/anthropic.rs`)

## Persistence Model
SQLite tables:
- `usage_records`
- `cost_records`

Snapshot behavior:
- refresh deletes rows for refreshed providers in the requested window and inserts fresh rows.
- This prevents duplicate accumulation on repeated refreshes.

## Connection Testing in TUI
Provider tests run in async background tasks and return:
- optional HTTP status
- duration

Results feed:
- provider status gating for enable/disable
- per-provider rolling logs shown in Edit Provider form.
