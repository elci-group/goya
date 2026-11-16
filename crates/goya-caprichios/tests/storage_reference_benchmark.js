const DATABASE_NAME = 'goya.storage.kv.v1:reference.benchmark';
const STORE_NAME = 'values';
const KEY = 'benchmark-value';
const VALUE_SIZE = 4096;
const WARMUPS_PER_PATH = 50;
const SAMPLES_PER_PATH = 500;

const fail = message => { throw new Error(message); };
const openDatabase = () => new Promise((resolve, reject) => {
  const request = indexedDB.open(DATABASE_NAME, 1);
  request.onupgradeneeded = () => {
    if (!request.result.objectStoreNames.contains(STORE_NAME)) {
      request.result.createObjectStore(STORE_NAME);
    }
  };
  request.onerror = () => reject(request.error || new Error('database open failed'));
  request.onblocked = () => reject(new Error('database open was blocked'));
  request.onsuccess = () => {
    const db = request.result;
    db.onversionchange = () => db.close();
    resolve(db);
  };
});

// Reference implementation mirrors the adapter's per-call lifecycle: open the
// same DB, issue one readonly get transaction, wait for transaction completion,
// copy the Uint8Array result, and close the DB.
const nativeGet = async key => {
  const db = await openDatabase();
  return new Promise((resolve, reject) => {
    let result;
    let failure;
    let transaction;
    try {
      transaction = db.transaction(STORE_NAME, 'readonly');
      transaction.oncomplete = () => {
        db.close();
        if (result === undefined) return resolve(undefined);
        if (!(result instanceof Uint8Array)) return reject(new TypeError('native value was not Uint8Array'));
        resolve(new Uint8Array(result));
      };
      transaction.onabort = () => {
        db.close();
        reject(failure || transaction.error || new Error('native read transaction aborted'));
      };
      const request = transaction.objectStore(STORE_NAME).get(key);
      request.onsuccess = () => { result = request.result; };
      request.onerror = () => { failure = request.error; };
    } catch (error) {
      if (transaction) {
        try { transaction.abort(); } catch (_) {}
      }
      db.close();
      reject(error);
    }
  });
};

const sameBytes = (actual, expected) => actual instanceof Uint8Array &&
  actual.length === expected.length &&
  actual.every((byte, index) => byte === expected[index]);
const percentile = (values, p) => {
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.max(0, Math.ceil(p * sorted.length) - 1)];
};
const summarize = values => ({
  median_ms: percentile(values, 0.50),
  p95_ms: percentile(values, 0.95),
});
const timed = async (operation, expected) => {
  const start = performance.now();
  const value = await operation();
  const elapsed = performance.now() - start;
  if (!sameBytes(value, expected)) fail('read returned bytes different from seeded value');
  return elapsed;
};

const runReferenceBenchmark = async generatedSource => {
  const moduleUrl = URL.createObjectURL(new Blob([generatedSource], { type: 'text/javascript' }));
  let adapter;
  try { adapter = await import(moduleUrl); }
  finally { URL.revokeObjectURL(moduleUrl); }

  const expected = Uint8Array.from({ length: VALUE_SIZE }, (_, index) => (index * 37 + 11) & 255);
  await adapter.put(KEY, expected);
  const generatedRead = () => adapter.get(KEY);
  const nativeRead = () => nativeGet(KEY);

  for (let i = 0; i < WARMUPS_PER_PATH; i++) {
    if (i % 2 === 0) {
      await timed(generatedRead, expected);
      await timed(nativeRead, expected);
    } else {
      await timed(nativeRead, expected);
      await timed(generatedRead, expected);
    }
  }

  const generatedSamples = [];
  const nativeSamples = [];
  for (let i = 0; i < SAMPLES_PER_PATH; i++) {
    if (i % 2 === 0) {
      generatedSamples.push(await timed(generatedRead, expected));
      nativeSamples.push(await timed(nativeRead, expected));
    } else {
      nativeSamples.push(await timed(nativeRead, expected));
      generatedSamples.push(await timed(generatedRead, expected));
    }
  }

  await adapter.deleteKey(KEY);
  return {
    methodology: {
      browser_api: 'Chrome IndexedDB on loopback origin with a fresh temporary user profile',
      generated_path: 'generated adapter get(key)',
      reference_path: 'native indexedDB.open + one readonly get transaction + wait for transaction completion + copy Uint8Array + close DB',
      lifecycle_match: 'both paths open and close the same database once per read and wait for readonly transaction completion',
      database_name: DATABASE_NAME,
      value_bytes: VALUE_SIZE,
      warmups_per_path: WARMUPS_PER_PATH,
      samples_per_path: SAMPLES_PER_PATH,
      ordering: 'paired calls alternate which path runs first',
      timing: 'performance.now around each awaited read; byte verification excluded',
      statistics: 'median and nearest-rank p95 in milliseconds',
      thresholds: 'none; results are descriptive measurements only',
    },
    generated_adapter: summarize(generatedSamples),
    native_indexeddb_reference: summarize(nativeSamples),
  };
};

try {
  const report = await runReferenceBenchmark(sources[0]);
  document.body.textContent = 'RESULT:' + JSON.stringify(report);
} catch (error) {
  document.body.textContent = 'FAIL:' + (error && error.stack ? error.stack : String(error));
}
