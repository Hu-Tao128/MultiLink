# `scripts/` - Build and Installation Tooling

This directory contains reproducible helper scripts for packaging and installing
MultiLink. Scripts are support tooling, not runtime agent tools.

## `build_binaries.sh`

Builds the Qt/QML GUI and the LSP server and writes version-less release-style
assets to `dist/`:

```text
dist/
├── MultiLink-<os>-<arch>.tar.gz       # GUI (bin/multilink, bin/qml)
├── MultiLink-lsp-<os>-<arch>.tar.gz   # LSP (bin/multilink-lsp)
└── SHA256SUMS
```

On macOS the GUI archive contains `MultiLink.app`. On Linux the GUI binary
requires the Qt 6 runtime; CI builds also publish an AppImage and a `.deb`.

Examples:

```bash
# Build everything locally
./scripts/build_binaries.sh

# Build only the LSP server
./scripts/build_binaries.sh --lsp-only

# Force a GitHub Actions build (requires `gh auth login`)
./scripts/build_binaries.sh --via-github

# Build on GitHub and publish a release for an existing tag
./scripts/build_binaries.sh --via-github --ref v1.1.0 --release-tag v1.1.0

# Write artifacts somewhere else
./scripts/build_binaries.sh --output /tmp/multilink-dist
```

Requirements for a local build:

- Rust stable (`cargo`)
- CMake 3.21+
- Qt 6.5+ with `Core`, `Quick`, `QuickControls2`, `Qml` and `Network`

If any prerequisite is missing, or the local build fails, the script falls back
to `.github/workflows/release.yml` through the GitHub CLI. The fallback needs
`gh` installed and authenticated, and triggers the build on GitHub Actions. Use
`--no-github` to disable it.

The release workflow also supports `workflow_dispatch`; the fallback downloads
its artifacts and normalizes them into `dist/`.

## `install.sh`

End-user installer for Linux, macOS and other Unix-like systems:

```bash
curl -fsSL https://raw.githubusercontent.com/Hu-Tao128/MultiLink/release/mvp-freeze/scripts/install.sh | bash
```

Installation strategy, in order:

1. Download a prebuilt asset from the latest GitHub Release.
2. If no asset exists for the platform, trigger the GitHub Actions build with
   `gh` (when installed and authenticated) and download the artifacts.
3. Otherwise, clone the repository and build locally (requires Rust, CMake and
   Qt 6 for the GUI).
4. Exit with actionable instructions if none of the above is possible.

Common options:

```bash
# Install into /usr/local (usually needs sudo for writing)
curl -fsSL <url> | bash -s -- --prefix /usr/local

# Only the LSP server
curl -fsSL <url> | bash -s -- --no-gui

# Pin a release and skip all build fallbacks
curl -fsSL <url> | bash -s -- --version v1.1.0 --from-release

# Build from source explicitly
./scripts/install.sh --from-source

# Build through GitHub Actions explicitly
./scripts/install.sh --from-github

# Install packages already produced by build_binaries.sh
./scripts/install.sh --from-dir ./dist
```

Default locations:

| Item | Path |
|------|------|
| Launchers | `~/.local/bin/multilink`, `~/.local/bin/multilink-lsp` |
| GUI payload | `~/.local/opt/multilink/` |
| macOS app | `~/Applications/MultiLink.app` |
| Desktop entry | `~/.local/share/applications/multilink.desktop` |
| Config | `~/.config/multilink/multilink.toml` (created on first run) |

Override with `--prefix`, `--bin-dir`, or the `MULTILINK_PREFIX`,
`MULTILINK_BIN_DIR`, `MULTILINK_VERSION`, `MULTILINK_REPO` and
`MULTILINK_REF` environment variables. The installer never uses `sudo`; it
writes only under the chosen prefix (and `~/Applications` on macOS).

To uninstall, remove the launchers, `~/.local/opt/multilink`, the desktop entry
and (optionally) `~/.config/multilink`.

## `install_multilink.sh`

Legacy developer installer that builds the GUI in `build/gui`, writes a default
config and registers a desktop entry. It is kept for existing workflows; new
installations should use `install.sh`.

## Agent Guidance

- Prefer Rust tools in `core/src/tools/` for runtime behavior.
- Keep scripts reproducible and documented with command-line examples.
- Do not rely on scripts for hidden side effects during chat/runtime execution.
