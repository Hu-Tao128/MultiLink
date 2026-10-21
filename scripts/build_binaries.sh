#!/usr/bin/env bash
#
# Build and package MultiLink binaries for Linux/Unix.
#
# The script tries a local build first. If the required toolchain (Rust, CMake,
# Qt 6) is unavailable, or a local build fails, it can fall back to GitHub
# Actions through the `gh` CLI and download the generated artifacts.
#
# Usage: scripts/build_binaries.sh [options]
# Run with --help for the full option list.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [[ -f "${SCRIPT_DIR}/../core/Cargo.toml" ]]; then
  REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
elif [[ -f "${PWD}/core/Cargo.toml" ]]; then
  REPO_ROOT="${PWD}"
else
  REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
fi

DEFAULT_REF="release/mvp-freeze"
DEFAULT_REPO="Hu-Tao128/MultiLink"

OUTPUT_DIR="${REPO_ROOT}/dist"
BUILD_DIR="${REPO_ROOT}/build"
WANT_GUI=1
WANT_LSP=1
GH_FALLBACK="auto" # auto | never | force
REPO_SLUG=""
REF=""
RELEASE_TAG=""

log() { printf '[build] %s\n' "$*" >&2; }
warn() { printf '[build] warning: %s\n' "$*" >&2; }
die() {
  printf '[build] error: %s\n' "$*" >&2
  exit 1
}

usage() {
  cat <<'EOF'
Usage: scripts/build_binaries.sh [options]

Build MultiLink binaries and package them into dist/.

Targets (default: both):
  --gui-only             Build only the Qt/QML GUI
  --lsp-only             Build only the LSP server

Options:
  -o, --output DIR       Output directory (default: <repo>/dist)
      --build-dir DIR    CMake build directory (default: <repo>/build)
      --via-github       Build on GitHub Actions with gh (skip the local build)
      --no-github        Never fall back to GitHub Actions
      --repo OWNER/REPO  GitHub repository (default: origin remote)
      --ref REF          Git branch/tag to build on GitHub (default: current)
      --release-tag TAG  With --via-github, publish the artifacts as a release
  -h, --help             Show this help

Environment:
  MULTILINK_BUILD_JOBS   Parallel jobs for CMake (passed to --parallel)
  GH_TOKEN / GITHUB_TOKEN Token used by gh if you are not logged in

Examples:
  ./scripts/build_binaries.sh
  ./scripts/build_binaries.sh --lsp-only
  ./scripts/build_binaries.sh --via-github --ref main
  ./scripts/build_binaries.sh --via-github --ref v1.1.0 --release-tag v1.1.0
EOF
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --gui-only)
        WANT_GUI=1
        WANT_LSP=0
        ;;
      --lsp-only)
        WANT_GUI=0
        WANT_LSP=1
        ;;
      -o | --output)
        [[ $# -ge 2 ]] || die "$1 requires a value"
        OUTPUT_DIR="$2"
        shift
        ;;
      --output=*)
        OUTPUT_DIR="${1#*=}"
        ;;
      --build-dir)
        [[ $# -ge 2 ]] || die "$1 requires a value"
        BUILD_DIR="$2"
        shift
        ;;
      --build-dir=*)
        BUILD_DIR="${1#*=}"
        ;;
      --via-github)
        GH_FALLBACK="force"
        ;;
      --no-github | --local-only)
        GH_FALLBACK="never"
        ;;
      --repo)
        [[ $# -ge 2 ]] || die "$1 requires a value"
        REPO_SLUG="$2"
        shift
        ;;
      --repo=*)
        REPO_SLUG="${1#*=}"
        ;;
      --ref)
        [[ $# -ge 2 ]] || die "$1 requires a value"
        REF="$2"
        shift
        ;;
      --ref=*)
        REF="${1#*=}"
        ;;
      --release-tag)
        [[ $# -ge 2 ]] || die "$1 requires a value"
        RELEASE_TAG="$2"
        shift
        ;;
      --release-tag=*)
        RELEASE_TAG="${1#*=}"
        ;;
      -h | --help)
        usage
        exit 0
        ;;
      *)
        die "Unknown option: $1 (try --help)"
        ;;
    esac
    shift
  done

  OUTPUT_DIR="$(mkdir -p "${OUTPUT_DIR}" && cd "${OUTPUT_DIR}" && pwd)"
  BUILD_DIR="$(mkdir -p "${BUILD_DIR}" && cd "${BUILD_DIR}" && pwd)"
}

