#!/usr/bin/env bash
# Verify a completed `devenv --all` run: tools, configurations, and smoke tests.
# Runs as whichever user performed the installation.
set -euo pipefail

export PATH="$HOME/.local/bin:$HOME/.local/share/mise/shims:$HOME/.cargo/bin:$PATH"

echo "=== Checking installed tools ==="
mise --version
fzf --version
rg --version
fd --version
bat --version
eza --version
glow --version
jaq --version
zellij --version
rustc --version
cargo --version
node --version
go version
uv --version
nvim --version
fish --version

echo "=== Checking configurations ==="
[ -d ~/.config/nvim ] || { echo "Missing nvim config"; exit 1; }
[ -f ~/.config/fish/config.fish ] || { echo "Missing fish config"; exit 1; }
grep -q "mise activate bash" ~/.bashrc || { echo "Missing bash activation"; exit 1; }
fish --no-execute ~/.config/fish/config.fish
fish -c 'source ~/.config/fish/config.fish'

echo "=== Smoke tests ==="
echo "test" | fzf -f "test"
tmp=$(mktemp)
echo "test content" > "$tmp"
rg "test" "$tmp"
rm -f "$tmp"
node -e 'console.log("Hello from Node.js")'

echo "=== All checks passed ==="
