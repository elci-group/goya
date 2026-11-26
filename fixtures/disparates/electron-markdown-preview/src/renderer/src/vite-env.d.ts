/// <reference types="vite/client" />

interface ElectronAPI {
  fs: {
    readFile: (filePath: string) => Promise<string>;
    writeFile: (filePath: string, content: string) => Promise<void>;
  };
  dialog: {
    openFile: () => Promise<string | null>;
    saveFile: () => Promise<string | null>;
  };
}

interface Window {
  electronAPI: ElectronAPI;
}