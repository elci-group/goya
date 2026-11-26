/// <reference types="vite/client" />

interface Task {
  id: string;
  title: string;
  completed: boolean;
  createdAt: number;
}

interface TasksAPI {
  list: () => Promise<Task[]>;
  add: (title: string) => Promise<Task>;
  toggle: (id: string) => Promise<Task | null>;
  delete: (id: string) => Promise<boolean>;
}

interface Window {
  tasks: TasksAPI;
}