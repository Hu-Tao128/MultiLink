#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
BUILD_DIR="${REPO_DIR}/build/gui"
APP_BIN="${BUILD_DIR}/multilink"

if [[ ! -x "${APP_BIN}" ]]; then
  echo "[install] GUI binary not found, building release target..."
  cmake -S "${REPO_DIR}/gui" -B "${BUILD_DIR}" -DCMAKE_BUILD_TYPE=Release
  cmake --build "${BUILD_DIR}" --config Release
fi

CONFIG_DIR="${HOME}/.config/multilink"
CONFIG_FILE="${CONFIG_DIR}/multilink.toml"
mkdir -p "${CONFIG_DIR}"

if [[ ! -f "${CONFIG_FILE}" ]]; then
  cat > "${CONFIG_FILE}" <<'EOF'
version = 2

[[servers]]
name = "Local Ollama"
provider = "ollama"
# Local:  http://127.0.0.1:11434
# Remote: http://192.168.1.50:11434
base_url = "http://127.0.0.1:11434"
default_model = "qwen2.5-coder:3b"
priority = 1
enabled = true

[storage]
models_dir = "~/.local/share/multilink/models"

[context]
embeddings_enabled = true
embed_model = "embeddinggemma"
project_top_k = 8
max_project_tokens = 2000
debug = false
engine = "v1"

[performance]
profile = "auto"
max_parallel_streams = 4

[routing]
remote_threshold = "heavy"

[network]
allow_remote = false
shared_secret = ""
allowed_ips = []

[ui]
streaming = true
json_logs = false

[runtime]
max_context_tokens = 7000
summary_trigger_tokens = 6000
keep_last_messages = 6
max_summary_tokens = 1200
max_project_files = 30
max_project_bytes = 204800
max_project_file_bytes = 65536
max_project_context_tokens = 3500
max_parallel_streams = 4
context_embeddings_enabled = true
context_debug = false
context_embed_model = "embeddinggemma"
context_project_top_k = 8
context_ollama_base_url = "http://127.0.0.1:11434"
observability_json_logs = false

[runtime.profiles.small]
max_project_context_tokens = 800
max_project_files = 6

[runtime.profiles.medium]
max_project_context_tokens = 2000
max_project_files = 15

[runtime.profiles.large]
max_project_context_tokens = 3500
max_project_files = 30
EOF
  chmod 600 "${CONFIG_FILE}"
  echo "[install] Created default config at ${CONFIG_FILE}"
fi

LOCAL_BIN_DIR="${HOME}/.local/bin"
WRAPPER="${LOCAL_BIN_DIR}/multilink-gui"
mkdir -p "${LOCAL_BIN_DIR}"

cat > "${WRAPPER}" <<EOF
#!/usr/bin/env bash
exec "${APP_BIN}" "\$@"
EOF
chmod +x "${WRAPPER}"

APP_DIR="${HOME}/.local/share/applications"
DESKTOP_FILE="${APP_DIR}/multilink.desktop"
mkdir -p "${APP_DIR}"

cat > "${DESKTOP_FILE}" <<EOF
[Desktop Entry]
Type=Application
Name=MultiLink
Comment=Multi-provider local chat assistant
Exec=${WRAPPER}
Path=${REPO_DIR}
Terminal=false
Categories=Development;Utility;
EOF

if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "${APP_DIR}" >/dev/null 2>&1 || true
fi

echo "[install] Done"
echo "- Launcher: ${DESKTOP_FILE}"
echo "- Command:  ${WRAPPER}"
echo "- Config:   ${CONFIG_FILE}"
