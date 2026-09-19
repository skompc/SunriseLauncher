#!/usr/bin/env bash
set -euo pipefail

if [[ -x /opt/homebrew/bin/brew ]]; then
  eval "$(/opt/homebrew/bin/brew shellenv)"
elif [[ -x /usr/local/bin/brew ]]; then
  eval "$(/usr/local/bin/brew shellenv)"
fi

if ! command -v brew >/dev/null 2>&1; then
  echo "Installing Homebrew..."
  /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
  if [[ -x /opt/homebrew/bin/brew ]]; then
    eval "$(/opt/homebrew/bin/brew shellenv)"
  elif [[ -x /usr/local/bin/brew ]]; then
    eval "$(/usr/local/bin/brew shellenv)"
  fi
fi

if ! command -v brew >/dev/null 2>&1; then
  echo "Homebrew installation did not complete. Install it from https://brew.sh and run this script again."
  exit 1
fi

brew update
brew install node llvm zig cmake pkg-config

if ! command -v rustup >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
fi

if [[ -f "${HOME}/.cargo/env" ]]; then
  source "${HOME}/.cargo/env"
else
  export PATH="${HOME}/.cargo/bin:${PATH}"
fi

if ! command -v rustup >/dev/null 2>&1; then
  echo "Rustup installation did not complete. Open a new terminal and run this script again."
  exit 1
fi

rustup default stable
rustup target add x86_64-apple-darwin aarch64-apple-darwin x86_64-pc-windows-msvc aarch64-pc-windows-msvc x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu
cargo install cargo-xwin --locked
cargo install cargo-zigbuild --locked
npm install
if [[ "${SUNRISE_SKIP_BUILD:-0}" != "1" ]]; then
  npm run package:mac
fi