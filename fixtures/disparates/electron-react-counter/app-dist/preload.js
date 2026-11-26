"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
const electron_1 = require("electron");
electron_1.contextBridge.exposeInMainWorld('counter', {
    add: (amountText) => electron_1.ipcRenderer.invoke('counter:add', amountText),
    get: () => electron_1.ipcRenderer.invoke('counter:get'),
});
