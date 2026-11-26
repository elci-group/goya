"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const electron_1 = require("electron");
const node_path_1 = __importDefault(require("node:path"));
const I64_MIN = -(2n ** 63n);
const I64_MAX = 2n ** 63n - 1n;
// Session state: held in memory by the main process and lost on exit.
let count = 0n;
function parseCanonicalI64(text) {
    if (typeof text !== 'string' || !/^(0|-?[1-9][0-9]*)$/.test(text)) {
        return null;
    }
    const value = BigInt(text);
    return value >= I64_MIN && value <= I64_MAX ? value : null;
}
electron_1.ipcMain.handle('counter:add', (_event, amountText) => {
    const amount = parseCanonicalI64(amountText);
    if (amount === null) {
        return { ok: false, error: 'amount', count: count.toString() };
    }
    const next = count + amount;
    if (next < I64_MIN || next > I64_MAX) {
        return { ok: false, error: 'overflow', count: count.toString() };
    }
    count = next;
    return { ok: true, count: count.toString() };
});
electron_1.ipcMain.handle('counter:get', () => count.toString());
function createWindow() {
    const window = new electron_1.BrowserWindow({
        width: 320,
        height: 160,
        useContentSize: true,
        title: 'Count',
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
        window.loadFile(node_path_1.default.join(__dirname, 'renderer/index.html'));
    }
}
electron_1.app.whenReady().then(createWindow);
electron_1.app.on('window-all-closed', () => {
    if (process.platform !== 'darwin') {
        electron_1.app.quit();
    }
});
