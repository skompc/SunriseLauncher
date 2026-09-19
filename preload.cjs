const { contextBridge, ipcRenderer } = require("electron");

contextBridge.exposeInMainWorld("electronHost", {
  invoke: (method, params) => ipcRenderer.invoke("backend:invoke", method, params),
  onOperationEvent: (callback) => {
    const listener = (_event, message) => callback(message);
    ipcRenderer.on("backend:event", listener);
    return () => ipcRenderer.removeListener("backend:event", listener);
  },
  close: () => ipcRenderer.invoke("window:close"),
  minimize: () => ipcRenderer.invoke("window:minimize"),
  toggleMaximize: () => ipcRenderer.invoke("window:toggle-maximize"),
  openDirectory: () => ipcRenderer.invoke("dialog:open-directory"),
  openExternal: (url) => ipcRenderer.invoke("shell:open-external", url),
});
