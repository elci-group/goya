"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
const electron_1 = require("electron");
electron_1.contextBridge.exposeInMainWorld('fs', {
    readFile: (filePath) => electron_1.ipcRenderer.invoke('fs:readFile', filePath),
    writeFile: (filePath, content) => electron_1.ipcRenderer.invoke('fs:writeFile', filePath, content),
});
electron_1.contextBridge.exposeInMainWorld('dialog', {
    openFile: () => electron_1.ipcRenderer.invoke('dialog:openFile'),
    saveFile: () => electron_1.ipcRenderer.invoke('dialog:saveFile'),
});
