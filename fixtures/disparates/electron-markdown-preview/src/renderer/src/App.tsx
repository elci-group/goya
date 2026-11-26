import React, { useState, useCallback } from 'react';
import { marked } from 'marked';

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

declare global {
  interface Window {
    electronAPI: ElectronAPI;
  }
}

const App: React.FC = () => {
  const [content, setContent] = useState<string>('');
  const [filePath, setFilePath] = useState<string | null>(null);
  const [isModified, setIsModified] = useState(false);

  const handleOpen = useCallback(async (): Promise<void> => {
    try {
      const path = await window.electronAPI.dialog.openFile();
      if (path) {
        const fileContent = await window.electronAPI.fs.readFile(path);
        setContent(fileContent);
        setFilePath(path);
        setIsModified(false);
      }
    } catch (error) {
      console.error('Failed to open file:', error);
      alert('Failed to open file');
    }
  }, []);

  const handleSave = useCallback(async (): Promise<void> => {
    try {
      let targetPath = filePath;
      if (!targetPath) {
        targetPath = await window.electronAPI.dialog.saveFile();
      }
      if (targetPath) {
        await window.electronAPI.fs.writeFile(targetPath, content);
        setFilePath(targetPath);
        setIsModified(false);
      }
    } catch (error) {
      console.error('Failed to save file:', error);
      alert('Failed to save file');
    }
  }, [content, filePath]);

  const handleSaveAs = useCallback(async (): Promise<void> => {
    try {
      const targetPath = await window.electronAPI.dialog.saveFile();
      if (targetPath) {
        await window.electronAPI.fs.writeFile(targetPath, content);
        setFilePath(targetPath);
        setIsModified(false);
      }
    } catch (error) {
      console.error('Failed to save file:', error);
      alert('Failed to save file');
    }
  }, [content]);

  const handleContentChange = useCallback((newContent: string): void => {
    setContent(newContent);
    setIsModified(true);
  }, []);

  const htmlContent = marked.parse(content, { async: false }) as string;

  return (
    <div className="app">
      <header className="toolbar">
        <div className="toolbar-group">
          <button onClick={handleOpen} title="Open (Ctrl+O)">
            Open
          </button>
          <button onClick={handleSave} title="Save (Ctrl+S)" disabled={!isModified}>
            Save
          </button>
          <button onClick={handleSaveAs} title="Save As (Ctrl+Shift+S)">
            Save As
          </button>
        </div>
        <div className="toolbar-group">
          {filePath && <span className="file-path" title={filePath}>{filePath}</span>}
          {isModified && <span className="modified-indicator">●</span>}
        </div>
      </header>

      <div className="split-view">
        <section className="editor-pane">
          <textarea
            className="editor"
            value={content}
            onChange={(e) => handleContentChange(e.target.value)}
            placeholder="Start typing Markdown here..."
            spellCheck={false}
          />
        </section>

        <div className="divider" />

        <section className="preview-pane">
          <div
            className="preview"
            dangerouslySetInnerHTML={{ __html: htmlContent }}
          />
        </section>
      </div>
    </div>
  );
};

export default App;