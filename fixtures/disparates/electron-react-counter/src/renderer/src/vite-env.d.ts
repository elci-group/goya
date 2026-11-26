/// <reference types="vite/client" />

interface Window {
  counter: {
    add: (amountText: string) => Promise<{ ok: boolean; count: string; error?: string }>;
    get: () => Promise<{ ok: boolean; count: string; error?: string }>;
  };
}