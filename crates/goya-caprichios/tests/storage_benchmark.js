const load = async source => {
  const url = URL.createObjectURL(new Blob([source], { type: 'text/javascript' }));
  try { return await import(url); } finally { URL.revokeObjectURL(url); }
};

const adapter = await load(sources[0]);
const iterations = 200;
const warmupIterations = 10;
const payload = Uint8Array.from({ length: 256 }, (_, index) => index & 0xff);
const sameBytes = (left, right) => left instanceof Uint8Array &&
  left.length === right.length && left.every((byte, index) => byte === right[index]);
const putMs = [];
const getMs = [];
const key = 'benchmark.fixed-payload';

for (let index = 0; index < warmupIterations; index++) {
  await adapter.put(key, payload);
  await adapter.get(key);
}

const started = performance.now();
for (let index = 0; index < iterations; index++) {
  let mark = performance.now();
  await adapter.put(key, payload);
  putMs.push(performance.now() - mark);

  mark = performance.now();
  const received = await adapter.get(key);
  getMs.push(performance.now() - mark);
  if (!sameBytes(received, payload)) throw Error('benchmark read did not match the fixed payload');
}
const totalMs = performance.now() - started;

const percentile = (values, fraction) => {
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.ceil(fraction * sorted.length) - 1];
};
const summary = values => ({
  median_ms: percentile(values, 0.50),
  p95_ms: percentile(values, 0.95),
  mean_ms: values.reduce((sum, value) => sum + value, 0) / values.length,
});

document.body.textContent = 'BENCHMARK:' + JSON.stringify({
  browser: navigator.userAgent,
  origin: location.origin,
  indexed_db_available: Boolean(indexedDB),
  recipe: 'browser-indexeddb@0.0.1',
  namespace: 'benchmark.smoke',
  max_value_bytes: 1024,
  payload_bytes: payload.byteLength,
  warmup_iterations: warmupIterations,
  measured_iterations: iterations,
  timings: 'adapter promises resolve after IndexedDB transaction completion',
  put: summary(putMs),
  get: summary(getMs),
  total_elapsed_ms: totalMs,
});
