import { contextBridge, ipcRenderer } from 'electron';

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

contextBridge.exposeInMainWorld('fs', {
  readdir: (dirPath: string): Promise<ReaddirResult> =>
    ipcRenderer.invoke('fs:readdir', dirPath),
  readFile: (filePath: string): Promise<ReadFileResult> =>
    ipcRenderer.invoke('fs:readFile', filePath),
});

contextBridge.exposeInMainWorld('dialog', {
  openDirectory: (): Promise<OpenDirectoryResult> =>
    ipcRenderer.invoke('dialog:openDirectory'),
});