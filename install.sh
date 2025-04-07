#!/usr/bin/env bash

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Directories
CONFIG_DIR="$HOME/.config"
LOCAL_BIN_DIR="$HOME/.local/bin"
LOCAL_SHARE_DIR="$HOME/.local/share"

# Create necessary directories
mkdir -p "$LOCAL_BIN_DIR"
mkdir -p "$CONFIG_DIR"
mkdir -p "$LOCAL_SHARE_DIR"

# Add ~/.local/bin to PATH if not already there
if [[ ":$PATH:" != *":$HOME/.local/bin:"* ]]; then
  echo 'export PATH="$HOME/.local/bin:$PATH"' >>"$HOME/.bashrc"
  export PATH="$HOME/.local/bin:$PATH"
  echo -e "${GREEN}Added ~/.local/bin to PATH${NC}"
fi

# Detect Linux distribution
detect_distro() {
  if [ -f /etc/os-release ]; then
    . /etc/os-release
    DISTRO_FAMILY=""

    if [[ "$ID" == "debian" || "$ID" == "ubuntu" || "$ID_LIKE" == *"debian"* ]]; then
      DISTRO_FAMILY="debian"
      PACKAGE_MANAGER="apt-get"
      INSTALL_CMD="$PACKAGE_MANAGER install -y"
      UPDATE_CMD="$PACKAGE_MANAGER update"
    elif [[ "$ID" == "arch" || "$ID_LIKE" == *"arch"* ]]; then
      DISTRO_FAMILY="arch"
      PACKAGE_MANAGER="pacman"
      INSTALL_CMD="$PACKAGE_MANAGER -S --noconfirm"
      UPDATE_CMD="$PACKAGE_MANAGER -Sy"
    elif [[ "$ID" == "fedora" || "$ID" == "centos" || "$ID_LIKE" == *"fedora"* || "$ID_LIKE" == *"centos"* ]]; then
      # Not compatible with almalinux, rocky, redhat for now
      DISTRO_FAMILY="redhat"
      PACKAGE_MANAGER="dnf"
      INSTALL_CMD="$PACKAGE_MANAGER install -y"
      UPDATE_CMD="$PACKAGE_MANAGER check-update || true"
    else
      echo -e "${RED}Unsupported distribution: $ID${NC}"
      exit 1
    fi

    echo -e "${GREEN}Detected distribution family: $DISTRO_FAMILY${NC}"
  else
    echo -e "${RED}Cannot detect Linux distribution${NC}"
    exit 1
  fi
}

# Install base dependencies
install_base_dependencies() {
  echo -e "${BLUE}Installing base dependencies...${NC}"

  sudo $UPDATE_CMD

  case $DISTRO_FAMILY in
  debian)
    sudo $INSTALL_CMD build-essential curl wget git unzip tar
    ;;
  arch)
    sudo $INSTALL_CMD base-devel curl wget git unzip tar
    ;;
  redhat)
    sudo $INSTALL_CMD gcc gcc-c++ make curl wget git unzip tar
    ;;
  esac

  echo -e "${GREEN}Base dependencies installed successfully${NC}"
}

