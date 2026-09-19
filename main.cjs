const { app, BrowserWindow, dialog, ipcMain, shell } = require("electron");
const { spawn } = require("node:child_process");
const readline = require("node:readline");
const path = require("node:path");

const renderer = path.join(__dirname, "dist", "index.html");
let worker;
let nextRequestId = 1;
const pending = new Map();

function startWorker() {
  const workerName = process.platform === "win32" ? "electron-worker.exe" : "electron-worker";
  const packagedWorker = path.join(process.resourcesPath, "backend", workerName);
  const developmentWorker = path.join(__dirname, "rust-backend", "target", "debug", workerName);
  const workerPath = process.env.SUNRISE_WORKER_PATH ?? (app.isPackaged ? packagedWorker : developmentWorker);
  worker = spawn(workerPath, [], { stdio: ["pipe", "pipe", "inherit"] });
  const lines = readline.createInterface({ input: worker.stdout });
  lines.on("line", (line) => {
    let message;
    try {
      message = JSON.parse(line);
    } catch {
      return;
    }
    if (message.event) {
      for (const window of BrowserWindow.getAllWindows()) {
        window.webContents.send("backend:event", message.event);
      }
      return;
    }
    const request = pending.get(message.id);
    if (!request) return;
    pending.delete(message.id);
    if (message.error) request.reject(new Error(message.error));
    else request.resolve(message.result);
  });
  worker.on("error", (error) => {
    for (const request of pending.values()) request.reject(error);
    pending.clear();
  });
}

function invokeBackend(method, params) {
  if (!worker) return Promise.reject(new Error("Rust backend worker is not running."));
  const id = nextRequestId++;
  return new Promise((resolve, reject) => {
    pending.set(id, { resolve, reject });
    worker.stdin.write(`${JSON.stringify({ id, method, params })}\n`);
  });
}

function registerIpc() {
  ipcMain.handle("backend:invoke", (_event, method, params) => invokeBackend(method, params));

  ipcMain.handle("window:close", (event) => {
    BrowserWindow.fromWebContents(event.sender)?.close();
  });

  ipcMain.handle("window:minimize", (event) => {
    BrowserWindow.fromWebContents(event.sender)?.minimize();
  });

  ipcMain.handle("window:toggle-maximize", (event) => {
    const window = BrowserWindow.fromWebContents(event.sender);
    if (!window) return false;
    if (window.isMaximized()) window.unmaximize();
    else window.maximize();
    return window.isMaximized();
  });

  ipcMain.handle("dialog:open-directory", async () => {
    const result = await dialog.showOpenDialog({
      properties: ["openDirectory", "createDirectory"],
      title: "Choose a Sunrise installation folder",
    });
    return result.canceled ? null : result.filePaths[0] ?? null;
  });

  ipcMain.handle("shell:open-external", (_event, url) => {
    if (typeof url !== "string" || !/^https?:\/\//i.test(url)) {
      throw new Error("Only HTTP and HTTPS URLs can be opened.");
    }
    return shell.openExternal(url);
  });
}

function createWindow() {
  const window = new BrowserWindow({
    width: 1180,
    height: 680,
    minWidth: 860,
    minHeight: 560,
    frame: false,
    backgroundColor: "#090b11",
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
      preload: path.join(__dirname, "preload.cjs"),
    },
  });

  window.loadFile(renderer);
}

app.whenReady().then(() => {
  registerIpc();
  startWorker();
  createWindow();
  app.on("activate", () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow();
  });
});

app.on("window-all-closed", () => {
  if (process.platform !== "darwin") app.quit();
});
