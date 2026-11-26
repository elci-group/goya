import { useState, useEffect, useCallback, useMemo } from 'react';

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

declare global {
  interface Window {
    fs: WindowFs;
    dialog: WindowDialog;
  }
}

interface TreeNode {
  name: string;
  path: string;
  isDirectory: boolean;
  children?: TreeNode[];
  expanded?: boolean;
  loading?: boolean;
  error?: string;
}

function App(): JSX.Element {
  const [rootPath, setRootPath] = useState<string>('');
  const [tree, setTree] = useState<TreeNode | null>(null);
  const [selectedFile, setSelectedFile] = useState<string>('');
  const [fileContent, setFileContent] = useState<string>('');
  const [loading, setLoading] = useState<boolean>(false);
  const [error, setError] = useState<string>('');

  const loadDirectory = useCallback(async (dirPath: string): Promise<TreeNode> => {
    const result: ReaddirResult = await window.fs.readdir(dirPath);
    if (!result.ok || !result.entries) {
      return {
        name: path.basename(dirPath),
        path: dirPath,
        isDirectory: true,
        error: result.error || 'Failed to read directory',
      };
    }

    const children: TreeNode[] = result.entries
      .filter((e) => !e.name.startsWith('.'))
      .map((entry) => ({
        name: entry.name,
        path: entry.path,
        isDirectory: entry.isDirectory,
        children: entry.isDirectory ? [] : undefined,
      }));

    return {
      name: path.basename(dirPath) || dirPath,
      path: dirPath,
      isDirectory: true,
      children,
      expanded: true,
    };
  }, []);

  const handleOpenDirectory = useCallback(async () => {
    setLoading(true);
    setError('');
    try {
      const result: OpenDirectoryResult = await window.dialog.openDirectory();
      if (result.ok && result.path) {
        setRootPath(result.path);
        const loadedTree = await loadDirectory(result.path);
        setTree(loadedTree);
        setSelectedFile('');
        setFileContent('');
      } else if (result.error !== 'Canceled') {
        setError(result.error || 'Failed to open directory');
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Unknown error');
    } finally {
      setLoading(false);
    }
  }, [loadDirectory]);

  const toggleNode = useCallback((nodePath: string) => {
    setTree((prevTree) => {
      if (!prevTree) return null;
      return toggleNodeRecursive(prevTree, nodePath);
    });
  }, []);

  const toggleNodeRecursive = (node: TreeNode, targetPath: string): TreeNode => {
    if (node.path === targetPath) {
      if (!node.isDirectory) return node;
      const newExpanded = !node.expanded;
      const newNode: TreeNode = { ...node, expanded: newExpanded };
      if (newExpanded && node.children && node.children.length === 0 && !node.loading) {
        newNode.loading = true;
        loadDirectory(targetPath).then((loaded) => {
          setTree((prev) => updateNodeRecursive(prev, targetPath, loaded));
        });
      }
      return newNode;
    }
    if (node.children) {
      return { ...node, children: node.children.map((child) => toggleNodeRecursive(child, targetPath)) };
    }
    return node;
  };

  const updateNodeRecursive = (node: TreeNode | null, targetPath: string, newNode: TreeNode): TreeNode | null => {
    if (!node) return null;
    if (node.path === targetPath) {
      return { ...newNode, expanded: true, loading: false };
    }
    if (node.children) {
      return { ...node, children: node.children.map((child) => updateNodeRecursive(child, targetPath, newNode)) };
    }
    return node;
  };

  const handleNodeClick = useCallback((node: TreeNode) => {
    if (node.isDirectory) {
      toggleNode(node.path);
    } else {
      setSelectedFile(node.path);
      loadFile(node.path);
    }
  }, [toggleNode]);

  const loadFile = useCallback(async (filePath: string) => {
    const result: ReadFileResult = await window.fs.readFile(filePath);
    if (result.ok) {
      setFileContent(result.content || '');
    } else {
      setFileContent(`Error: ${result.error}`);
    }
  }, []);

  const renderTree = useMemo(() => {
    if (!tree) return null;
    return renderNode(tree, 0);
  }, [tree, selectedFile]);

  const renderNode = (node: TreeNode, depth: number): JSX.Element => {
    const indent = depth * 16;
    const hasChildren = node.isDirectory && node.children !== undefined;
    const isExpanded = node.expanded === true;
    const isSelected = node.path === selectedFile;

    return (
      <div key={node.path} style={{ display: 'flex', flexDirection: 'column' }}>
        <div
          style={{
            paddingLeft: `${indent}px`,
            display: 'flex',
            alignItems: 'center',
            height: '24px',
            backgroundColor: isSelected ? '#0078d4' : 'transparent',
            color: isSelected ? 'white' : 'inherit',
            cursor: 'pointer',
            userSelect: 'none',
          }}
          onClick={() => handleNodeClick(node)}
          title={node.path}
        >
          {hasChildren && (
            <span style={{ width: '16px', textAlign: 'center' }}>
              {isExpanded ? '▼' : '▶'}
            </span>
          )}
          {!hasChildren && <span style={{ width: '16px' }} />}
          <span style={{ flex: 1, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
            {node.name}
          </span>
          {node.loading && <span style={{ marginRight: '8px' }}>⟳</span>}
          {node.error && <span style={{ color: 'red', marginLeft: '8px' }}>{node.error}</span>}
        </div>
        {hasChildren && isExpanded && node.children && (
          <div>{node.children.map((child) => renderNode(child, depth + 1))}</div>
        )}
      </div>
    );
  };

  return (
    <main id="window-root">
      <header id="window-header">
        <h1 id="window-title">File Explorer</h1>
        <button id="window-open-btn" onClick={handleOpenDirectory} disabled={loading} aria-label="Open directory">
          {loading ? 'Opening...' : 'Open Directory'}
        </button>
      </header>
      {error && <div id="window-error" role="alert">{error}</div>}
      <div id="window-content">
        <aside id="window-sidebar">
          <nav id="window-tree" aria-label="Directory tree">
            {renderTree}
            {!tree && !loading && (
              <div id="window-empty" style={{ padding: '16px', color: '#666', textAlign: 'center' }}>
                Click "Open Directory" to browse the filesystem
              </div>
            )}
          </nav>
        </aside>
        <section id="window-main">
          <header id="window-file-header">
            {selectedFile ? (
              <span id="window-file-path" title={selectedFile}>{selectedFile}</span>
            ) : (
              <span id="window-no-file">No file selected</span>
            )}
          </header>
          <pre id="window-file-content" aria-label="File content">{fileContent}</pre>
        </section>
      </div>
    </main>
  );
}

// Helper to get basename (since we can't use node:path in renderer)
function pathBasename(p: string): string {
  return p.split(/[/\\]/).filter(Boolean).pop() || p;
}

const path = { basename: pathBasename };

export default App;