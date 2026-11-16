const load = async source => {
  const url = URL.createObjectURL(new Blob([source], { type: 'text/javascript' }));
  try { return await import(url); } finally { URL.revokeObjectURL(url); }
};

const adapter = await load(sources[0]);
const warmupIterations = 10;
const iterations = 200;
const responseBytes = 256;
const expected = Uint8Array.from({ length: responseBytes }, (_, index) => index & 0xff);
const endpoint = location.origin + '/benchmark';
const latencies = [];
const started = performance.now();

for (let index = 0; index < warmupIterations; index++) {
  const response = await adapter.get(endpoint);
  if (response.status !== 200 || response.body.length !== responseBytes ||
      !response.body.every((byte, offset) => byte === expected[offset])) {
    throw Error('warmup response mismatch');
  }
}

for (let index = 0; index < iterations; index++) {
  const mark = performance.now();
  const response = await adapter.get(endpoint);
  latencies.push(performance.now() - mark);
  if (response.status !== 200 || response.body.length !== responseBytes ||
      !response.body.every((byte, offset) => byte === expected[offset])) {
    throw Error('measured response mismatch');
  }
}
const totalMs = performance.now() - started;
const sorted = [...latencies].sort((left, right) => left - right);
const percentile = fraction => sorted[Math.ceil(fraction * sorted.length) - 1];
const mean = latencies.reduce((sum, value) => sum + value, 0) / latencies.length;

document.body.textContent = 'BENCHMARK:' + JSON.stringify({
  browser: navigator.userAgent,
  origin: location.origin,
  endpoint,
  transport: 'local self-signed HTTPS; certificate errors bypassed in isolated benchmark Chrome only',
  recipe: 'browser-fetch@0.0.1',
  timeout_ms: 5000,
  concurrency_limit: 4,
  warmup_iterations: warmupIterations,
  measured_iterations: iterations,
  fixed_response_bytes: responseBytes,
  response_status_and_bytes_verified: true,
  timings: 'generated get() call from invocation through complete response body read',
  median_ms: percentile(0.50),
  p95_ms: percentile(0.95),
  mean_ms: mean,
  total_elapsed_ms: totalMs,
});
