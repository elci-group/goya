/// <reference types="vite/client" />

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

interface WindowFs {
  readdir: (dirPath: string) => Promise<ReaddirResult>;
  readFile: (filePath: string) => Promise<ReadFileResult>;
}

interface WindowDialog {
  openDirectory: () => Promise<OpenDirectoryResult>;
}

interface Window {
  fs: WindowFs;
  dialog: WindowDialog;
}