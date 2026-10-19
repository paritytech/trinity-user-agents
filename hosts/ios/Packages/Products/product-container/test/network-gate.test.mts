import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createNetworkGate } from '../src/network-gate.ts';

(globalThis as any).location = { href: 'https://product.example/' };

function makeGate(answer: (url: string) => Promise<{ allowed: boolean }>, ttlMs = 1000) {
  const calls: string[] = [];
  let clock = 0;
  const gate = createNetworkGate(
    (url) => {
      calls.push(url);
      return answer(url);
    },
    { ttlMs, timeoutMs: 50, now: () => clock },
  );
  return { gate, calls, tick: (ms: number) => (clock += ms) };
}

test('an allowed host is not asked again within the ttl', async () => {
  const { gate, calls } = makeGate(async () => ({ allowed: true }));

  assert.equal(await gate.allow('https://api.example/a'), true);
  assert.equal(await gate.allow('https://api.example/b'), true);

  assert.deepEqual(calls, ['https://api.example/a']);
});

test('the grant expires after the ttl', async () => {
  const { gate, calls, tick } = makeGate(async () => ({ allowed: true }), 1000);

  await gate.allow('https://api.example/a');
  tick(1001);
  await gate.allow('https://api.example/b');

  assert.equal(calls.length, 2);
});

test('a refused host is asked again next time', async () => {
  const { gate, calls } = makeGate(async () => ({ allowed: false }));

  assert.equal(await gate.allow('https://api.example/a'), false);
  assert.equal(await gate.allow('https://api.example/b'), false);

  assert.equal(calls.length, 2);
});

test('parallel requests to one host share a single native call', async () => {
  let release: (value: { allowed: boolean }) => void = () => {};
  const { gate, calls } = makeGate(() => new Promise((resolve) => (release = resolve)));

  const first = gate.allow('https://api.example/a');
  const second = gate.allow('https://api.example/b');
  release({ allowed: true });

  assert.deepEqual(await Promise.all([first, second]), [true, true]);
  assert.equal(calls.length, 1);
});

test('different hosts are asked separately', async () => {
  const { gate, calls } = makeGate(async () => ({ allowed: true }));

  await gate.allow('https://one.example/');
  await gate.allow('https://two.example/');

  assert.equal(calls.length, 2);
});

test('a relative url resolves against the page and is cached by host', async () => {
  const { gate, calls } = makeGate(async () => ({ allowed: true }));

  await gate.allow('/assets/a.json');
  await gate.allow('https://product.example/assets/b.json');

  assert.equal(calls.length, 1);
});

test('a failed native call is not cached and surfaces to the caller', async () => {
  let fail = true;
  const { gate, calls } = makeGate(async () => {
    if (fail) throw new Error('timed out');
    return { allowed: true };
  });

  await assert.rejects(gate.allow('https://api.example/a'), /timed out/);
  fail = false;
  assert.equal(await gate.allow('https://api.example/b'), true);

  assert.equal(calls.length, 2);
});
