#!/usr/bin/env bash
# install.sh — thin bootstrap that downloads, verifies, and runs a devenv release.
# Source: https://github.com/nguyenvulong/devenv-linux
#
# Environment:
#   DEVENV_VERSION        Release tag to install (e.g. v1.1.0). Defaults to the latest release.
#   DEVENV_DOWNLOAD_URL   Base URL holding <arch>.tar.xz and SHA256SUMS (mirrors/testing).

set -euo pipefail

REPO="nguyenvulong/devenv-linux"
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'
DEVENV_TMP=""

die() {
  echo -e "${RED}$*${NC}" >&2
  exit 1
}

download() {
  local url="$1" dest="$2"
  if command -v curl &>/dev/null; then
    curl -fsSL --retry 3 "$url" -o "$dest"
  else
    wget -q --tries=3 -O "$dest" "$url"
  fi
}

sha256_check() {
  if command -v sha256sum &>/dev/null; then
    sha256sum -c --status -
  elif command -v shasum &>/dev/null; then
    shasum -a 256 -c --status -
  else
    die "Neither sha256sum nor shasum found; cannot verify the download."
  fi
}

# Everything runs inside main so a truncated `curl | bash` download never
# executes a partial script.
main() {
  # ── Detect architecture ─────────────────────────────────────────────────────
  local arch
  case "$(uname -m)" in
    x86_64) arch="x86_64" ;;
    aarch64 | arm64) arch="aarch64" ;;
    *)
      die "Unsupported architecture: $(uname -m)
Pre-built binaries are available for x86_64 and aarch64 only."
      ;;
  esac

  # ── Check required tools ────────────────────────────────────────────────────
  command -v curl &>/dev/null || command -v wget &>/dev/null \
    || die "Neither curl nor wget found. Install one and try again."
  command -v tar &>/dev/null || die "tar is required. Install it and try again."
  command -v xz &>/dev/null \
    || die "xz is required to extract the release archive.
Install it first: apt install xz-utils | dnf install xz | pacman -S xz"

  # ── Resolve release URL ─────────────────────────────────────────────────────
  local base
  if [ -n "${DEVENV_DOWNLOAD_URL:-}" ]; then
    base="${DEVENV_DOWNLOAD_URL%/}"
    echo -e "${BLUE}Installing devenv from ${base}...${NC}"
  elif [ -n "${DEVENV_VERSION:-}" ]; then
    base="https://github.com/${REPO}/releases/download/v${DEVENV_VERSION#v}"
    echo -e "${BLUE}Installing devenv v${DEVENV_VERSION#v}...${NC}"
  else
    base="https://github.com/${REPO}/releases/latest/download"
    echo -e "${BLUE}Installing the latest devenv release...${NC}"
  fi

  # ── Download, verify, and extract ───────────────────────────────────────────
  local archive="${arch}.tar.xz" tmp status
  DEVENV_TMP=$(mktemp -d)
  trap 'rm -rf "$DEVENV_TMP"' EXIT
  tmp="$DEVENV_TMP"

  echo -e "${BLUE}Downloading ${archive}...${NC}"
  download "${base}/${archive}" "${tmp}/${archive}" \
    || die "Failed to download ${base}/${archive}"
  download "${base}/SHA256SUMS" "${tmp}/SHA256SUMS" \
    || die "Failed to download ${base}/SHA256SUMS; refusing to run an unverified binary."

  (cd "$tmp" && grep " ${archive}\$" SHA256SUMS | sha256_check) \
    || die "Checksum verification failed for ${archive}."
  echo -e "${GREEN}Checksum verified.${NC}"

  tar -xJf "${tmp}/${archive}" -C "$tmp"
  chmod +x "${tmp}/devenv"

  # ── Run the installer ───────────────────────────────────────────────────────
  # Not exec'd, so the EXIT trap removes the temporary directory afterwards.
  # Under `curl | bash`, stdin is the script pipe; hand the installer the
  # terminal instead when one is available.
  echo -e "${GREEN}Launching $("${tmp}/devenv" --version)...${NC}"
  status=0
  if [ ! -t 0 ] && (: </dev/tty) 2>/dev/null; then
    "${tmp}/devenv" "$@" </dev/tty || status=$?
  else
    "${tmp}/devenv" "$@" || status=$?
  fi
  return "$status"
}

main "$@"
