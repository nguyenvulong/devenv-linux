#!/usr/bin/env bash
# install.sh — thin bootstrap that downloads and runs the latest devenv release.
# Source: https://github.com/nguyenvulong/devenv-linux

set -euo pipefail

REPO="nguyenvulong/devenv-linux"
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

# ── Detect architecture ───────────────────────────────────────────────────────
ARCH=$(uname -m)
case "$ARCH" in
  x86_64)  ARCH_LABEL="x86_64" ;;
  aarch64) ARCH_LABEL="aarch64" ;;
  arm64)   ARCH_LABEL="aarch64" ;;  # macOS arm64 alias
  *)
    echo -e "${RED}Unsupported architecture: $ARCH${NC}"
    echo "Pre-built binaries are available for x86_64 and aarch64 only."
    exit 1
    ;;
esac

# ── Fetch latest release version ─────────────────────────────────────────────
echo -e "${BLUE}Fetching latest devenv release...${NC}"
if command -v curl &>/dev/null; then
  VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
    | grep '"tag_name"' | sed 's/.*"v\([^"]*\)".*/\1/')
elif command -v wget &>/dev/null; then
  VERSION=$(wget -qO- "https://api.github.com/repos/${REPO}/releases/latest" \
    | grep '"tag_name"' | sed 's/.*"v\([^"]*\)".*/\1/')
else
  echo -e "${RED}Neither curl nor wget found. Install one and try again.${NC}"
  exit 1
fi

if [ -z "$VERSION" ]; then
  echo -e "${RED}Could not determine latest release version. Check your internet connection.${NC}"
  exit 1
fi

echo -e "${BLUE}Latest version: v${VERSION}${NC}"

# ── Download and extract ──────────────────────────────────────────────────────
ARCHIVE="devenv-${VERSION}-${ARCH_LABEL}.tar.xz"
URL="https://github.com/${REPO}/releases/download/v${VERSION}/${ARCHIVE}"
TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

echo -e "${BLUE}Downloading ${ARCHIVE}...${NC}"
if command -v curl &>/dev/null; then
  curl -fsSL "$URL" -o "${TMPDIR}/${ARCHIVE}"
else
  wget -qO "${TMPDIR}/${ARCHIVE}" "$URL"
fi

tar -xJf "${TMPDIR}/${ARCHIVE}" -C "$TMPDIR"
chmod +x "${TMPDIR}/devenv"

# ── Run the installer ─────────────────────────────────────────────────────────
echo -e "${GREEN}Launching devenv v${VERSION}...${NC}"
exec "${TMPDIR}/devenv" "$@"
