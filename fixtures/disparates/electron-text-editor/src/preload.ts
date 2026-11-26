import { contextBridge, ipcRenderer } from 'electron';

contextBridge.exposeInMainWorld('fs', {
  readFile: (filePath: string) => ipcRenderer.invoke('fs:readFile', filePath),
  writeFile: (filePath: string, content: string) => ipcRenderer.invoke('fs:writeFile', filePath, content),
});

contextBridge.exposeInMainWorld('dialog', {
  openFile: () => ipcRenderer.invoke('dialog:openFile'),
  saveFile: () => ipcRenderer.invoke('dialog:saveFile'),
});