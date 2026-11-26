'use strict';

const path = require('node:path');
const { app, BrowserWindow, ipcMain } = require('electron');

const I64_MIN = -(2n ** 63n);
const I64_MAX = 2n ** 63n - 1n;

// Session state: held in memory by the main process and lost on exit.
let count = 0n;

// The amount must be a canonical decimal i64: no sign on zero, no leading
// zeros, no whitespace. Anything else is refused and the count is unchanged.
function parseCanonicalI64(text) {
  if (typeof text !== 'string' || !/^(0|-?[1-9][0-9]*)$/.test(text)) {
    return null;
  }
  const value = BigInt(text);
  return value >= I64_MIN && value <= I64_MAX ? value : null;
}

ipcMain.handle('counter:add', (_event, amountText) => {
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

function createWindow() {
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
  window.loadFile(path.join(__dirname, 'index.html'));
}

app.whenReady().then(createWindow);

app.on('window-all-closed', () => {
  app.quit();
});
