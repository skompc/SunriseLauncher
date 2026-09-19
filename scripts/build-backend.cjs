const { cp, mkdir, rm } = require("node:fs/promises");
const { spawn } = require("node:child_process");
const path = require("node:path");

const targets = {
  "mac-x64": { platform: "darwin", arch: "x64", triple: "x86_64-apple-darwin" },
  "mac-arm64": { platform: "darwin", arch: "arm64", triple: "aarch64-apple-darwin" },
  "win-x64": { platform: "win32", arch: "x64", triple: "x86_64-pc-windows-msvc" },
  "win-arm64": { platform: "win32", arch: "arm64", triple: "aarch64-pc-windows-msvc" },
  "linux-x64": { platform: "linux", arch: "x64", triple: "x86_64-unknown-linux-gnu" },
  "linux-arm64": { platform: "linux", arch: "arm64", triple: "aarch64-unknown-linux-gnu" },
};

function run(command, args, env) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { env, stdio: "inherit" });
    child.on("error", reject);
    child.on("exit", (code) => {
      if (code === 0) resolve();
      else reject(new Error(`${command} exited with code ${code}`));
    });
  });
}

async function main() {
  const platform = process.argv.find((argument) => argument.startsWith("--platform="))?.slice("--platform=".length);
  const requested = process.argv.includes("--all")
    ? Object.keys(targets)
    : platform
      ? Object.entries(targets)
          .filter(([, target]) => target.platform === ({ mac: "darwin", win: "win32", linux: "linux" }[platform] ?? platform))
          .map(([name]) => name)
      : [process.argv.find((argument) => argument.startsWith("--target="))?.slice("--target=".length)];
  if (!requested[0] || requested.some((target) => !targets[target])) {
    throw new Error(`Choose --all, --platform=<mac|win|linux>, or --target=<${Object.keys(targets).join("|")}>.`);
  }

  const stagingRoot = path.resolve("build", "backend");
  await rm(stagingRoot, { recursive: true, force: true });

  for (const name of requested) {
    const target = targets[name];
    const binaryName = target.platform === "win32" ? "electron-worker.exe" : "electron-worker";
    const buildCommand = target.platform === "win32"
      ? ["cargo", "xwin"]
      : target.platform === "linux"
        ? ["cargo", "zigbuild"]
        : ["cargo"];
    const buildArguments = [...buildCommand.slice(1)];
    if (target.platform !== "linux") buildArguments.push("build");
    if (target.platform === "win32") buildArguments.push("--cross-compiler", "clang");
    const environment = target.platform === "win32"
      ? {
          ...process.env,
          PATH: `${process.env.HOMEBREW_PREFIX ?? (process.arch === "arm64" ? "/opt/homebrew" : "/usr/local")}/opt/llvm/bin:${process.env.PATH}`,
        }
      : process.env;
    await run(buildCommand[0], [
      ...buildArguments,
      "--release",
      "--target",
      target.triple,
      "--manifest-path",
      "rust-backend/Cargo.toml",
      "--bin",
      "electron-worker",
    ], environment);
    await mkdir(path.join(stagingRoot, name), { recursive: true });
    await cp(
      path.join("rust-backend", "target", target.triple, "release", binaryName),
      path.join(stagingRoot, name, binaryName),
    );
  }
}

main().catch((error) => {
  console.error(error.message);
  process.exitCode = 1;
});