detect_platform() {
  case "$(uname -s)" in
    Linux*) OS="linux" ;;
    Darwin*) OS="macos" ;;
    FreeBSD*) OS="freebsd" ;;
    OpenBSD*) OS="openbsd" ;;
    NetBSD*) OS="netbsd" ;;
    DragonFly*) OS="dragonfly" ;;
    *) OS="$(uname -s | tr '[:upper:]' '[:lower:]')" ;;
  esac

  case "$(uname -m)" in
    x86_64 | amd64) ARCH="x86_64" ;;
    aarch64 | arm64)
      if [[ "${OS}" == "macos" ]]; then
        ARCH="arm64"
      else
        ARCH="aarch64"
      fi
      ;;
    armv7l | armv7) ARCH="armv7" ;;
    i386 | i686) ARCH="i686" ;;
    *) ARCH="$(uname -m)" ;;
  esac

  SLUG="${OS}-${ARCH}"
  log "Platform: ${SLUG}"
}

detect_repo_slug() {
  if [[ -n "${REPO_SLUG}" ]]; then
    return 0
  fi

  local url=""
  if git -C "${REPO_ROOT}" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    url="$(git -C "${REPO_ROOT}" remote get-url origin 2>/dev/null || true)"
  fi

  if [[ -n "${url}" ]]; then
    url="${url%.git}"
    case "${url}" in
      git@*:*) REPO_SLUG="${url#*:}" ;;
      ssh://git@*/*) REPO_SLUG="${url#ssh://git@*/}" ;;
      https://*/*) REPO_SLUG="${url#https://*/}" ;;
      http://*/*) REPO_SLUG="${url#http://*/}" ;;
      *) REPO_SLUG="${url}" ;;
    esac
  fi

  [[ -n "${REPO_SLUG}" ]] || REPO_SLUG="${DEFAULT_REPO}"
}

detect_ref() {
  if [[ -n "${REF}" ]]; then
    return 0
  fi

  local branch=""
  if git -C "${REPO_ROOT}" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    branch="$(git -C "${REPO_ROOT}" symbolic-ref --quiet --short HEAD 2>/dev/null || true)"
    if [[ -z "${branch}" ]]; then
      branch="$(git -C "${REPO_ROOT}" describe --tags --exact-match 2>/dev/null || true)"
    fi
  fi

  if [[ -n "${branch}" ]]; then
    REF="${branch}"
  else
    REF="${DEFAULT_REF}"
  fi
}

project_version() {
  if git -C "${REPO_ROOT}" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    git -C "${REPO_ROOT}" describe --tags --always --dirty 2>/dev/null || echo "unknown"
  else
    echo "unknown"
  fi
}

require_local_repo() {
  [[ -f "${REPO_ROOT}/core/Cargo.toml" ]] ||
    die "Local build requested but core/Cargo.toml was not found in ${REPO_ROOT}. Use --via-github to build remotely."
  if ((WANT_LSP)); then
    [[ -f "${REPO_ROOT}/lsp-server/Cargo.toml" ]] ||
      die "Local LSP build requested but lsp-server/Cargo.toml was not found in ${REPO_ROOT}."
  fi
  if ((WANT_GUI)); then
    [[ -f "${REPO_ROOT}/gui/CMakeLists.txt" ]] ||
      die "Local GUI build requested but gui/CMakeLists.txt was not found in ${REPO_ROOT}. Use --via-github to build remotely."
  fi
}

have_tools() {
  local tool
  for tool in "$@"; do
    if ! command -v "${tool}" >/dev/null 2>&1; then
      warn "Missing command: ${tool}"
      return 1
    fi
  done
  return 0
}

find_gui_binary() {
  local candidate
  for candidate in \
    "${BUILD_DIR}/gui/multilink" \
    "${BUILD_DIR}/gui/Release/multilink" \
    "${BUILD_DIR}/gui/Release/multilink.exe"; do
    if [[ -f "${candidate}" ]]; then
      printf '%s\n' "${candidate}"
      return 0
    fi
  done

  candidate="$(find "${BUILD_DIR}/gui" -maxdepth 2 -type f \
    \( -name 'multilink' -o -name 'multilink.exe' \) 2>/dev/null | head -n 1 || true)"
  [[ -n "${candidate}" ]] || return 1
  printf '%s\n' "${candidate}"
}

find_gui_bundle() {
  local candidate
  candidate="$(find "${BUILD_DIR}/gui" -maxdepth 1 -type d -name '*.app' 2>/dev/null | head -n 1 || true)"
  [[ -n "${candidate}" ]] || return 1
  printf '%s\n' "${candidate}"
}

