# Configuration

Configuration logic lives in `src/config.rs`.

## Home Directory Resolution
App home is resolved in this order:
1. `LLM_METER_HOME` environment variable (if set)
2. OS project directory (`com/neubell/llm-meter`)
3. Fallback: local `.llm-meter/` directory

Derived paths:
- config dir: `<home>/config`
- data dir: `<home>/data`
- config file: `<home>/config/config.toml`
- database: `<home>/data/snapshots.sqlite`

## `config.toml` Shape

```toml
refresh_seconds = 60
enabled_providers = ["openai"]

[provider_settings.openai]
base_url = "https://api.openai.com"
organization_id = "org_123"

[[pricing_overrides]]
provider = "openai"
model_pattern = "gpt-4o"
input_per_1m = 2.5
output_per_1m = 10.0
```

Notes:
- Provider names are normalized to lowercase.
- Duplicate enabled providers are deduplicated.

## API Key Resolution
When a provider key is needed, resolution order is:
1. OS keychain entry under service `llm-meter` and account `provider:<name>`
2. Environment variable `<PROVIDER>_API_KEY` (uppercased, `-` converted to `_`)

Examples:
- `OPENAI_API_KEY`
- `ANTHROPIC_API_KEY`

## Provider Settings
- `base_url` (optional): custom API base URL
- `organization_id` (optional): provider org context (used by providers that support it)

You can leave advanced fields empty and rely on default provider endpoints.
