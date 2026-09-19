#!/usr/bin/env bash
set -euo pipefail

if ! command -v apt-get >/dev/null 2>&1; then
  echo "This Linux bootstrap supports Debian/Ubuntu via apt-get. Install Node.js, Rust, LLVM, MinGW, and aarch64-linux-gnu-gcc with your distribution package manager."
  exit 1
fi

sudo apt-get update
sudo apt-get install -y curl build-essential pkg-config clang lld llvm cmake gcc-aarch64-linux-gnu g++-aarch64-linux-gnu mingw-w64

if ! command -v zig >/dev/null 2>&1; then
  zig_version="0.14.1"
  case "$(uname -m)" in
    x86_64) zig_arch="x86_64" ;;
    aarch64) zig_arch="aarch64" ;;
    *)
      echo "Unsupported Linux architecture for the Zig fallback: $(uname -m). Install Zig manually and rerun this script."
      exit 1
      ;;
  esac

  zig_archive="zig-${zig_arch}-linux-${zig_version}.tar.xz"
  zig_url="https://ziglang.org/download/${zig_version}/${zig_archive}"
  tmp_dir="$(mktemp -d)"
  trap 'rm -rf "$tmp_dir"' EXIT
  curl -fsSL "$zig_url" -o "${tmp_dir}/${zig_archive}"
  sudo mkdir -p /opt/zig
  sudo tar -xJf "${tmp_dir}/${zig_archive}" -C /opt/zig
  sudo ln -sfn "/opt/zig/zig-${zig_arch}-linux-${zig_version}" /opt/zig/current
  export PATH="/opt/zig/current:${PATH}"
fi

if ! command -v zig >/dev/null 2>&1; then
  echo "Zig installation did not complete. Install Zig and run this script again."
  exit 1
fi

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
rustup target add x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu x86_64-pc-windows-gnu aarch64-pc-windows-gnu
cargo install cargo-xwin --locked
cargo install cargo-zigbuild --locked
if ! command -v node >/dev/null 2>&1; then
  curl -fsSL https://deb.nodesource.com/setup_22.x | sudo -E bash -
  sudo apt-get install -y nodejs
fi
npm install
if [[ "${SUNRISE_SKIP_BUILD:-0}" != "1" ]]; then
  npm run build:renderer
  node scripts/build-backend.cjs --platform=linux
  npx electron-builder --publish never --linux --x64 --arm64
fi