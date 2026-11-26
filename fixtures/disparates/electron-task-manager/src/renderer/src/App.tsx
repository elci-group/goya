import { useState, useEffect, useCallback } from 'react';

interface Task {
  id: string;
  title: string;
  completed: boolean;
  createdAt: number;
}

declare global {
  interface Window {
    tasks: {
      list: () => Promise<Task[]>;
      add: (title: string) => Promise<Task>;
      toggle: (id: string) => Promise<Task | null>;
      delete: (id: string) => Promise<boolean>;
    };
  }
}

function App() {
  const [tasks, setTasks] = useState<Task[]>([]);
  const [newTaskTitle, setNewTaskTitle] = useState('');
  const [loading, setLoading] = useState(true);

  const loadTasks = useCallback(async () => {
    try {
      const data = await window.tasks.list();
      setTasks(data);
    } catch (error) {
      console.error('Failed to load tasks:', error);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadTasks();
  }, [loadTasks]);

  const handleAdd = useCallback(async (e: React.FormEvent) => {
    e.preventDefault();
    const title = newTaskTitle.trim();
    if (!title) return;

    try {
      const task = await window.tasks.add(title);
      setTasks((prev) => [task, ...prev]);
      setNewTaskTitle('');
    } catch (error) {
      console.error('Failed to add task:', error);
    }
  }, [newTaskTitle]);

  const handleToggle = useCallback(async (id: string) => {
    try {
      const updated = await window.tasks.toggle(id);
      if (updated) {
        setTasks((prev) => prev.map((t) => (t.id === id ? updated : t)));
      }
    } catch (error) {
      console.error('Failed to toggle task:', error);
    }
  }, []);

  const handleDelete = useCallback(async (id: string) => {
    try {
      const ok = await window.tasks.delete(id);
      if (ok) {
        setTasks((prev) => prev.filter((t) => t.id !== id));
      }
    } catch (error) {
      console.error('Failed to delete task:', error);
    }
  }, []);

  if (loading) {
    return <div className="app">Loading...</div>;
  }

  return (
    <div className="app">
      <header>
        <h1>Task Manager</h1>
      </header>
      <main>
        <form onSubmit={handleAdd} className="task-form">
          <input
            type="text"
            value={newTaskTitle}
            onChange={(e) => setNewTaskTitle(e.target.value)}
            placeholder="Add a new task..."
            aria-label="New task title"
          />
          <button type="submit" disabled={!newTaskTitle.trim()}>
            Add
          </button>
        </form>

        <ul className="task-list">
          {tasks.map((task) => (
            <li key={task.id} className="task-item">
              <label className="task-label">
                <input
                  type="checkbox"
                  checked={task.completed}
                  onChange={() => handleToggle(task.id)}
                  aria-label={task.title}
                />
                <span className={task.completed ? 'completed' : ''}>
                  {task.title}
                </span>
              </label>
              <button
                className="delete-btn"
                onClick={() => handleDelete(task.id)}
                aria-label={`Delete ${task.title}`}
              >
                Delete
              </button>
            </li>
          ))}
          {tasks.length === 0 && (
            <li className="empty-state">No tasks yet. Add one above!</li>
          )}
        </ul>
      </main>
    </div>
  );
}

export default App;