build_gui_local() {
  log "Building Rust chat backend..."
  cargo build --release --manifest-path "${REPO_ROOT}/gui/rust/chat_controller/Cargo.toml"

  local cmake_args=(-S "${REPO_ROOT}/gui" -B "${BUILD_DIR}/gui" -DCMAKE_BUILD_TYPE=Release)
  if [[ -n "${MULTILINK_BUILD_JOBS:-}" ]]; then
    cmake_args+=("-DCMAKE_BUILD_PARALLEL_LEVEL=${MULTILINK_BUILD_JOBS}")
  fi

  log "Configuring CMake..."
  cmake "${cmake_args[@]}"

  log "Compiling the GUI..."
  if [[ -n "${MULTILINK_BUILD_JOBS:-}" ]]; then
    cmake --build "${BUILD_DIR}/gui" --config Release --parallel "${MULTILINK_BUILD_JOBS}"
  else
    cmake --build "${BUILD_DIR}/gui" --config Release --parallel
  fi
}

package_gui() {
  local stage="${OUTPUT_DIR}/.stage/gui-${SLUG}"
  local archive="${OUTPUT_DIR}/MultiLink-${SLUG}.tar.gz"

  rm -rf "${stage}"
  mkdir -p "${stage}"
  rm -f "${archive}"

  log "Staging GUI files..."
  if ! cmake --install "${BUILD_DIR}/gui" --config Release --prefix "${stage}" >/dev/null 2>&1; then
    warn "cmake --install failed; staging the binary manually."
  fi

  if [[ "${OS}" == "macos" ]]; then
    local bundle=""
    if [[ -d "${stage}/multilink.app" ]]; then
      bundle="${stage}/multilink.app"
    elif bundle="$(find_gui_bundle)"; then
      :
    else
      die "The macOS application bundle was not found after the build."
    fi
    rm -rf "${stage}/MultiLink.app"
    mv "${bundle}" "${stage}/MultiLink.app"
    tar -czf "${archive}" -C "${stage}" MultiLink.app
  else
    if [[ ! -f "${stage}/bin/multilink" ]]; then
      local binary=""
      binary="$(find_gui_binary)" || die "The GUI binary was not found after the build."
      mkdir -p "${stage}/bin"
      install -m 0755 "${binary}" "${stage}/bin/multilink"
      if [[ -d "${REPO_ROOT}/gui/qml" && ! -d "${stage}/bin/qml" ]]; then
        cp -R "${REPO_ROOT}/gui/qml" "${stage}/bin/qml"
      fi
    fi
    tar -czf "${archive}" -C "${stage}" bin
  fi

  log "Packaged: ${archive}"
}

build_lsp_local() {
  log "Building the LSP server..."
  cargo build --release --manifest-path "${REPO_ROOT}/lsp-server/Cargo.toml"
}

package_lsp() {
  local stage="${OUTPUT_DIR}/.stage/lsp-${SLUG}"
  local archive="${OUTPUT_DIR}/MultiLink-lsp-${SLUG}.tar.gz"
  local binary="${REPO_ROOT}/lsp-server/target/release/multilink-lsp"

  [[ -f "${binary}" ]] || die "The LSP binary was not found at ${binary}."
  rm -rf "${stage}"
  mkdir -p "${stage}/bin"
  install -m 0755 "${binary}" "${stage}/bin/multilink-lsp"
  rm -f "${archive}"
  tar -czf "${archive}" -C "${stage}" bin
  log "Packaged: ${archive}"
}

