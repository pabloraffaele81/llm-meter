# Troubleshooting

## `No API key found for provider ...`
Cause:
- No key in keychain and no provider env var.

Fix:
```bash
cargo run -- add-provider openai --api-key "$OPENAI_API_KEY"
```
Or export env var:
```bash
export OPENAI_API_KEY="..."
```

## `Unsupported window. Use 1d, 7d, or 30d.`
Cause:
- Invalid `refresh --window` value.

Fix:
```bash
cargo run -- refresh --window 7d
```

## Provider test fails in TUI
Possible causes:
- invalid key
- blocked network
- wrong custom `base_url`
- provider permission/org mismatch

Fix steps:
1. Open provider form, run `t` test again.
2. Press `i` for detailed error.
3. Check test log panel in Edit Provider.
4. Remove/adjust advanced fields with `v` if not needed.

## Cannot enable provider in TUI
Cause:
- enable is gated by successful connection test.

Fix:
1. Run `t` until status is success.
2. Toggle Enabled field with `e`.
3. Save with `Enter`.

## Keyring errors
Cause:
- OS credential service unavailable or denied.

Fix:
- retry command
- ensure OS keychain/credential manager is unlocked
- use env vars as fallback for runtime access

## Empty exports
Cause:
- no refresh data collected yet
- all providers disabled

Fix:
```bash
cargo run -- refresh --window 7d
cargo run -- export --format json
```

## Local environment isolation
Use isolated home for debugging:

```bash
LLM_METER_HOME="$(mktemp -d)" cargo run -- init
LLM_METER_HOME="$LLM_METER_HOME" cargo run -- tui
```
