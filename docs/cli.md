# CLI Reference

Binary name: `llm-meter`

Run via Cargo from repo root:

```bash
cargo run -- <command> [args]
```

## `init`
Creates config/data directories and initial config file.

```bash
cargo run -- init
```

Expected output:
- `Initialized llm-meter config and data directories.`

## `add-provider`
Configures provider settings and stores API key in OS keychain.

```bash
cargo run -- add-provider openai --api-key "$OPENAI_API_KEY"
cargo run -- add-provider anthropic --api-key "$ANTHROPIC_API_KEY"
```

Optional fields:

```bash
cargo run -- add-provider openai \
  --api-key "$OPENAI_API_KEY" \
  --base-url "https://api.openai.com" \
  --organization-id "org_123"
```

Notes:
- Provider names are normalized to lowercase.
- `add-provider` adds provider to `enabled_providers`.

## `refresh`
Polls enabled providers and writes a fresh snapshot window.

```bash
cargo run -- refresh --window 1d
cargo run -- refresh --window 7d
cargo run -- refresh --window 30d
```

Invalid example:

```bash
cargo run -- refresh --window 2d
```

Expected error:
- `Unsupported window. Use 1d, 7d, or 30d.`

## `export`
Exports stored `cost_records`.

```bash
cargo run -- export --format json
cargo run -- export --format csv
```

Supported formats:
- `json`
- `csv`

## `tui`
Launches interactive terminal UI.

```bash
cargo run -- tui
```

## Script Equivalents
- Run app: `./scripts/run-app.sh`
- Full local checks: `./scripts/test-local.sh`
