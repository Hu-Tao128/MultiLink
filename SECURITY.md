# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

## Reporting a Vulnerability

If you discover a security vulnerability, please open an issue. We appreciate responsible disclosure and will respond promptly.

## Security Model

### What We Protect

- **OAuth Tokens**: Stored encrypted at rest using AES-256-GCM
- **Configuration Files**: Restricted file permissions (0600 on Unix)
- **Local Data**: Sessions and conversation history

### What We Don't Protect (User Responsibility)

- Local Ollama server security (network exposure, authentication)
- System-level keyring integration (planned for future)

## Encryption Details

- **Algorithm**: AES-256-GCM
- **Key Derivation**: Currently app-local, planned migration to OS keyring
- **Storage Location**:
  - Linux: `~/.config/multilink/`
  - macOS: `~/Library/Application Support/multilink/`
  - Windows: `%APPDATA%\multilink\`

## Best Practices

1. Don't expose local Ollama to untrusted networks
2. Use firewall rules to restrict Ollama access
3. For production, consider authentication proxies
4. Keep MultiLink updated for security fixes
