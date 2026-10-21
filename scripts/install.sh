#!/usr/bin/env bash
#
# MultiLink installer for Linux and other Unix-like systems.
#
# One-liner:
#   curl -fsSL https://raw.githubusercontent.com/Hu-Tao128/MultiLink/release/mvp-freeze/scripts/install.sh | bash
#
# The installer prefers a prebuilt GitHub Release asset. If no asset exists
# for this platform, it can ask GitHub Actions to build one through `gh`, or
# fall back to a local source build.

set -euo pipefail

REPO="${MULTILINK_REPO:-Hu-Tao128/MultiLink}"
REF="${MULTILINK_REF:-release/mvp-freeze}"
VERSION="${MULTILINK_VERSION:-latest}"
PREFIX="${MULTILINK_PREFIX:-${HOME}/.local}"
BIN_DIR="${MULTILINK_BIN_DIR:-}"
BIN_DIR_EXPLICIT=0
if [[ -n "${BIN_DIR}" ]]; then
  BIN_DIR_EXPLICIT=1
fi

INSTALL_GUI=1
INSTALL_LSP=1
USE_GH=1
MODE="auto" # auto | release | github | source | dir
PINNED_VERSION=0
if [[ "${VERSION}" != "latest" ]]; then
  PINNED_VERSION=1
fi
LOCAL_DIST=""
TMP_DIR=""
MOUNT_POINT=""

log() { printf '[multilink] %s\n' "$*" >&2; }
warn() { printf '[multilink] warning: %s\n' "$*" >&2; }
die() {
  printf '[multilink] error: %s\n' "$*" >&2
  exit 1
}

usage() {
  cat <<'EOF'
Usage: install.sh [options]
       curl -fsSL <install-url> | bash

Options:
  --prefix DIR        Installation prefix (default: ~/.local)
  --bin-dir DIR       Directory for launchers (default: <prefix>/bin)
  --version TAG       Release tag to install (default: latest)
  --repo OWNER/REPO   GitHub repository (default: Hu-Tao128/MultiLink)
  --ref REF           Branch/tag used for source and GitHub builds
  --no-gui            Do not install the Qt GUI
  --no-lsp            Do not install the LSP server
  --no-gh            Disable the GitHub Actions fallback
  --from-github       Skip releases and build through GitHub Actions
  --from-source       Skip releases/GitHub and build from source
  --from-release      Only use a prebuilt release (never build)
  --from-dir DIR      Install already-built packages from a local directory
  -h, --help          Show this help

Environment:
  MULTILINK_REPO, MULTILINK_REF, MULTILINK_VERSION,
  MULTILINK_PREFIX, MULTILINK_BIN_DIR
  GH_TOKEN / GITHUB_TOKEN for non-interactive gh usage
EOF
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --prefix)
        [[ $# -ge 2 ]] || die "$1 requires a value"
        PREFIX="$2"
        shift
        ;;
      --prefix=*) PREFIX="${1#*=}" ;;
      --bin-dir)
        [[ $# -ge 2 ]] || die "$1 requires a value"
        BIN_DIR="$2"
        BIN_DIR_EXPLICIT=1
        shift
        ;;
      --bin-dir=*)
        BIN_DIR="${1#*=}"
        BIN_DIR_EXPLICIT=1
        ;;
      --version)
        [[ $# -ge 2 ]] || die "$1 requires a value"
        VERSION="$2"
        if [[ "${VERSION}" == "latest" ]]; then
          PINNED_VERSION=0
        else
          PINNED_VERSION=1
        fi
        shift
        ;;
      --version=*)
        VERSION="${1#*=}"
        if [[ "${VERSION}" == "latest" ]]; then
          PINNED_VERSION=0
        else
          PINNED_VERSION=1
        fi
        ;;
      --repo)
        [[ $# -ge 2 ]] || die "$1 requires a value"
        REPO="$2"
        shift
        ;;
      --repo=*) REPO="${1#*=}" ;;
      --ref)
        [[ $# -ge 2 ]] || die "$1 requires a value"
        REF="$2"
        shift
        ;;
      --ref=*) REF="${1#*=}" ;;
      --no-gui) INSTALL_GUI=0 ;;
      --no-lsp) INSTALL_LSP=0 ;;
      --no-gh) USE_GH=0 ;;
      --from-github) MODE="github" ;;
      --from-source) MODE="source" ;;
      --from-release) MODE="release" ;;
      --from-dir)
        [[ $# -ge 2 ]] || die "$1 requires a value"
        MODE="dir"
        LOCAL_DIST="$2"
        shift
        ;;
      --from-dir=*)
        MODE="dir"
        LOCAL_DIST="${1#*=}"
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

  ((INSTALL_GUI || INSTALL_LSP)) || die "Nothing to install: both --no-gui and --no-lsp were given."

  # Expand a leading ~ so paths work when the value comes from env vars.
  PREFIX="${PREFIX/#\~/${HOME}}"
  if ((BIN_DIR_EXPLICIT)); then
    BIN_DIR="${BIN_DIR/#\~/${HOME}}"
  else
    BIN_DIR="${PREFIX}/bin"
  fi
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
}