github_build() {
  command -v gh >/dev/null 2>&1 ||
    die "GitHub fallback requested but the 'gh' CLI is not installed. Install it from https://cli.github.com/ or build locally."
  gh auth status >/dev/null 2>&1 ||
    die "The 'gh' CLI is not authenticated. Run 'gh auth login' (or set GH_TOKEN) and retry."

  detect_repo_slug
  detect_ref
  [[ "${REF}" != "HEAD" ]] ||
    die "Detached HEAD detected. Pass --ref <branch-or-tag> so GitHub Actions can check out the code."

  log "Dispatching GitHub Actions 'release.yml' on ${REPO_SLUG}@${REF}..."
  log "The workflow must exist on that remote ref (push your changes first if needed)."

  local since run_id="" url=""
  since="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

  local args=(workflow run release.yml --repo "${REPO_SLUG}" --ref "${REF}")
  if [[ -n "${RELEASE_TAG}" ]]; then
    args+=(-f "release_tag=${RELEASE_TAG}")
  fi
  gh "${args[@]}"

  log "Waiting for the workflow run to appear..."
  local attempt
  for ((attempt = 0; attempt < 30; attempt++)); do
    run_id="$(gh run list --repo "${REPO_SLUG}" --workflow release.yml --limit 30 \
      --json databaseId,createdAt \
      --jq "[.[] | select(.createdAt >= \"${since}\")] | sort_by(.createdAt) | reverse | .[0].databaseId // empty" \
      2>/dev/null || true)"
    [[ -n "${run_id}" ]] && break
    sleep 4
  done

  [[ -n "${run_id}" ]] ||
    die "Could not locate the dispatched workflow run. Open https://github.com/${REPO_SLUG}/actions and check manually."

  url="https://github.com/${REPO_SLUG}/actions/runs/${run_id}"
  log "Watching run ${run_id}: ${url}"

  if ! gh run watch "${run_id}" --repo "${REPO_SLUG}" --interval 10 --exit-status; then
    die "GitHub Actions build failed: ${url}"
  fi

  local download_dir="${OUTPUT_DIR}/.gh-download"
  rm -rf "${download_dir}"
  mkdir -p "${download_dir}"

  log "Downloading build artifacts..."
  gh run download "${run_id}" --repo "${REPO_SLUG}" --pattern 'MultiLink-*' --dir "${download_dir}"

  local copied=0
  local file base
  while IFS= read -r -d '' file; do
    base="${file##*/}"
    case "${base}" in
      MultiLink-* | SHA256SUMS*)
        cp -f "${file}" "${OUTPUT_DIR}/"
        copied=$((copied + 1))
        ;;
    esac
  done < <(find "${download_dir}" -type f -print0)

  rm -rf "${download_dir}"
  ((copied > 0)) || die "The workflow finished but no MultiLink artifacts were downloaded."

  log "Downloaded ${copied} artifact(s) into ${OUTPUT_DIR}."
}

generate_checksums() {
  local checksum_file="${OUTPUT_DIR}/SHA256SUMS"
  local file hash
  local hasher=""

  if command -v sha256sum >/dev/null 2>&1; then
    hasher="sha256sum"
  elif command -v shasum >/dev/null 2>&1; then
    hasher="shasum -a 256"
  elif command -v sha256 >/dev/null 2>&1; then
    hasher="sha256 -q"
  else
    warn "No SHA-256 tool found; skipping SHA256SUMS."
    return 0
  fi

  rm -f "${checksum_file}"
  for file in "${OUTPUT_DIR}"/MultiLink-*; do
    [[ -f "${file}" ]] || continue
    hash="$(${hasher} "${file}" | awk '{print $1}')"
    printf '%s  %s\n' "${hash}" "${file##*/}" >>"${checksum_file}"
  done

  [[ -s "${checksum_file}" ]] || rm -f "${checksum_file}"
}

print_summary() {
  local file
  printf '\n[build] Local version: %s\n' "$(project_version)" >&2
  printf '[build] Artifacts in %s:\n' "${OUTPUT_DIR}" >&2
  for file in "${OUTPUT_DIR}"/MultiLink-* "${OUTPUT_DIR}/SHA256SUMS"; do
    [[ -f "${file}" ]] || continue
    printf '  - %s (%s)\n' "${file##*/}" "$(du -h "${file}" | awk '{print $1}')" >&2
  done
}

main() {
  parse_args "$@"
  detect_platform

  if [[ "${GH_FALLBACK}" == "force" ]]; then
    github_build
    generate_checksums
    print_summary
    return 0
  fi

  require_local_repo

  local failures=0
  local rc=0

  if ((WANT_GUI)); then
    if have_tools cargo cmake; then
      set +e
      build_gui_local
      rc=$?
      set -e
      if ((rc == 0)); then
        set +e
        package_gui
        rc=$?
        set -e
      fi
      if ((rc != 0)); then
        warn "The local GUI build failed (exit ${rc})."
        failures=1
      fi
    else
      warn "The GUI build requires Rust and CMake."
      failures=1
    fi
  fi

  if ((WANT_LSP)); then
    if have_tools cargo; then
      set +e
      build_lsp_local
      rc=$?
      set -e
      if ((rc == 0)); then
        set +e
        package_lsp
        rc=$?
        set -e
      fi
      if ((rc != 0)); then
        warn "The local LSP build failed (exit ${rc})."
        failures=1
      fi
    else
      warn "The LSP build requires Rust."
      failures=1
    fi
  fi

  if ((failures)); then
    if [[ "${GH_FALLBACK}" == "never" ]]; then
      die "Build failed and the GitHub fallback is disabled (--no-github)."
    fi

    warn "Falling back to GitHub Actions to build the binaries..."
    if github_build; then
      generate_checksums
      print_summary
      return 0
    fi
    die "The GitHub Actions fallback also failed."
  fi

  generate_checksums
  print_summary
}

main "$@"
