"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
const electron_1 = require("electron");
const tasksAPI = {
    list: () => electron_1.ipcRenderer.invoke('tasks:list'),
    add: (title) => electron_1.ipcRenderer.invoke('tasks:add', title),
    toggle: (id) => electron_1.ipcRenderer.invoke('tasks:toggle', id),
    delete: (id) => electron_1.ipcRenderer.invoke('tasks:delete', id),
};
electron_1.contextBridge.exposeInMainWorld('tasks', tasksAPI);
//# sourceMappingURL=preload.js.map