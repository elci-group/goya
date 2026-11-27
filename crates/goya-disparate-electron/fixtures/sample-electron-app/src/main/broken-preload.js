const { contextBridge, ipcRenderer } = require("electron")

contextBridge.exposeInMainWorld("fallbackAPI", {
  ping: () => ipcRenderer.invoke("fallback:ping"),
  done: (value) => ipcRenderer.send("fallback:done", value),
})

this line is deliberately not valid javascript @@
