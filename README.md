# Project Sunrise Launcher

Standalone Electron launcher for Project Sunrise. The application combines a Vite-rendered interface, a secure Electron shell, and a Rust worker for streamed installer and launcher operations.

## Requirements

For normal development, install:

- Node.js 22 or newer with npm
- Rust stable through `rustup`
- Native C/C++ linker and platform build tools

For a fresh machine, use the platform bootstrap script instead of installing Node manually. The scripts install system dependencies, Rust toolchains, cross-build tools, npm packages, and then build the application.

## Fresh machine setup

Run the script for the operating system from the project root.

### macOS

```bash
bash scripts/setup-build-env-macos.sh
```

The macOS script installs Homebrew if needed, then installs Node.js, LLVM, Zig, CMake, pkg-config, Rust through the official rustup installer, `cargo-xwin`, and `cargo-zigbuild`.

### Debian or Ubuntu Linux

```bash
bash scripts/setup-build-env-linux.sh
```

The Linux script uses `apt-get` and requires `sudo`. It supports Debian/Ubuntu-style systems and installs the GNU ARM64 linker, MinGW, LLVM, CMake, Rust, and project dependencies. If the distribution does not provide a `zig` package, the script downloads Zig 0.14.1 from the official Zig release archive.

### Windows

Run PowerShell as a user who can install software:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\setup-build-env.ps1
```

The Windows script uses `winget` when available and falls back to Chocolatey. It installs Node.js, Rustup, LLVM, Zig, CMake, and the required Cargo tools.

The Unix scripts must be executable when invoked directly:

```bash
chmod +x scripts/setup-build-env-macos.sh scripts/setup-build-env-linux.sh
```

## Development

Install dependencies on an already prepared machine:

```bash
npm install
```

Build the Rust worker and renderer, then launch Electron:

```bash
npm run dev
```

Useful individual commands:

```bash
npm run build:renderer       # TypeScript check and Vite production build
npm run build:backend        # Debug Rust worker for the current host
npm run build:backend:all    # Release Rust workers for every configured target
npm start                    # Launch the current Electron build
```

Build one platform locally, including both x64 and ARM64 artifacts:

```bash
npm run package:mac
npm run package:linux
npm run package:win
```

Use the command matching the host and toolchain you have installed. `package:mac` is intended for macOS, `package:linux` for Debian/Ubuntu Linux, and `package:win` for Windows. Each command builds only that platform instead of attempting unrelated SDKs.

The development worker is loaded from `rust-backend/target/debug`. Set `SUNRISE_WORKER_PATH` to override the worker executable path when running Electron manually or testing a packaged installation.

## Packaging

Build all configured targets from a capable macOS host with:

```bash
npm run package:all
```

This builds the renderer, stages release Rust workers, and invokes Electron Builder for macOS, Windows, and Linux x64/ARM64 packages. Output is written to `release/`.

On Linux and Windows, use the platform bootstrap script for a native local build. Those hosts do not provide Apple SDKs, so they package their own platform. The complete cross-platform build is handled by the GitHub Actions workflow below.

### Output formats

| Platform | Architectures | Artifacts |
| --- | --- | --- |
| macOS | x64, arm64 | DMG and ZIP |
| Windows | x64, arm64 | NSIS installer and ZIP |
| Linux | x64, arm64 | AppImage |

Rust target triples:

| Platform | x64 | ARM64 |
| --- | --- | --- |
| macOS | `x86_64-apple-darwin` | `aarch64-apple-darwin` |
| Windows | `x86_64-pc-windows-msvc` | `aarch64-pc-windows-msvc` |
| Linux | `x86_64-unknown-linux-gnu` | `aarch64-unknown-linux-gnu` |

Windows Rust workers are built through `cargo-xwin`. Linux workers use `cargo-zigbuild` so the ARM64 linker is available on supported hosts.

## GitHub Actions

[`.github/workflows/build.yml`](.github/workflows/build.yml) runs three native jobs:

1. macOS 14 builds both macOS architectures.
2. Ubuntu 24.04 builds both Linux architectures.
3. Windows 2022 builds both Windows architectures.

Each job checks out the source, installs its environment with the platform script, runs the matching `package:mac`, `package:linux`, or `package:win` command, and uploads `release/` as a workflow artifact. The workflow runs on pushes to `main`, pull requests, and manual dispatches.

Native runners are intentional. macOS packaging requires Apple SDKs, and native runners make linker and Electron Builder behavior predictable. The workflow does not sign installers. Configure platform-specific signing secrets and Electron Builder signing options before distributing release artifacts publicly.

## Architecture

```text
src/                    Vite renderer and UI assets
main.cjs                Electron main process
preload.cjs             Isolated renderer bridge
rust-backend/src/       Rust worker and installer logic
scripts/                Build and environment bootstrap scripts
build/backend/          Staged workers used by Electron Builder
dist/                   Vite output
release/                Electron Builder artifacts
```

The renderer communicates with Electron through the isolated preload bridge. Electron starts the Rust worker and forwards newline-delimited JSON requests, results, errors, and streamed operation events.

## Generated files

These directories are intentionally ignored by Git and can be deleted at any time:

- `node_modules/`
- `build/`
- `dist/`
- `release/`
- `rust-backend/target/`

The npm lockfile and `.cargo/config.toml` are source-controlled inputs and should be retained.

## Troubleshooting

### Rustup cannot be found

Open a new terminal after installing Rust, or load Cargo manually:

```bash
source "$HOME/.cargo/env"
```

The bootstrap scripts already perform this step when the file exists and fall back to `$HOME/.cargo/bin` when it does not.

### Linux bootstrap rejects the distribution

The supplied Linux script supports Debian and Ubuntu through `apt-get`. On another distribution, install equivalent Node.js, Rust, LLVM, Zig, CMake, MinGW, and ARM64 GNU linker packages manually before running the build commands.

### Windows package manager is unavailable

Install or enable `winget` or Chocolatey, then rerun the PowerShell bootstrap. A new PowerShell session may be required for newly installed commands to appear on `PATH`.

### Cross-build linker errors

Confirm that the target is installed with `rustup target list --installed`, that LLVM/Zig is on `PATH`, and that `cargo-xwin` or `cargo-zigbuild` is available. Apple targets must be built on macOS with an available Apple SDK.

### Packaged app cannot find the worker

Confirm that the matching worker was staged under `build/backend/<platform>-<arch>/` before packaging. For a deliberate override, set `SUNRISE_WORKER_PATH` to the worker executable.

## Security notes

The Electron window uses context isolation and disables Node integration in the renderer. External links are restricted to HTTP and HTTPS URLs. Release artifacts are currently unsigned; users may see operating-system warnings until signing and notarization are configured.
