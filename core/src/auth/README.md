# Auth (`core/src/auth/`)

OAuth and token management for remote providers.

## Files

- `oauth.rs`: authorization URL, localhost callback handling, code exchange, refresh, revoke.
- `token_store.rs`: encrypted token at-rest storage.
- `service.rs`: high-level auth workflow used by callers.
- `mod.rs`: module exports.

## Notes

- Keep credentials out of logs.
- Keep storage API stable; GUI should call service-level methods only.
- Current token store uses local encrypted files; keyring-backed storage is the next security milestone.
- OAuth login URL + callback + refresh logic exists in core and should remain Rust-owned.
