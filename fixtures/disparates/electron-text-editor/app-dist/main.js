"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const electron_1 = require("electron");
const promises_1 = __importDefault(require("node:fs/promises"));
const node_path_1 = __importDefault(require("node:path"));
function createWindow() {
    const window = new electron_1.BrowserWindow({
        width: 800,
        height: 600,
        useContentSize: true,
        title: 'Text Editor',
        webPreferences: {
            preload: node_path_1.default.join(__dirname, 'preload.js'),
            contextIsolation: true,
            nodeIntegration: false,
            sandbox: true,
        },
    });
    // In development, load from Vite dev server; in production, load built file
    if (process.env.NODE_ENV === 'development') {
        window.loadURL('http://localhost:5173');
    }
    else {
        window.loadFile(node_path_1.default.join(__dirname, '../dist/renderer/index.html'));
    }
}
electron_1.ipcMain.handle('fs:readFile', async (_event, filePath) => {
    try {
        const content = await promises_1.default.readFile(filePath, 'utf-8');
        return { ok: true, content };
    }
    catch (error) {
        return { ok: false, error: error instanceof Error ? error.message : String(error) };
    }
});
electron_1.ipcMain.handle('fs:writeFile', async (_event, filePath, content) => {
    try {
        await promises_1.default.writeFile(filePath, content, 'utf-8');
        return { ok: true };
    }
    catch (error) {
        return { ok: false, error: error instanceof Error ? error.message : String(error) };
    }
});
electron_1.ipcMain.handle('dialog:openFile', async () => {
    const result = await electron_1.dialog.showOpenDialog({
        properties: ['openFile'],
        filters: [
            { name: 'Text Files', extensions: ['txt', 'md', 'json', 'js', 'ts', 'tsx', 'html', 'css'] },
            { name: 'All Files', extensions: ['*'] },
        ],
    });
    if (result.canceled || result.filePaths.length === 0) {
        return { ok: true, canceled: true, filePath: null };
    }
    return { ok: true, canceled: false, filePath: result.filePaths[0] };
});
electron_1.ipcMain.handle('dialog:saveFile', async () => {
    const result = await electron_1.dialog.showSaveDialog({
        filters: [
            { name: 'Text Files', extensions: ['txt', 'md', 'json', 'js', 'ts', 'tsx', 'html', 'css'] },
            { name: 'All Files', extensions: ['*'] },
        ],
    });
    if (result.canceled || !result.filePath) {
        return { ok: true, canceled: true, filePath: null };
    }
    return { ok: true, canceled: false, filePath: result.filePath };
});
electron_1.app.whenReady().then(createWindow);
electron_1.app.on('window-all-closed', () => {
    if (process.platform !== 'darwin') {
        electron_1.app.quit();
    }
});
