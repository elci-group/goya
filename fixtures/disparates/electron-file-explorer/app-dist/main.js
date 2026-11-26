"use strict";
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || function (mod) {
    if (mod && mod.__esModule) return mod;
    var result = {};
    if (mod != null) for (var k in mod) if (k !== "default" && Object.prototype.hasOwnProperty.call(mod, k)) __createBinding(result, mod, k);
    __setModuleDefault(result, mod);
    return result;
};
Object.defineProperty(exports, "__esModule", { value: true });
const electron_1 = require("electron");
const fs = __importStar(require("node:fs/promises"));
const path = __importStar(require("node:path"));
function createWindow() {
    const window = new electron_1.BrowserWindow({
        width: 800,
        height: 600,
        useContentSize: true,
        title: 'File Explorer',
        webPreferences: {
            preload: path.join(__dirname, 'preload.js'),
            contextIsolation: true,
            nodeIntegration: false,
            sandbox: true,
        },
    });
    if (process.env.NODE_ENV === 'development') {
        window.loadURL('http://localhost:5173');
    }
    else {
        window.loadFile(path.join(__dirname, '../dist/renderer/index.html'));
    }
}
electron_1.ipcMain.handle('fs:readdir', async (_event, dirPath) => {
    try {
        const entries = await fs.readdir(dirPath, { withFileTypes: true });
        const result = entries.map((entry) => ({
            name: entry.name,
            isDirectory: entry.isDirectory(),
            path: path.join(dirPath, entry.name),
        }));
        return { ok: true, entries: result };
    }
    catch (error) {
        return { ok: false, error: error instanceof Error ? error.message : 'Unknown error' };
    }
});
electron_1.ipcMain.handle('fs:readFile', async (_event, filePath) => {
    try {
        const content = await fs.readFile(filePath, 'utf-8');
        return { ok: true, content };
    }
    catch (error) {
        return { ok: false, error: error instanceof Error ? error.message : 'Unknown error' };
    }
});
electron_1.ipcMain.handle('dialog:openDirectory', async () => {
    try {
        const result = await electron_1.dialog.showOpenDialog({
            properties: ['openDirectory', 'createDirectory'],
        });
        if (result.canceled || result.filePaths.length === 0) {
            return { ok: false, error: 'Canceled' };
        }
        return { ok: true, path: result.filePaths[0] };
    }
    catch (error) {
        return { ok: false, error: error instanceof Error ? error.message : 'Unknown error' };
    }
});
electron_1.app.whenReady().then(createWindow);
electron_1.app.on('window-all-closed', () => {
    if (process.platform !== 'darwin') {
        electron_1.app.quit();
    }
});