cleanup() {
  if [[ -n "${MOUNT_POINT}" ]]; then
    hdiutil detach "${MOUNT_POINT}" >/dev/null 2>&1 || true
  fi
  if [[ -n "${TMP_DIR}" ]]; then
    rm -rf "${TMP_DIR}"
  fi
}

curl_cmd() {
  curl -fsSL --proto '=https' --tlsv1.2 --retry 3 --retry-delay 2 --connect-timeout 20 "$@"
}

github_url() {
  printf 'https://github.com/%s' "${REPO}"
}

resolve_version() {
  [[ "${VERSION}" == "latest" ]] || return 0

  local api tag="" effective=""
  api="https://api.github.com/repos/${REPO}/releases/latest"

  tag="$(curl_cmd -H 'Accept: application/vnd.github+json' "${api}" 2>/dev/null |
    sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' |
    head -n 1 || true)" || true

  if [[ -z "${tag}" ]]; then
    effective="$(curl -fsSIL --proto '=https' --tlsv1.2 -o /dev/null -w '%{url_effective}' \
      "$(github_url)/releases/latest" 2>/dev/null || true)" || true
    case "${effective}" in
      */releases/tag/*) tag="${effective##*/releases/tag/}" ;;
      *) tag="" ;;
    esac
  fi

  if [[ -z "${tag}" ]]; then
    return 1
  fi

  VERSION="${tag}"
  log "Latest release: ${VERSION}"
}

asset_url() {
  printf '%s/releases/download/%s/%s' "$(github_url)" "${VERSION}" "$1"
}

asset_exists() {
  curl -fsIL --proto '=https' --tlsv1.2 --connect-timeout 15 -o /dev/null \
    "$(asset_url "$1")" >/dev/null 2>&1
}

download_asset() {
  local name="$1"
  local dest="$2"
  log "Downloading ${name}..."
  curl_cmd -o "${dest}" "$(asset_url "${name}")" ||
    die "Failed to download $(asset_url "${name}")"
  [[ -s "${dest}" ]] || die "Downloaded file is empty: ${name}"
}

hash_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  elif command -v sha256 >/dev/null 2>&1; then
    sha256 -q "$1"
  else
    return 1
  fi
}

verify_checksums() {
  local dir="$1"
  local file="${dir}/SHA256SUMS"
  [[ -f "${file}" ]] || return 0

  local hash name actual
  while read -r hash name _; do
    [[ -n "${hash}" && -n "${name}" ]] || continue
    [[ -f "${dir}/${name}" ]] || continue
    actual="$(hash_file "${dir}/${name}")" || {
      warn "No SHA-256 tool available; skipping checksum verification."
      return 0
    }
    [[ "${actual}" == "${hash}" ]] || die "Checksum mismatch for ${name}."
    log "Checksum OK: ${name}"
  done <"${file}"
}

pick_release_gui_asset() {
  local candidate
  local candidates=()

  case "${OS}" in
    linux)
      candidates=(
        "MultiLink-linux-${ARCH}.AppImage"
        "MultiLink-${ARCH}.AppImage"
        "MultiLink.AppImage"
        "MultiLink-linux-${ARCH}.tar.gz"
        "MultiLink-linux-${ARCH}.deb"
      )
      ;;
    macos)
      candidates=(
        "MultiLink-macos-${ARCH}.dmg"
        "MultiLink-macos-${ARCH}.zip"
        "MultiLink-macos-${ARCH}.tar.gz"
      )
      ;;
    *) return 1 ;;
  esac

  for candidate in "${candidates[@]}"; do
    if asset_exists "${candidate}"; then
      printf '%s\n' "${candidate}"
      return 0
    fi
  done
  return 1
}

