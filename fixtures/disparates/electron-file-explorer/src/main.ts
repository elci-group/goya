import { app, BrowserWindow, ipcMain, dialog } from 'electron';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';

interface DirectoryEntry {
  name: string;
  isDirectory: boolean;
  path: string;
}

interface ReaddirResult {
  ok: boolean;
  entries?: DirectoryEntry[];
  error?: string;
}

interface ReadFileResult {
  ok: boolean;
  content?: string;
  error?: string;
}

interface OpenDirectoryResult {
  ok: boolean;
  path?: string;
  error?: string;
}

function createWindow(): void {
  const window = new BrowserWindow({
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
  } else {
    window.loadFile(path.join(__dirname, '../dist/renderer/index.html'));
  }
}

ipcMain.handle('fs:readdir', async (_event, dirPath: string): Promise<ReaddirResult> => {
  try {
    const entries = await fs.readdir(dirPath, { withFileTypes: true });
    const result: DirectoryEntry[] = entries.map((entry) => ({
      name: entry.name,
      isDirectory: entry.isDirectory(),
      path: path.join(dirPath, entry.name),
    }));
    return { ok: true, entries: result };
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : 'Unknown error' };
  }
});

ipcMain.handle('fs:readFile', async (_event, filePath: string): Promise<ReadFileResult> => {
  try {
    const content = await fs.readFile(filePath, 'utf-8');
    return { ok: true, content };
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : 'Unknown error' };
  }
});

ipcMain.handle('dialog:openDirectory', async (): Promise<OpenDirectoryResult> => {
  try {
    const result = await dialog.showOpenDialog({
      properties: ['openDirectory', 'createDirectory'],
    });
    if (result.canceled || result.filePaths.length === 0) {
      return { ok: false, error: 'Canceled' };
    }
    return { ok: true, path: result.filePaths[0] };
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : 'Unknown error' };
  }
});

app.whenReady().then(createWindow);

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit();
  }
});