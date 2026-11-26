import { contextBridge, ipcRenderer } from 'electron';

const electronAPI = {
  fs: {
    readFile: (filePath: string): Promise<string> =>
      ipcRenderer.invoke('fs:readFile', filePath),
    writeFile: (filePath: string, content: string): Promise<void> =>
      ipcRenderer.invoke('fs:writeFile', filePath, content),
  },
  dialog: {
    openFile: (): Promise<string | null> =>
      ipcRenderer.invoke('dialog:openFile'),
    saveFile: (): Promise<string | null> =>
      ipcRenderer.invoke('dialog:saveFile'),
  },
};

contextBridge.exposeInMainWorld('electronAPI', electronAPI);

export type ElectronAPI = typeof electronAPI;