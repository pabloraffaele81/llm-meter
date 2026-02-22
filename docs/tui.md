# TUI Guide

The TUI is implemented in `src/ui/run.rs` and centered around dashboard monitoring plus provider management.

## Main Screens
- Dashboard
- Provider Manager
- Provider Form (Add/Edit)
- Confirm Dialog
- Error Dialog
- Info Dialog

## Dashboard Keys
- `a`: focus action panel
- `r`: refresh now
- `1`: 1-day window
- `7`: 7-day window
- `3`: 30-day window
- `z`: toggle compact mode
- `q` or `Ctrl+C`: open quit confirmation
- `Esc`: unfocus action panel

Action panel keys (when focused):
- `Up` / `Down`: select action
- `Enter`: execute selected action

## Provider Manager Keys
- `n`: add provider
- `Enter`: edit selected provider
- `t`: test selected provider connection
- `e`: enable/disable selected provider
- `k`: delete stored provider key
- `d`: remove provider config and key
- `Esc`: return to dashboard

Enable rule:
- Provider must pass connection test before being enabled.

## Provider Form Keys
- `Tab` / `Shift+Tab`: move field focus
- `t`: run connection test
- `x`: clear test logs for current provider
- `v`: show/hide advanced fields (`base_url`, `organization_id`)
- `e`: toggle Enabled (only when Enabled field is focused)
- `i`: open full test error details (when failed)
- `Enter`: save
- `Esc`: cancel

## Test Logs in Edit Provider
The lower panel in provider form shows rolling test logs:
- event name
- detail message
- HTTP status (if available)
- duration in ms (if available)

Log behavior:
- stored per provider
- capped (oldest entries are trimmed)
- clearable with `x`

## Confirm and Info Dialogs
- `Enter` confirms primary action or closes dialog
- `Esc` cancels/closes