pick_release_lsp_asset() {
  local candidate
  local candidates=(
    "MultiLink-lsp-${SLUG}.tar.gz"
    "MultiLink-lsp-${SLUG}.zip"
  )

  for candidate in "${candidates[@]}"; do
    if asset_exists "${candidate}"; then
      printf '%s\n' "${candidate}"
      return 0
    fi
  done
  return 1
}

try_release() {
  if ! resolve_version; then
    log "No GitHub release found for ${REPO}."
    return 1
  fi

  local dir="${TMP_DIR}/release"
  mkdir -p "${dir}"

  local gui_asset="" lsp_asset=""
  if ((INSTALL_GUI)); then
    gui_asset="$(pick_release_gui_asset || true)"
  fi
  if ((INSTALL_LSP)); then
    lsp_asset="$(pick_release_lsp_asset || true)"
  fi

  if [[ -z "${gui_asset}" && -z "${lsp_asset}" ]]; then
    log "Release ${VERSION} has no ${SLUG} asset."
    return 1
  fi

  if ((INSTALL_GUI)) && [[ -z "${gui_asset}" ]]; then
    log "Release ${VERSION} does not provide a GUI package for ${SLUG}."
    return 1
  fi
  if ((INSTALL_LSP)) && [[ -z "${lsp_asset}" ]]; then
    log "Release ${VERSION} does not provide an LSP package for ${SLUG}."
    return 1
  fi

  if [[ -n "${gui_asset}" ]]; then
    download_asset "${gui_asset}" "${dir}/${gui_asset}"
  fi
  if [[ -n "${lsp_asset}" ]]; then
    download_asset "${lsp_asset}" "${dir}/${lsp_asset}"
  fi
  if asset_exists "SHA256SUMS"; then
    download_asset "SHA256SUMS" "${dir}/SHA256SUMS"
  fi

  verify_checksums "${dir}"

  if [[ -n "${gui_asset}" ]]; then
    install_gui_file "${dir}/${gui_asset}"
  fi
  if [[ -n "${lsp_asset}" ]]; then
    install_lsp_file "${dir}/${lsp_asset}"
  fi
}

write_wrapper() {
  local target="$1"
  local name="${2:-multilink}"
  local prelude="${3:-}"

  mkdir -p "${BIN_DIR}"
  {
    printf '#!/usr/bin/env sh\n'
    if [[ -n "${prelude}" ]]; then
      printf '%s\n' "${prelude}"
    fi
    printf 'exec "%s" "$@"\n' "${target}"
  } >"${BIN_DIR}/${name}"
  chmod 0755 "${BIN_DIR}/${name}"
}

install_appimage() {
  local file="$1"
  local stage="${PREFIX}/opt/multilink"
  local extract_dir="${TMP_DIR}/appimage"

  chmod 0755 "${file}"
  rm -rf "${stage}" "${extract_dir}"
  mkdir -p "${stage}" "${extract_dir}"

  if (cd "${extract_dir}" && "${file}" --appimage-extract >/dev/null 2>&1) &&
    [[ -x "${extract_dir}/squashfs-root/AppRun" ]]; then
    mv "${extract_dir}/squashfs-root" "${stage}/app"
    write_wrapper "${stage}/app/AppRun" "multilink"
    log "Installed GUI: ${BIN_DIR}/multilink"
    return 0
  fi

  warn "Could not extract the AppImage. Installing it as-is (FUSE is required at runtime)."
  cp "${file}" "${stage}/multilink.AppImage"
  chmod 0755 "${stage}/multilink.AppImage"
  write_wrapper "${stage}/multilink.AppImage" "multilink" \
    'if [ ! -r /dev/fuse ]; then APPIMAGE_EXTRACT_AND_RUN=1; export APPIMAGE_EXTRACT_AND_RUN; fi'
  log "Installed GUI: ${BIN_DIR}/multilink"
}

