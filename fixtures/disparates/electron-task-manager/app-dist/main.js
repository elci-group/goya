"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const electron_1 = require("electron");
const path_1 = __importDefault(require("path"));
const tasks = [];
let mainWindow = null;
function generateId() {
    return `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
}
function createWindow() {
    mainWindow = new electron_1.BrowserWindow({
        width: 800,
        height: 600,
        webPreferences: {
            preload: path_1.default.join(__dirname, 'preload.js'),
            contextIsolation: true,
            nodeIntegration: false,
        },
    });
    if (process.env.NODE_ENV === 'development') {
        mainWindow.loadURL('http://localhost:5173');
        mainWindow.webContents.openDevTools();
    }
    else {
        mainWindow.loadFile(path_1.default.join(__dirname, 'renderer/index.html'));
    }
    mainWindow.on('closed', () => {
        mainWindow = null;
    });
}
electron_1.app.whenReady().then(() => {
    createWindow();
    electron_1.app.on('activate', () => {
        if (electron_1.BrowserWindow.getAllWindows().length === 0) {
            createWindow();
        }
    });
});
electron_1.app.on('window-all-closed', () => {
    if (process.platform !== 'darwin') {
        electron_1.app.quit();
    }
});
// IPC Handlers
electron_1.ipcMain.handle('tasks:list', () => {
    return [...tasks].sort((a, b) => b.createdAt - a.createdAt);
});
electron_1.ipcMain.handle('tasks:add', (_event, title) => {
    const task = {
        id: generateId(),
        title: title.trim(),
        completed: false,
        createdAt: Date.now(),
    };
    tasks.push(task);
    return task;
});
electron_1.ipcMain.handle('tasks:toggle', (_event, id) => {
    const index = tasks.findIndex((t) => t.id === id);
    if (index === -1)
        return null;
    tasks[index] = { ...tasks[index], completed: !tasks[index].completed };
    return tasks[index];
});
electron_1.ipcMain.handle('tasks:delete', (_event, id) => {
    const index = tasks.findIndex((t) => t.id === id);
    if (index === -1)
        return false;
    tasks.splice(index, 1);
    return true;
});
//# sourceMappingURL=main.js.map