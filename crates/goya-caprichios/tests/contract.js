let active = 0, maximum = 0, calls = 0;
const order = [];
globalThis.fetch = async (url, options) => {
  calls++;
  if (options.method !== 'GET' || options.redirect !== 'error') throw Error('request options');
  active++; maximum = Math.max(active, maximum); order.push(url.pathname);
  try {
    if (url.pathname === '/fail') throw Error('network failure');
    if (url.pathname === '/timeout') await new Promise((resolve, reject) => options.signal.addEventListener('abort', () => reject(Error('aborted')), {once:true}));
    else await new Promise(resolve => setTimeout(resolve, 5));
    return { status: 404, arrayBuffer: async () => {
      if (url.pathname === '/body-timeout') await new Promise((resolve, reject) => options.signal.addEventListener('abort', () => reject(Error('aborted')), {once:true}));
      return new Uint8Array([1,2,3]).buffer;
    }};
  } finally { active--; }
};
try { await get('http://example.test'); throw Error('accepted HTTP'); } catch (e) { if (!(e instanceof TypeError)) throw e; }
if (calls !== 0) throw Error('HTTP reached fetch');
const results = await Promise.all([get('https://example.test/a'), get('https://example.test/b')]);
if (maximum !== 1 || order.join() !== '/a,/b' || results[0].status !== 404 || results[0].body.join() !== '1,2,3') throw Error('result or concurrency');
for (const path of ['/fail', '/timeout', '/body-timeout']) {
  let failed = false; try { await get('https://example.test' + path); } catch (_) { failed = true; }
  if (!failed) throw Error('expected failure ' + path);
}
await get('https://example.test/after');

// A concurrency slot remains occupied while the response body is read.
let releaseBody, bodyStarted, bodyCalls = 0;
const started = new Promise(resolve => { bodyStarted = resolve; });
const bodyGate = new Promise(resolve => { releaseBody = resolve; });
globalThis.fetch = async () => {
  bodyCalls++;
  return { status: 200, arrayBuffer: async () => {
    if (bodyCalls === 1) { bodyStarted(); await bodyGate; }
    return new ArrayBuffer(0);
  }};
};
const first = get('https://example.test/body');
await started;
const second = get('https://example.test/waiting');
await new Promise(resolve => setTimeout(resolve, 5));
if (bodyCalls !== 1) throw Error('concurrency slot released before body completed');
releaseBody();
await Promise.all([first, second]);
if (bodyCalls !== 2) throw Error('waiting request did not run');

// Waiting for admission has no timeout. A failed admitted request hands its
// slot to the queue, where the next request receives a fresh timeout budget.
const realSetTimeout = globalThis.setTimeout;
const realClearTimeout = globalThis.clearTimeout;
const timers = new Map();
let nextTimer = 0, rejectFirst, firstStarted, secondStarted;
const firstAdmission = new Promise(resolve => { firstStarted = resolve; });
const secondAdmission = new Promise(resolve => { secondStarted = resolve; });
let admissionCalls = 0;
globalThis.setTimeout = callback => {
  const id = ++nextTimer;
  timers.set(id, callback);
  return id;
};
globalThis.clearTimeout = id => { timers.delete(id); };
try {
  globalThis.fetch = async (_url, options) => {
    admissionCalls++;
    if (options.signal.aborted) throw Error('request admitted with expired budget');
    if (admissionCalls === 1) {
      firstStarted();
      return await new Promise((_resolve, reject) => { rejectFirst = reject; });
    }
    secondStarted();
    return { status: 200, arrayBuffer: async () => new ArrayBuffer(0) };
  };
  const admitted = get('https://example.test/admitted').then(
    () => { throw Error('expected admitted request to fail'); },
    error => { if (error.message !== 'controlled failure') throw error; }
  );
  await firstAdmission;
  const queued = get('https://example.test/queued');
  await Promise.resolve();
  if (nextTimer !== 1 || timers.size !== 1 || admissionCalls !== 1)
    throw Error('queued request started its timeout before admission');
  rejectFirst(Error('controlled failure'));
  await secondAdmission;
  await Promise.all([admitted, queued]);
  if (nextTimer !== 2 || timers.size !== 0 || admissionCalls !== 2)
    throw Error('timeout cleanup or failed-request queue handoff');
} finally {
  globalThis.setTimeout = realSetTimeout;
  globalThis.clearTimeout = realClearTimeout;
}
