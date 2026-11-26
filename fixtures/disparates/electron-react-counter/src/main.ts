import { app, BrowserWindow, ipcMain } from 'electron';
import path from 'node:path';

const I64_MIN = -(2n ** 63n);
const I64_MAX = 2n ** 63n - 1n;

// Session state: held in memory by the main process and lost on exit.
let count = 0n;

function parseCanonicalI64(text: string): bigint | null {
  if (typeof text !== 'string' || !/^(0|-?[1-9][0-9]*)$/.test(text)) {
    return null;
  }
  const value = BigInt(text);
  return value >= I64_MIN && value <= I64_MAX ? value : null;
}

ipcMain.handle('counter:add', (_event, amountText: string) => {
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

ipcMain.handle('counter:get', () => count.toString());

function createWindow(): void {
  const window = new BrowserWindow({
    width: 320,
    height: 160,
    useContentSize: true,
    title: 'Count',
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
    },
  });
  // In development, load from Vite dev server; in production, load built file
  if (process.env.NODE_ENV === 'development') {
    window.loadURL('http://localhost:5173');
  } else {
    window.loadFile(path.join(__dirname, '../dist/renderer/index.html'));
  }
}

app.whenReady().then(createWindow);

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit();
  }
});