install_utilities() {
  echo -e "${BLUE}Installing utilities...${NC}"

  # Install fzf
  echo -e "${YELLOW}Installing fzf...${NC}"
  if ! command -v fzf &>/dev/null; then
    # Always install from GitHub to ensure it goes to LOCAL_BIN_DIR
    install_fzf_from_github
  else
    # Check if fzf is in LOCAL_BIN_DIR
    if [[ "$(which fzf)" != "$LOCAL_BIN_DIR/fzf" ]]; then
      install_fzf_from_github
    else
      echo -e "${GREEN}fzf is already installed in $LOCAL_BIN_DIR${NC}"
    fi
  fi

  # Install ripgrep
  echo -e "${YELLOW}Installing ripgrep...${NC}"
  if ! command -v rg &>/dev/null; then
    # Always install from GitHub to ensure it goes to LOCAL_BIN_DIR
    install_ripgrep_from_github
  else
    # Check if rg is in LOCAL_BIN_DIR
    if [[ "$(which rg)" != "$LOCAL_BIN_DIR/rg" ]]; then
      install_ripgrep_from_github
    else
      echo -e "${GREEN}ripgrep is already installed in $LOCAL_BIN_DIR${NC}"
    fi
  fi

  # Install fd-find
  echo -e "${YELLOW}Installing fd-find...${NC}"
  if ! command -v fd &>/dev/null; then
    # Always install from GitHub to ensure it goes to LOCAL_BIN_DIR
    install_fd_from_github
  else
    # Check if fd is in LOCAL_BIN_DIR
    if [[ "$(which fd)" != "$LOCAL_BIN_DIR/fd" ]]; then
      install_fd_from_github
    else
      echo -e "${GREEN}fd is already installed in $LOCAL_BIN_DIR${NC}"
    fi
  fi

  # Install tmux
  echo -e "${YELLOW}Installing tmux...${NC}"
  case $DISTRO_FAMILY in
  debian)
    sudo apt install -y tmux
    ;;
  arch)
    sudo pacman -S --noconfirm tmux
    ;;
  redhat)
    if command -v dnf >/dev/null; then
      sudo dnf install -y tmux
    else
      sudo yum install -y tmux
    fi
    ;;
  esac

  # Install bat
  echo -e "${YELLOW}Installing bat...${NC}"
  if ! command -v bat &>/dev/null; then
    # Always install from GitHub to ensure it goes to LOCAL_BIN_DIR
    install_bat_from_github
  else
    # Check if bat is in LOCAL_BIN_DIR
    if [[ "$(which bat)" != "$LOCAL_BIN_DIR/bat" ]]; then
      install_bat_from_github
    else
      echo -e "${GREEN}bat is already installed in $LOCAL_BIN_DIR${NC}"
    fi
  fi

  # Install eza (newer replacement for exa)
  echo -e "${YELLOW}Installing eza...${NC}"
  if ! command -v eza &>/dev/null; then
    # Always install from GitHub to ensure it goes to LOCAL_BIN_DIR
    install_eza_from_github
  else
    # Check if eza is in LOCAL_BIN_DIR
    if [[ "$(which eza)" != "$LOCAL_BIN_DIR/eza" ]]; then
      install_eza_from_github
    else
      echo -e "${GREEN}eza is already installed in $LOCAL_BIN_DIR${NC}"
    fi
  fi

  # Configure tmux
  ## 1. Clone oh-my-tmux into XDG-compliant location
  git clone --single-branch https://github.com/gpakosz/.tmux.git ~/.config/tmux

  ## 2. Symlink ~/.tmux.conf to the real file in ~/.config/tmux
  ln -sf ~/.config/tmux/.tmux.conf ~/.tmux.conf

  ## 3. Copy the user-editable local config without leading dot (more readable)
  cp ~/.config/tmux/.tmux.conf.local ~/.config/tmux/tmux.conf.local

  echo -e "${GREEN}Utilities installed successfully${NC}"
}

