"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
const electron_1 = require("electron");
electron_1.contextBridge.exposeInMainWorld('fs', {
    readdir: (dirPath) => electron_1.ipcRenderer.invoke('fs:readdir', dirPath),
    readFile: (filePath) => electron_1.ipcRenderer.invoke('fs:readFile', filePath),
});
electron_1.contextBridge.exposeInMainWorld('dialog', {
    openDirectory: () => electron_1.ipcRenderer.invoke('dialog:openDirectory'),
});
