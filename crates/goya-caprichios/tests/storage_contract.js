const check = (condition, message) => { if (!condition) throw Error(message); };
const load = async source => {
  const url = URL.createObjectURL(new Blob([source], { type: 'text/javascript' }));
  try { return await import(url); } finally { URL.revokeObjectURL(url); }
};
const a = await load(sources[0]);
const b = await load(sources[1]);
const bytesEqual = (actual, expected) => actual instanceof Uint8Array &&
  actual.length === expected.length && actual.every((byte, index) => byte === expected[index]);
check(await a.get('missing') === undefined, 'missing key must return undefined');
await a.deleteKey('missing');

// Observe real IndexedDB transaction completion; request success alone does
// not establish that a write committed.
const originalTransaction = IDBDatabase.prototype.transaction;
let writes = 0, commits = 0, abortedTransactions = 0;
let abortNextWrite = false, abortRequestSuccesses = 0;
IDBDatabase.prototype.transaction = function (...args) {
  const transaction = originalTransaction.apply(this, args);
  if (args[1] === 'readwrite') {
    writes++;
    transaction.addEventListener('complete', () => { commits++; });
    if (abortNextWrite) {
      abortNextWrite = false;
      transaction.addEventListener('abort', () => { abortedTransactions++; });
      const originalObjectStore = transaction.objectStore.bind(transaction);
      transaction.objectStore = name => {
        const store = originalObjectStore(name);
        return new Proxy(store, {
          get(target, property) {
            const member = Reflect.get(target, property, target);
            if (property === 'put') {
              return (...putArgs) => {
                const request = Reflect.apply(member, target, putArgs);
                request.addEventListener('success', () => {
                  abortRequestSuccesses++;
                  transaction.abort();
                }, { once: true });
                return request;
              };
            }
            return typeof member === 'function' ? member.bind(target) : member;
          },
        });
      };
    }
  }
  return transaction;
};
try {
  const input = new Uint8Array([0, 127, 128, 255]);
  const putting = a.put('shared', input);
  input[0] = 99;
  await putting;
  check(writes === 1 && commits === 1, 'put resolved before transaction completed');
  check(bytesEqual(await a.get('shared'), [0, 127, 128, 255]), 'stored bytes or input copy');
  const output = await a.get('shared');
  output[0] = 44;
  check(bytesEqual(await a.get('shared'), [0, 127, 128, 255]), 'get leaked mutable stored bytes');
  await a.put('empty', new Uint8Array(0));
  check(bytesEqual(await a.get('empty'), []), 'empty value differs from missing key');
  let tooLarge = false;
  try { await a.put('shared', new Uint8Array(5)); }
  catch (error) { tooLarge = error instanceof RangeError; }
  check(tooLarge, 'value above max size must reject with RangeError');
  check(bytesEqual(await a.get('shared'), [0, 127, 128, 255]), 'oversize put changed stored value');

  await a.put('abort-check', new Uint8Array([7, 8]));
  abortNextWrite = true;
  let abortedPutRejected = false;
  try { await a.put('abort-check', new Uint8Array([1, 2, 3])); }
  catch (_) { abortedPutRejected = true; }
  check(abortRequestSuccesses === 1, 'write request did not succeed before forced transaction abort');
  check(abortedTransactions === 1, 'forced write transaction did not abort');
  check(abortedPutRejected, 'put resolved after its transaction was aborted');
  check(bytesEqual(await a.get('abort-check'), [7, 8]), 'aborted write changed the prior committed value');
  await a.put('abort-check', new Uint8Array([4, 5]));
  check(bytesEqual(await a.get('abort-check'), [4, 5]), 'write did not recover after an aborted transaction');

  for (const operation of [() => a.get(1), () => a.put('bad', [1]), () => a.deleteKey(null)]) {
    let typeError = false;
    try { await operation(); } catch (error) { typeError = error instanceof TypeError; }
    check(typeError, 'invalid argument must reject with TypeError');
  }
  check(await b.get('shared') === undefined, 'namespaces leaked data');
  await b.put('shared', new Uint8Array([9]));
  const reopened = await load(sources[0]);
  check(bytesEqual(await reopened.get('shared'), [0, 127, 128, 255]), 'fresh module lost committed data');
  await reopened.put('shared', new Uint8Array([3, 2, 1]));
  check(bytesEqual(await a.get('shared'), [3, 2, 1]), 'overwrite not visible to another module');
  await a.deleteKey('shared');
  check(writes === commits + abortedTransactions, 'write settled without commit or observed abort');
  check(await reopened.get('shared') === undefined, 'delete not visible to another module');
  check(bytesEqual(await b.get('shared'), [9]), 'delete affected another namespace');
} finally {
  IDBDatabase.prototype.transaction = originalTransaction;
}
