import { useState, useCallback } from 'react';

interface FileOperationResult {
  ok: boolean;
  content?: string;
  filePath?: string;
  canceled?: boolean;
  error?: string;
}

function App(): JSX.Element {
  const [text, setText] = useState('');
  const [filePath, setFilePath] = useState<string | null>(null);
  const [status, setStatus] = useState('Ready');
  const [pending, setPending] = useState<Promise<void>>(Promise.resolve());

  const handleOpen = useCallback(() => {
    setPending((prev) =>
      prev.then(async () => {
        setStatus('Opening...');
        const result: FileOperationResult = await window.dialog.openFile();
        if (result.ok && !result.canceled && result.filePath) {
          const readResult: FileOperationResult = await window.fs.readFile(result.filePath);
          if (readResult.ok && readResult.content !== undefined) {
            setText(readResult.content);
            setFilePath(result.filePath);
            setStatus(`Opened: ${result.filePath}`);
          } else {
            setStatus(`Error: ${readResult.error}`);
          }
        } else if (result.ok && result.canceled) {
          setStatus('Open canceled');
        } else {
          setStatus(`Error: ${result.error}`);
        }
      })
    );
  }, []);

  const handleSave = useCallback(() => {
    setPending((prev) =>
      prev.then(async () => {
        setStatus('Saving...');
        let targetPath = filePath;
        if (!targetPath) {
          const saveResult: FileOperationResult = await window.dialog.saveFile();
          if (saveResult.ok && !saveResult.canceled && saveResult.filePath) {
            targetPath = saveResult.filePath;
          } else if (saveResult.ok && saveResult.canceled) {
            setStatus('Save canceled');
            return;
          } else {
            setStatus(`Error: ${saveResult.error}`);
            return;
          }
        }
        if (targetPath) {
          const writeResult: FileOperationResult = await window.fs.writeFile(targetPath, text);
          if (writeResult.ok) {
            setFilePath(targetPath);
            setStatus(`Saved: ${targetPath}`);
          } else {
            setStatus(`Error: ${writeResult.error}`);
          }
        }
      })
    );
  }, [filePath, text]);

  const handleTextChange = useCallback((event: React.ChangeEvent<HTMLTextAreaElement>) => {
    setText(event.target.value);
  }, []);

  return (
    <main id="window-root">
      <header id="window-toolbar">
        <button id="window-open" type="button" onClick={handleOpen} aria-label="Open file">
          Open
        </button>
        <button id="window-save" type="button" onClick={handleSave} aria-label="Save file">
          Save
        </button>
        <span id="window-filepath" aria-live="polite">
          {filePath ? filePath : 'Untitled'}
        </span>
      </header>
      <textarea
        id="window-editor"
        value={text}
        onChange={handleTextChange}
        aria-label="Text editor"
        spellCheck={false}
      />
      <footer id="window-status" aria-live="polite">
        {status}
      </footer>
    </main>
  );
}

export default App;