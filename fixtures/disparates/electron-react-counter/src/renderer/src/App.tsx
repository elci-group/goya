import { useState, useEffect, useCallback } from 'react';

interface CounterResult {
  ok: boolean;
  count: string;
  error?: string;
}

function App(): JSX.Element {
  const [count, setCount] = useState('0');
  const [amount, setAmount] = useState('1');
  const [pending, setPending] = useState<Promise<void>>(Promise.resolve());

  // Initial load
  useEffect(() => {
    window.counter.get().then((result: CounterResult) => {
      if (result.ok) {
        setCount(result.count);
      }
    });
  }, []);

  const handleAdd = useCallback(() => {
    setPending((prev) =>
      prev.then(async () => {
        const result: CounterResult = await window.counter.add(amount);
        if (result.ok) {
          setCount(result.count);
        }
      })
    );
  }, [amount]);

  // Paste prevention on the amount input
  const handlePaste = useCallback((event: React.ClipboardEvent<HTMLInputElement>) => {
    event.preventDefault();
  }, []);

  return (
    <main id="window-root">
      <span className="caption">Count</span>
      <input
        id="window-amount"
        type="text"
        value={amount}
        onChange={(e) => setAmount(e.target.value)}
        onPaste={handlePaste}
        aria-label="Amount"
        aria-description="A whole number to add to the count"
      />
      <button id="window-add" type="button" onClick={handleAdd} aria-label="Add amount">
        Add
      </button>
      <output id="window-total" aria-live="polite">
        {count}
      </output>
    </main>
  );
}

export default App;