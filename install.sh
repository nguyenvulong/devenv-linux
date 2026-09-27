#!/usr/bin/env bash
# install.sh -- bootstrap for devenv-linux.
#
# Downloads the devenv release binary for this machine's architecture,
# verifies it against the release's SHA256SUMS, and runs it. Any arguments
# are passed to devenv (e.g. `bash -s -- --all`).
#
# Requirements (checked up front, with a distro-specific install hint):
#   - curl or wget (and CA certificates) to download the release
#   - tar and gzip to unpack it, sha256sum (or shasum) to verify it
# All but curl/wget ship with every supported base image. Releases up to
# v1.1.1 only published .tar.xz; for those, xz is also needed.
#
# Environment:
#   DEVENV_VERSION        Release tag to install (e.g. v1.2.0). Defaults to the latest release.
#   DEVENV_DOWNLOAD_URL   Base URL holding <arch>.tar.gz and SHA256SUMS (mirrors/testing).
#
# Source: https://github.com/nguyenvulong/devenv-linux

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

# Print the command that installs the given packages on this distro.
install_hint() {
  local id="" like=""
  if [ -r /etc/os-release ]; then
    # shellcheck source=/dev/null
    id=$(. /etc/os-release && echo "${ID:-}")
    # shellcheck source=/dev/null
    like=$(. /etc/os-release && echo "${ID_LIKE:-}")
  fi
  case " $id $like " in
    *" debian "* | *" ubuntu "*) echo "apt-get update && apt-get install -y $*" ;;
    *" fedora "* | *" rhel "* | *" centos "*) echo "dnf install -y ${*//xz-utils/xz}" ;;
    *" arch "*) echo "pacman -S --needed ${*//xz-utils/xz}" ;;
    *) echo "install: $*" ;;
  esac
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
    || die "Neither curl nor wget found. Install one first (as root):
  $(install_hint curl ca-certificates)"
  local tool
  for tool in tar gzip; do
    command -v "$tool" &>/dev/null \
      || die "$tool is required to unpack the release. Install it first (as root):
  $(install_hint "$tool")"
  done

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
  local archive tmp status
  DEVENV_TMP=$(mktemp -d)
  trap 'rm -rf "$DEVENV_TMP"' EXIT
  tmp="$DEVENV_TMP"

  # Prefer .tar.gz (no xz needed); releases up to v1.1.1 only have .tar.xz.
  archive="${arch}.tar.gz"
  echo -e "${BLUE}Downloading ${archive}...${NC}"
  if ! download "${base}/${archive}" "${tmp}/${archive}" 2>/dev/null; then
    archive="${arch}.tar.xz"
    command -v xz &>/dev/null \
      || die "This release only provides ${archive}, which needs xz. Install it first (as root):
  $(install_hint xz-utils)"
    echo -e "${BLUE}Falling back to ${archive}...${NC}"
    download "${base}/${archive}" "${tmp}/${archive}" \
      || die "Failed to download ${base}/${archive}"
  fi
  download "${base}/SHA256SUMS" "${tmp}/SHA256SUMS" \
    || die "Failed to download ${base}/SHA256SUMS; refusing to run an unverified binary."

  (cd "$tmp" && grep " ${archive}\$" SHA256SUMS | sha256_check) \
    || die "Checksum verification failed for ${archive}."
  echo -e "${GREEN}Checksum verified.${NC}"

  tar -xf "${tmp}/${archive}" -C "$tmp"
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
