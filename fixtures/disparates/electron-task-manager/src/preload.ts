import { contextBridge, ipcRenderer } from 'electron';

interface Task {
  id: string;
  title: string;
  completed: boolean;
  createdAt: number;
}

const tasksAPI = {
  list: (): Promise<Task[]> => ipcRenderer.invoke('tasks:list'),
  add: (title: string): Promise<Task> => ipcRenderer.invoke('tasks:add', title),
  toggle: (id: string): Promise<Task | null> => ipcRenderer.invoke('tasks:toggle', id),
  delete: (id: string): Promise<boolean> => ipcRenderer.invoke('tasks:delete', id),
};

contextBridge.exposeInMainWorld('tasks', tasksAPI);

declare global {
  interface Window {
    tasks: typeof tasksAPI;
  }
}