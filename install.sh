#!/usr/bin/env bash

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}Bootstrapping devenv-linux interactive installer...${NC}"

# 1. Install minimal dependencies to build rust
if [ -f /etc/os-release ]; then
  . /etc/os-release
  if [[ "$ID" == "debian" || "$ID" == "ubuntu" || "$ID_LIKE" == *"debian"* ]]; then
    echo -e "${BLUE}Debian/Ubuntu detected, updating and checking base packages...${NC}"
    if ! dpkg -s build-essential curl git unzip tar >/dev/null 2>&1; then
      sudo apt-get update
      sudo apt-get install -y build-essential curl git unzip tar
    fi
  elif [[ "$ID" == "arch" || "$ID_LIKE" == *"arch"* ]]; then
    echo -e "${BLUE}Arch Linux detected, checking base packages...${NC}"
    if ! pacman -Qi base-devel curl git unzip tar >/dev/null 2>&1; then
      sudo pacman -Sy --noconfirm base-devel curl git unzip tar
    fi
  elif [[ "$ID" == "fedora" || "$ID" == "centos" || "$ID_LIKE" == *"fedora"* || "$ID_LIKE" == *"centos"* ]]; then
    echo -e "${BLUE}RedHat-family Linux detected, checking base packages...${NC}"
    if ! rpm -q gcc gcc-c++ make curl git unzip tar >/dev/null 2>&1; then
      sudo dnf install -y gcc gcc-c++ make curl git unzip tar
    fi
  fi
fi

# 2. Check for rust
if ! command -v cargo &> /dev/null; then
  echo -e "${BLUE}Cargo not found. Installing Rust via rustup...${NC}"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  source "$HOME/.cargo/env"
fi

# 3. Build the installer
echo -e "${BLUE}Building the interactive installer...${NC}"
DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
cd "$DIR/installer"
cargo build --release

# 4. Run the installer
echo -e "${GREEN}Starting the installer...${NC}"
./target/release/installer
