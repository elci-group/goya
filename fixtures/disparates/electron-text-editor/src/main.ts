import { app, BrowserWindow, ipcMain, dialog } from 'electron';
import fs from 'node:fs/promises';
import path from 'node:path';

function createWindow(): void {
  const window = new BrowserWindow({
    width: 800,
    height: 600,
    useContentSize: true,
    title: 'Text Editor',
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

ipcMain.handle('fs:readFile', async (_event, filePath: string) => {
  try {
    const content = await fs.readFile(filePath, 'utf-8');
    return { ok: true, content };
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : String(error) };
  }
});

ipcMain.handle('fs:writeFile', async (_event, filePath: string, content: string) => {
  try {
    await fs.writeFile(filePath, content, 'utf-8');
    return { ok: true };
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : String(error) };
  }
});

ipcMain.handle('dialog:openFile', async () => {
  const result = await dialog.showOpenDialog({
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

ipcMain.handle('dialog:saveFile', async () => {
  const result = await dialog.showSaveDialog({
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

app.whenReady().then(createWindow);

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit();
  }
});