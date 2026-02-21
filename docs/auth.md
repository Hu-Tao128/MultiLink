# OAuth Design

## Goals

- Desktop-friendly login flow.
- Local callback handling without embedded webview.
- Encrypted token storage with refresh support.

## Flow

1. Core creates provider authorization URL with state.
2. GUI opens default browser.
3. Provider redirects to `http://127.0.0.1:<port>/callback`.
4. Core listener captures code/state.
5. Core exchanges code for access + refresh tokens.
6. TokenStore saves encrypted payload on disk.
7. AuthService refreshes token when near expiry.

## Storage

- Linux: `~/.config/multilink/`
- Windows: `%APPDATA%\multilink\`
- macOS: `~/Library/Application Support/multilink/`
- Tokens are encrypted with AES-256-GCM and stored per provider.
- Current implementation keeps an app-local encryption key (`master.key`) with restrictive permissions.
- Planned hardening: move key material to OS keyring/credential store.

## Security controls

- AES-256-GCM encryption at rest.
- Restrictive file permissions on Unix.
- Logout clears local tokens.
- Revocation endpoint call is attempted when provider supports it.

## Current integration status

- Core module (`core/src/auth/`) implements OAuth URL generation, localhost callback capture, code exchange, refresh, and revoke.
- GUI flow wiring (login/logout buttons and account state surfacing) is still pending.
