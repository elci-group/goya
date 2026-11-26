import { contextBridge, ipcRenderer } from 'electron';

contextBridge.exposeInMainWorld('counter', {
  add: (amountText: string) => ipcRenderer.invoke('counter:add', amountText),
  get: () => ipcRenderer.invoke('counter:get'),
});