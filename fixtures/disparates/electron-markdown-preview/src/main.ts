import { app, BrowserWindow, ipcMain, dialog } from 'electron';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';

// __dirname is available globally in Electron main process
declare const __dirname: string;

const isDev = process.env.NODE_ENV === 'development';

function createWindow(): void {
  const win = new BrowserWindow({
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
  } else {
    win.loadFile(path.join(__dirname, '../renderer/index.html'));
  }
}

app.whenReady().then(() => {
  createWindow();

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow();
    }
  });
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit();
  }
});

// IPC handlers for filesystem operations
ipcMain.handle('fs:readFile', async (_event, filePath: string): Promise<string> => {
  try {
    const content = await fs.readFile(filePath, 'utf-8');
    return content;
  } catch (error) {
    console.error('Error reading file:', error);
    throw error;
  }
});

ipcMain.handle('fs:writeFile', async (_event, filePath: string, content: string): Promise<void> => {
  try {
    await fs.writeFile(filePath, content, 'utf-8');
  } catch (error) {
    console.error('Error writing file:', error);
    throw error;
  }
});

ipcMain.handle('dialog:openFile', async (): Promise<string | null> => {
  const result = await dialog.showOpenDialog({
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

ipcMain.handle('dialog:saveFile', async (): Promise<string | null> => {
  const result = await dialog.showSaveDialog({
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