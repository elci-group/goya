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