# Install fzf from GitHub
install_fzf_from_github() {
  echo -e "${YELLOW}Installing fzf from GitHub...${NC}"
  FZF_VERSION=$(curl -s https://api.github.com/repos/junegunn/fzf/releases/latest | grep -Po '"tag_name": "\K[^"]*')

  # Create temporary directory
  TMP_DIR=$(mktemp -d)
  cd "$TMP_DIR"

  # Download and extract
  curl -sL "https://github.com/junegunn/fzf/releases/download/${FZF_VERSION}/fzf-${FZF_VERSION#v}-linux_amd64.tar.gz" -o fzf.tar.gz
  tar -xzf fzf.tar.gz

  # Install
  chmod +x fzf
  mv fzf "$LOCAL_BIN_DIR/"

  # Clean up
  cd "$HOME"
  rm -rf "$TMP_DIR"

  echo -e "${GREEN}fzf installed successfully${NC}"
}

# Install ripgrep from GitHub
install_ripgrep_from_github() {
  echo -e "${YELLOW}Installing ripgrep from GitHub...${NC}"
  RG_VERSION=$(curl -s https://api.github.com/repos/BurntSushi/ripgrep/releases/latest | grep -Po '"tag_name": "\K[^"]*')

  # Create temporary directory
  TMP_DIR=$(mktemp -d)
  cd "$TMP_DIR"

  # Download and extract
  curl -sL "https://github.com/BurntSushi/ripgrep/releases/download/${RG_VERSION}/ripgrep-${RG_VERSION}-x86_64-unknown-linux-musl.tar.gz" -o rg.tar.gz
  tar -xzf rg.tar.gz

  # Install
  cd ripgrep-*-x86_64-*/
  chmod +x rg
  mv rg "$LOCAL_BIN_DIR/"

  # Clean up
  cd "$HOME"
  rm -rf "$TMP_DIR"

  echo -e "${GREEN}ripgrep installed successfully${NC}"
}

# Install fd from GitHub
install_fd_from_github() {
  echo -e "${YELLOW}Installing fd from GitHub...${NC}"
  FD_VERSION=$(curl -s https://api.github.com/repos/sharkdp/fd/releases/latest | grep -Po '"tag_name": "\K[^"]*')

  # Create temporary directory
  TMP_DIR=$(mktemp -d)
  cd "$TMP_DIR"

  # Download and extract
  curl -sL "https://github.com/sharkdp/fd/releases/download/${FD_VERSION}/fd-${FD_VERSION}-x86_64-unknown-linux-musl.tar.gz" -o fd.tar.gz
  tar -xzf fd.tar.gz

  # Install
  cd fd-*-x86_64-*/
  chmod +x fd
  mv fd "$LOCAL_BIN_DIR/"

  # Clean up
  cd "$HOME"
  rm -rf "$TMP_DIR"

  echo -e "${GREEN}fd installed successfully${NC}"
}

# Install Rust
install_rust() {
  echo -e "${BLUE}Installing Rust...${NC}"
  if ! command -v rustc &>/dev/null; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
    echo -e "${GREEN}Rust installed successfully${NC}"
  else
    echo -e "${GREEN}Rust is already installed${NC}"
  fi
}

# Install Neovim
install_neovim() {
  echo -e "${BLUE}Installing Neovim...${NC}"
  if ! command -v nvim &>/dev/null; then
    NVIM_VERSION=$(curl -s https://api.github.com/repos/neovim/neovim/releases/latest | grep -Po '"tag_name": "\K[^"]*')

    # Create temporary directory
    TMP_DIR=$(mktemp -d)
    cd "$TMP_DIR"

    # Download and extract
    curl -sL "https://github.com/neovim/neovim/releases/download/${NVIM_VERSION}/nvim-linux-x86_64.tar.gz" -o nvim.tar.gz
    tar -xzf nvim.tar.gz

    # Install
    mkdir -p "$LOCAL_SHARE_DIR/nvim"
    cp -r nvim-linux-x86_64/* "$LOCAL_SHARE_DIR/nvim/"
    ln -sf "$LOCAL_SHARE_DIR/nvim/bin/nvim" "$LOCAL_BIN_DIR/nvim"

    # Create config directory
    mkdir -p "$CONFIG_DIR/nvim"

    # Clean up
    cd "$HOME"
    rm -rf "$TMP_DIR"

    echo -e "${GREEN}Neovim installed successfully${NC}"
  else
    echo -e "${GREEN}Neovim is already installed${NC}"
  fi
}

# Install Node.js
install_nodejs() {
  echo -e "${BLUE}Installing Node.js...${NC}"
  if ! command -v node &>/dev/null; then
    # Install nvm
    curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/master/install.sh | bash

    # Source nvm
    export NVM_DIR="$HOME/.nvm"
    [ -s "$NVM_DIR/nvm.sh" ] && \. "$NVM_DIR/nvm.sh"

    # Install latest LTS version
    nvm install --lts

    echo -e "${GREEN}Node.js installed successfully${NC}"
  else
    echo -e "${GREEN}Node.js is already installed${NC}"
  fi
}

# Install Go
install_go() {
  echo -e "${BLUE}Installing Go...${NC}"
  if ! command -v go &>/dev/null; then
    GO_VERSION=$(curl -s https://go.dev/VERSION?m=text | head -n 1)

    # Create temporary directory
    TMP_DIR=$(mktemp -d)
    cd "$TMP_DIR"

    # Download and extract
    curl -sL "https://go.dev/dl/${GO_VERSION}.linux-amd64.tar.gz" -o go.tar.gz

    # Install
    tar -C "$LOCAL_SHARE_DIR" -xzf go.tar.gz

    # Add to PATH if not already there
    if [[ ":$PATH:" != *":$LOCAL_SHARE_DIR/go/bin:"* ]]; then
      echo 'export PATH="$HOME/.local/share/go/bin:$PATH"' >>"$HOME/.bashrc"
      export PATH="$LOCAL_SHARE_DIR/go/bin:$PATH"
    fi

    # Clean up
    cd "$HOME"
    rm -rf "$TMP_DIR"

    echo -e "${GREEN}Go installed successfully${NC}"
  else
    echo -e "${GREEN}Go is already installed${NC}"
  fi
}

install_uv() {
  echo -e "${BLUE}Installing uv...${NC}"
  if ! command -v uv &>/dev/null; then
    if command -v curl &>/dev/null; then
      curl -LsSf https://astral.sh/uv/install.sh | sh
    elif command -v wget &>/dev/null; then
      wget -qO- https://astral.sh/uv/install.sh | sh
    else
      echo -e "${RED}Neither curl nor wget is available to download uv${NC}"
      exit 1
    fi
    echo -e "${GREEN}uv installed successfully${NC}"
  else
    echo -e "${GREEN}uv is already installed${NC}"
  fi
}

install_lazyvim() {
  echo -e "${BLUE}Installing LazyVim and dependencies...${NC}"
  if [ -d ~/.config/nvim ]; then
    echo -e "${YELLOW}Backing up existing Neovim configuration...${NC}"
    mv ~/.config/nvim ~/.config/nvim.bak
  fi

  # Clone LazyVim starter
  git clone https://github.com/LazyVim/starter ~/.config/nvim
  rm -rf ~/.config/nvim/.git

  echo -e "${GREEN}LazyVim has been installed. Start Neovim to complete the setup.${NC}"
}

install_nushell() {
  echo -e "${BLUE}Installing Nushell...${NC}"

  # Fetch the latest Nushell version
  NU_VERSION=$(curl -s https://api.github.com/repos/nushell/nushell/releases/latest | grep -Po '"tag_name": "\K[^"]*')

  # Create temporary directory
  TMP_DIR=$(mktemp -d)
  cd "$TMP_DIR"

  # Download and extract
  curl -sL "https://github.com/nushell/nushell/releases/download/${NU_VERSION}/nu-${NU_VERSION}-x86_64-unknown-linux-musl.tar.gz" -o nu.tar.gz
  tar -xzf nu.tar.gz -C $LOCAL_SHARE_DIR/

  # Install
  mv $LOCAL_SHARE_DIR/nu-${NU_VERSION}-x86_64-unknown-linux-musl $LOCAL_SHARE_DIR/nu
  ln -sf $LOCAL_SHARE_DIR/nu/nu $LOCAL_BIN_DIR/

  # Cleanup
  rm -rf "$TMP_DIR"
  cd "$HOME"

  # Configure Nushell
  mkdir -p ~/.config/nushell

  # Add Nushell to available shells if not already present
  if ! grep -q "$LOCAL_BIN_DIR/nu" /etc/shells; then
    echo "$LOCAL_BIN_DIR/nu" | sudo tee -a /etc/shells
  fi

  echo -e "${GREEN}Nushell installed successfully${NC}"
}

# Install bat from GitHub
install_bat_from_github() {
  echo -e "${YELLOW}Installing bat from GitHub...${NC}"
  BAT_VERSION=$(curl -s https://api.github.com/repos/sharkdp/bat/releases/latest | grep -Po '"tag_name": "\K[^"]*')

  # Create temporary directory
  TMP_DIR=$(mktemp -d)
  cd "$TMP_DIR"

  # Download and extract
  curl -sL "https://github.com/sharkdp/bat/releases/download/${BAT_VERSION}/bat-${BAT_VERSION}-x86_64-unknown-linux-musl.tar.gz" -o bat.tar.gz
  tar -xzf bat.tar.gz

  # Install
  mkdir -p "$LOCAL_SHARE_DIR/bat"
  cp -r bat-*-x86_64-*/* "$LOCAL_SHARE_DIR/bat/"
  ln -sf "$LOCAL_SHARE_DIR/bat/bat" "$LOCAL_BIN_DIR/bat"

  # Clean up
  cd "$HOME"
  rm -rf "$TMP_DIR"

  echo -e "${GREEN}bat installed successfully${NC}"
}

# Install eza from GitHub
install_eza_from_github() {
  echo -e "${YELLOW}Installing eza from GitHub...${NC}"
  EZA_VERSION=$(curl -s https://api.github.com/repos/eza-community/eza/releases/latest | grep -Po '"tag_name": "\K[^"]*')

  # Create temporary directory
  TMP_DIR=$(mktemp -d)
  cd "$TMP_DIR"

  # Download and extract
  curl -sL "https://github.com/eza-community/eza/releases/download/${EZA_VERSION}/eza_x86_64-unknown-linux-musl.tar.gz" -o eza.tar.gz
  tar -xzf eza.tar.gz

  # Install
  chmod +x eza
  mv eza "$LOCAL_BIN_DIR/"

  # Clean up
  cd "$HOME"
  rm -rf "$TMP_DIR"

  echo -e "${GREEN}eza installed successfully${NC}"
}

# Report installation results and versions
report() {
  echo -e "${BLUE}Generating installation report...${NC}"

  # Function to check command existence and version
  check_version() {
    local cmd=$1
    local name=$2
    local version
    if command -v "$cmd" &>/dev/null; then
      version=$("$cmd" --version 2>&1 | head -n 1)
      echo -e "${GREEN}$name is installed: $version${NC}"
    else
      echo -e "${RED}$name is not installed${NC}"
    fi
  }

  # Check versions of installed tools
  check_version fzf "fzf"
  check_version rg "ripgrep"
  check_version fd "fd"
  check_version rustc "Rust"
  check_version nvim "Neovim"
  check_version node "Node.js"
  check_version nu "Nushell"
  echo -e "${GREEN}bat is installed: $(bat --version)${NC}"
  echo -e "${GREEN}eza is installed: $(eza --version | tail -n2 | head -n1)${NC}"
  echo -e "${GREEN}Go is installed: $(go version)${NC}"

  echo -e "${BLUE}Installation report generated successfully${NC}"
}

# Main function
main() {
  echo -e "${BLUE}Setting up Linux development environment...${NC}"
  echo -e "${BLUE}*******************************************${NC}"

  # Detect distribution
  detect_distro

  # Install base dependencies
  install_base_dependencies

  # Install programming languages
  install_rust
  install_uv
  install_nodejs
  install_go

  # Install utilities
  install_utilities

  # Install development tools
  install_neovim
  install_lazyvim
  install_nushell

  # Generate installation report
  report

  echo -e "${BLUE}*******************************************${NC}"
  echo -e "${BLUE}Linux development environment setup completed!${NC}"
  echo -e "${BLUE}Run ${GREEN}source $HOME/.bashrc${BLUE} to reload the new configuration.${NC}"
}

# Run the main function
main
