"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
const electron_1 = require("electron");
const electronAPI = {
    fs: {
        readFile: (filePath) => electron_1.ipcRenderer.invoke('fs:readFile', filePath),
        writeFile: (filePath, content) => electron_1.ipcRenderer.invoke('fs:writeFile', filePath, content),
    },
    dialog: {
        openFile: () => electron_1.ipcRenderer.invoke('dialog:openFile'),
        saveFile: () => electron_1.ipcRenderer.invoke('dialog:saveFile'),
    },
};
electron_1.contextBridge.exposeInMainWorld('electronAPI', electronAPI);
//# sourceMappingURL=preload.js.map