const { contextBridge, ipcRenderer } = require("electron")

contextBridge.exposeInMainWorld("desktopAPI", {
  readSettings: () => ipcRenderer.invoke("settings:read"),
  sendSync: (value) => ipcRenderer.send("sync-message", value),
})