install_gui_tar() {
  local file="$1"
  local extract_dir="${TMP_DIR}/gui"

  rm -rf "${extract_dir}"
  mkdir -p "${extract_dir}"
  tar -xzf "${file}" -C "${extract_dir}"

  if [[ "${OS}" == "macos" ]]; then
    install_macos_app "${extract_dir}"
    return 0
  fi

  local stage="${PREFIX}/opt/multilink"
  rm -rf "${stage}"
  mkdir -p "${stage}"
  cp -R "${extract_dir}"/. "${stage}"/

  local binary=""
  local candidate
  for candidate in \
    "${stage}/bin/multilink" \
    "${stage}/multilink" \
    "${stage}/usr/bin/multilink"; do
    if [[ -f "${candidate}" ]]; then
      binary="${candidate}"
      break
    fi
  done
  if [[ -z "${binary}" ]]; then
    binary="$(find "${stage}" -type f -name 'multilink' -perm -u+x 2>/dev/null | head -n 1 || true)"
  fi
  [[ -n "${binary}" ]] || die "The downloaded archive does not contain a MultiLink binary."

  chmod 0755 "${binary}"
  write_wrapper "${binary}" "multilink"
  log "Installed GUI: ${BIN_DIR}/multilink"
}

install_gui_deb() {
  local file="$1"
  local extract_dir="${TMP_DIR}/deb"
  local stage="${PREFIX}/opt/multilink"

  command -v dpkg-deb >/dev/null 2>&1 ||
    die "dpkg-deb is required to install the .deb package. Install dpkg or use the tar.gz asset."

  rm -rf "${extract_dir}" "${stage}"
  mkdir -p "${extract_dir}" "${stage}/bin"
  dpkg-deb -x "${file}" "${extract_dir}"

  if [[ -d "${extract_dir}/usr/bin" ]]; then
    cp -R "${extract_dir}/usr/bin/." "${stage}/bin/"
  fi
  [[ -f "${stage}/bin/multilink" ]] ||
    die "The .deb package does not contain usr/bin/multilink."

  write_wrapper "${stage}/bin/multilink" "multilink"
  log "Installed GUI: ${BIN_DIR}/multilink"
}

install_macos_app() {
  local root="$1"
  local app=""
  app="$(find "${root}" -maxdepth 3 -type d -name '*.app' 2>/dev/null | head -n 1 || true)"
  [[ -n "${app}" ]] || die "No .app bundle was found in the downloaded archive."

  local apps_dir="${HOME}/Applications"
  mkdir -p "${apps_dir}"
  rm -rf "${apps_dir}/MultiLink.app"
  cp -R "${app}" "${apps_dir}/MultiLink.app"

  local exe="${apps_dir}/MultiLink.app/Contents/MacOS/multilink"
  if [[ ! -x "${exe}" ]]; then
    exe="$(find "${apps_dir}/MultiLink.app/Contents/MacOS" -maxdepth 1 -type f -perm -u+x 2>/dev/null | head -n 1 || true)"
  fi
  [[ -n "${exe}" ]] || die "The MultiLink.app bundle has no executable."
  write_wrapper "${exe}" "multilink"
  log "Installed GUI: ${apps_dir}/MultiLink.app"
}

install_gui_dmg() {
  local file="$1"
  MOUNT_POINT="${TMP_DIR}/dmg"
  mkdir -p "${MOUNT_POINT}"

  command -v hdiutil >/dev/null 2>&1 || die "hdiutil is required to open the .dmg package."
  hdiutil attach -nobrowse -readonly -mountpoint "${MOUNT_POINT}" "${file}" >/dev/null ||
    die "Could not mount ${file##*/}."

  install_macos_app "${MOUNT_POINT}"
  hdiutil detach "${MOUNT_POINT}" >/dev/null 2>&1 || true
  MOUNT_POINT=""
}

install_gui_zip() {
  local file="$1"
  local extract_dir="${TMP_DIR}/gui-zip"

  command -v unzip >/dev/null 2>&1 || die "unzip is required to open the .zip package."
  [[ "${OS}" == "macos" ]] || die "The .zip GUI package is only supported on macOS."

  rm -rf "${extract_dir}"
  mkdir -p "${extract_dir}"
  unzip -q "${file}" -d "${extract_dir}"
  install_macos_app "${extract_dir}"
}

install_gui_file() {
  local file="$1"
  [[ -f "${file}" ]] || die "GUI asset not found: ${file}"

  case "${file}" in
    *.AppImage) install_appimage "${file}" ;;
    *.tar.gz) install_gui_tar "${file}" ;;
    *.deb) install_gui_deb "${file}" ;;
    *.dmg) install_gui_dmg "${file}" ;;
    *.zip) install_gui_zip "${file}" ;;
    *) die "Unsupported GUI package format: ${file##*/}" ;;
  esac
}

