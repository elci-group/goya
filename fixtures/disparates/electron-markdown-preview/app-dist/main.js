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
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
const electron_1 = require("electron");
const fs = __importStar(require("node:fs/promises"));
const path = __importStar(require("node:path"));
const isDev = process.env.NODE_ENV === 'development';
function createWindow() {
    const win = new electron_1.BrowserWindow({
        width: 1200,
        height: 800,
        webPreferences: {
            preload: path.join(__dirname, 'preload.js'),
            contextIsolation: true,
            nodeIntegration: false,
            sandbox: false,
        },
    });
    if (isDev) {
        win.loadURL('http://localhost:5173');
        win.webContents.openDevTools();
    }
    else {
        win.loadFile(path.join(__dirname, '../renderer/index.html'));
    }
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
// IPC handlers for filesystem operations
electron_1.ipcMain.handle('fs:readFile', async (_event, filePath) => {
    try {
        const content = await fs.readFile(filePath, 'utf-8');
        return content;
    }
    catch (error) {
        console.error('Error reading file:', error);
        throw error;
    }
});
electron_1.ipcMain.handle('fs:writeFile', async (_event, filePath, content) => {
    try {
        await fs.writeFile(filePath, content, 'utf-8');
    }
    catch (error) {
        console.error('Error writing file:', error);
        throw error;
    }
});
electron_1.ipcMain.handle('dialog:openFile', async () => {
    const result = await electron_1.dialog.showOpenDialog({
        properties: ['openFile'],
        filters: [
            { name: 'Markdown Files', extensions: ['md', 'markdown', 'txt'] },
            { name: 'All Files', extensions: ['*'] },
        ],
    });
    if (result.canceled || result.filePaths.length === 0) {
        return null;
    }
    return result.filePaths[0];
});
electron_1.ipcMain.handle('dialog:saveFile', async () => {
    const result = await electron_1.dialog.showSaveDialog({
        filters: [
            { name: 'Markdown Files', extensions: ['md', 'markdown'] },
            { name: 'All Files', extensions: ['*'] },
        ],
        defaultPath: 'untitled.md',
    });
    if (result.canceled || !result.filePath) {
        return null;
    }
    return result.filePath;
});
//# sourceMappingURL=main.js.map