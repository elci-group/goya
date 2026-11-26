import { app, BrowserWindow, ipcMain } from 'electron';
import path from 'path';

interface Task {
  id: string;
  title: string;
  completed: boolean;
  createdAt: number;
}

const tasks: Task[] = [];
let mainWindow: BrowserWindow | null = null;

function generateId(): string {
  return `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
}

function createWindow(): void {
  mainWindow = new BrowserWindow({
    width: 800,
    height: 600,
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
    },
  });

  if (process.env.NODE_ENV === 'development') {
    mainWindow.loadURL('http://localhost:5173');
    mainWindow.webContents.openDevTools();
  } else {
    mainWindow.loadFile(path.join(__dirname, 'renderer/index.html'));
  }

  mainWindow.on('closed', () => {
    mainWindow = null;
  });
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

// IPC Handlers
ipcMain.handle('tasks:list', (): Task[] => {
  return [...tasks].sort((a, b) => b.createdAt - a.createdAt);
});

ipcMain.handle('tasks:add', (_event, title: string): Task => {
  const task: Task = {
    id: generateId(),
    title: title.trim(),
    completed: false,
    createdAt: Date.now(),
  };
  tasks.push(task);
  return task;
});

ipcMain.handle('tasks:toggle', (_event, id: string): Task | null => {
  const index = tasks.findIndex((t) => t.id === id);
  if (index === -1) return null;
  tasks[index] = { ...tasks[index], completed: !tasks[index].completed };
  return tasks[index];
});

ipcMain.handle('tasks:delete', (_event, id: string): boolean => {
  const index = tasks.findIndex((t) => t.id === id);
  if (index === -1) return false;
  tasks.splice(index, 1);
  return true;
});