install_lsp_file() {
  local file="$1"
  local extract_dir="${TMP_DIR}/lsp"
  local binary=""

  [[ -f "${file}" ]] || die "LSP asset not found: ${file}"
  rm -rf "${extract_dir}"
  mkdir -p "${extract_dir}"

  case "${file}" in
    *.tar.gz) tar -xzf "${file}" -C "${extract_dir}" ;;
    *.zip)
      command -v unzip >/dev/null 2>&1 || die "unzip is required to open ${file##*/}."
      unzip -q "${file}" -d "${extract_dir}"
      ;;
    *) die "Unsupported LSP package format: ${file##*/}" ;;
  esac

  binary="$(find "${extract_dir}" -type f \( -name 'multilink-lsp' -o -name 'multilink-lsp.exe' \) 2>/dev/null |
    head -n 1 || true)"
  [[ -n "${binary}" ]] || die "The LSP package does not contain multilink-lsp."

  mkdir -p "${BIN_DIR}"
  install -m 0755 "${binary}" "${BIN_DIR}/multilink-lsp"
  log "Installed LSP: ${BIN_DIR}/multilink-lsp"
}

install_from_dist() {
  local dir="$1"
  local candidate gui_file="" lsp_file=""

  verify_checksums "${dir}"

  if ((INSTALL_GUI)); then
    case "${OS}" in
      linux)
        for candidate in .AppImage .tar.gz .deb; do
          if [[ -f "${dir}/MultiLink-${SLUG}${candidate}" ]]; then
            gui_file="${dir}/MultiLink-${SLUG}${candidate}"
            break
          fi
        done
        ;;
      macos)
        for candidate in .dmg .zip .tar.gz; do
          if [[ -f "${dir}/MultiLink-${SLUG}${candidate}" ]]; then
            gui_file="${dir}/MultiLink-${SLUG}${candidate}"
            break
          fi
        done
        ;;
    esac
    if [[ -z "${gui_file}" ]]; then
      die "No GUI package was produced for ${SLUG}."
    fi
  fi

  if ((INSTALL_LSP)); then
    if [[ -f "${dir}/MultiLink-lsp-${SLUG}.tar.gz" ]]; then
      lsp_file="${dir}/MultiLink-lsp-${SLUG}.tar.gz"
    elif [[ -f "${dir}/MultiLink-lsp-${SLUG}.zip" ]]; then
      lsp_file="${dir}/MultiLink-lsp-${SLUG}.zip"
    else
      die "No LSP package was produced for ${SLUG}."
    fi
  fi

  [[ -n "${gui_file}" || -n "${lsp_file}" ]] ||
    die "The build produced no installable package for ${SLUG}."

  if [[ -n "${gui_file}" ]]; then
    install_gui_file "${gui_file}"
  fi
  if [[ -n "${lsp_file}" ]]; then
    install_lsp_file "${lsp_file}"
  fi
}

try_github_build() {
  command -v gh >/dev/null 2>&1 || {
    warn "The 'gh' CLI is not installed; skipping the GitHub build fallback."
    return 1
  }
  gh auth status >/dev/null 2>&1 || {
    warn "The 'gh' CLI is not authenticated; skipping the GitHub build fallback."
    return 1
  }

  local helper="${TMP_DIR}/build_binaries.sh"
  local raw="https://raw.githubusercontent.com/${REPO}/${REF}/scripts/build_binaries.sh"
  local dist="${TMP_DIR}/dist"

  log "Asking GitHub Actions to build ${REPO}@${REF}..."
  curl_cmd -o "${helper}" "${raw}" || {
    warn "Could not download the build helper from ${raw}."
    return 1
  }
  chmod 0755 "${helper}"

  local args=(--via-github --repo "${REPO}" --ref "${REF}" --output "${dist}")
  if ((!INSTALL_GUI)); then
    args+=("--lsp-only")
  elif ((!INSTALL_LSP)); then
    args+=("--gui-only")
  fi

  bash "${helper}" "${args[@]}" || {
    warn "The GitHub Actions build failed."
    return 1
  }

  install_from_dist "${dist}"
}

