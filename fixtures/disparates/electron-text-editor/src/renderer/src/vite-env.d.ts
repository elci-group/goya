/// <reference types="vite/client" />

interface Window {
  fs: {
    readFile: (filePath: string) => Promise<{ ok: boolean; content?: string; error?: string }>;
    writeFile: (filePath: string, content: string) => Promise<{ ok: boolean; error?: string }>;
  };
  dialog: {
    openFile: () => Promise<{ ok: boolean; canceled?: boolean; filePath?: string | null; error?: string }>;
    saveFile: () => Promise<{ ok: boolean; canceled?: boolean; filePath?: string | null; error?: string }>;
  };
}