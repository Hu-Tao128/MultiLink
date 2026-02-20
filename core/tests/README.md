# Core Tests (`core/tests/`)

Integration tests for core behavior.

## Files

- `config_tests.rs`: default config creation and parsing behavior.
- `router_tests.rs`: provider availability and fallback routing.
- `token_store_tests.rs`: encrypted token storage roundtrip.
- `chat_runtime_tests.rs`: stream event flow and persistence behavior.

Run with:

```bash
cd core
cargo test
```