try_source_build() {
  command -v git >/dev/null 2>&1 || {
    warn "git is required for a source build."
    return 1
  }
  command -v cargo >/dev/null 2>&1 || {
    warn "cargo (Rust) is required for a source build."
    return 1
  }
  if ((INSTALL_GUI)) && ! command -v cmake >/dev/null 2>&1; then
    warn "cmake is required for a source GUI build."
    return 1
  fi

  local src="${TMP_DIR}/src"
  local dist="${TMP_DIR}/dist"
  local repo_url

  repo_url="https://github.com/${REPO}.git"
  log "Cloning ${repo_url} (${REF})..."
  if ! git clone --depth 1 --branch "${REF}" "${repo_url}" "${src}" >/dev/null 2>&1; then
    warn "Could not clone ref '${REF}'; retrying with the repository default branch."
    rm -rf "${src}"
    git clone --depth 1 "${repo_url}" "${src}" >/dev/null 2>&1 || {
      warn "Could not clone ${repo_url}."
      return 1
    }
  fi

  local args=(--no-github --output "${dist}")
  if ((!INSTALL_GUI)); then
    args+=("--lsp-only")
  elif ((!INSTALL_LSP)); then
    args+=("--gui-only")
  fi

  log "Building from source (this can take several minutes)..."
  bash "${src}/scripts/build_binaries.sh" "${args[@]}" || {
    warn "The source build failed. Install Rust, CMake and Qt 6, or use --from-github."
    return 1
  }

  install_from_dist "${dist}"
}

install_desktop_entry() {
  [[ "${OS}" == "linux" ]] || return 0
  ((INSTALL_GUI)) || return 0

  local dir="${PREFIX}/share/applications"
  mkdir -p "${dir}"
  cat >"${dir}/multilink.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=MultiLink
Comment=Local-first multi-provider AI coding agent
Exec="${BIN_DIR}/multilink"
Terminal=false
Categories=Development;Utility;
EOF
  chmod 0644 "${dir}/multilink.desktop"

  if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "${dir}" >/dev/null 2>&1 || true
  fi
}

print_done() {
  printf '\n[multilink] Installation complete.\n' >&2
  if ((INSTALL_GUI)); then
    printf '  GUI: %s/multilink\n' "${BIN_DIR}" >&2
  fi
  if ((INSTALL_LSP)); then
    printf '  LSP: %s/multilink-lsp\n' "${BIN_DIR}" >&2
  fi
  printf '  Config (created on first run): ~/.config/multilink/multilink.toml\n' >&2

  case ":${PATH}:" in
    *":${BIN_DIR}:"*) ;;
    *)
      printf '\n[multilink] Add the launcher directory to your PATH:\n' >&2
      # shellcheck disable=SC2016 # $PATH is literal text for the user to copy
      printf '  export PATH="%s:$PATH"\n' "${BIN_DIR}" >&2
      ;;
  esac
}

main() {
  parse_args "$@"
  detect_platform
  TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/multilink-install.XXXXXX")"
  trap cleanup EXIT

  if [[ "${EUID}" -eq 0 && "${PREFIX}" == "${HOME}/.local" ]]; then
    warn "Running as root installs into ${HOME}/.local. Use --prefix /usr/local for a system-wide install."
  fi

  mkdir -p "${BIN_DIR}" 2>/dev/null ||
    die "Cannot create ${BIN_DIR}. Choose a writable prefix with --prefix or run with the appropriate permissions."
  log "Target: ${SLUG} -> ${PREFIX}"

  case "${MODE}" in
    release)
      try_release || die "No usable release asset was found for ${SLUG} (version: ${VERSION})."
      ;;
    github)
      try_github_build || die "The GitHub Actions build failed."
      ;;
    source)
      try_source_build || die "The source build failed."
      ;;
    dir)
      [[ -d "${LOCAL_DIST}" ]] || die "Local build directory not found: ${LOCAL_DIST}"
      install_from_dist "${LOCAL_DIST}"
      ;;
    auto)
      if try_release; then
        :
      elif [[ "${PINNED_VERSION}" -eq 1 ]]; then
        die "Version ${VERSION} does not provide an asset for ${SLUG}."
      elif ((USE_GH)) && try_github_build; then
        :
      elif try_source_build; then
        :
      else
        die "No prebuilt binary is available for ${SLUG}. Install Rust, CMake and Qt 6 to build from source, or install the GitHub CLI and authenticate with 'gh auth login'."
      fi
      ;;
    *)
      die "Unknown install mode: ${MODE}"
      ;;
  esac

  install_desktop_entry
  print_done
}

main "$@"
