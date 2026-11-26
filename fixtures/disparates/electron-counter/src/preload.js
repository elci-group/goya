'use strict';

const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('counter', {
  add: (amountText) => ipcRenderer.invoke('counter:add', amountText),
  get: () => ipcRenderer.invoke('counter:get'),
});
