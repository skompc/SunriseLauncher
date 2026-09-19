$ErrorActionPreference = "Stop"

function Refresh-Path {
  $env:Path = [Environment]::GetEnvironmentVariable("Path", "Machine") + ";" + [Environment]::GetEnvironmentVariable("Path", "User")
}

if (Get-Command winget -ErrorAction SilentlyContinue) {
  winget install --id OpenJS.NodeJS.LTS --exact --accept-source-agreements --accept-package-agreements
  winget install --id Rustlang.Rustup --exact --accept-source-agreements --accept-package-agreements
  winget install --id LLVM.LLVM --exact --accept-source-agreements --accept-package-agreements
  winget install --id Kitware.CMake --exact --accept-source-agreements --accept-package-agreements
  winget install --id zig.zig --exact --accept-source-agreements --accept-package-agreements
} elseif (Get-Command choco -ErrorAction SilentlyContinue) {
  choco install nodejs-lts rustup.install llvm cmake zig -y
} else {
  throw "Install winget (recommended) or Chocolatey, then run this script again."
}

Refresh-Path
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:ProgramFiles\LLVM\bin;$env:Path"

if (-not (Get-Command rustup -ErrorAction SilentlyContinue)) {
  throw "Rustup was not found after installation. Open a new PowerShell window and run this script again."
}

rustup default stable
rustup target add x86_64-pc-windows-msvc aarch64-pc-windows-msvc x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu
cargo install cargo-xwin --locked
cargo install cargo-zigbuild --locked
npm install
if ($env:SUNRISE_SKIP_BUILD -ne "1") {
  npm run build:renderer
  node scripts/build-backend.cjs --platform=win
  npx electron-builder --publish never --win --x64 --arm64
}