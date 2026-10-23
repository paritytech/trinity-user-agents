import { test, mock } from 'node:test';
import assert from 'node:assert/strict';
import { createNativeTransport } from '../src/native-transport.ts';

(globalThis as any).window = globalThis;

function makeTransport() {
  const sent: any[] = [];
  const transport = createNativeTransport((message) => sent.push(message));
  const reply = (id: string, payload: object) => (globalThis as any).__container_callback__(id, payload);
  return { transport, sent, reply };
}

test('a call without a timeout waits for native', async () => {
  const { transport, sent, reply } = makeTransport();

  const call = transport.callNative('ping', {});
  reply(sent[0].id, { value: 'pong' });

  assert.equal(await call, 'pong');
});

test('a call with a timeout rejects with code timeout when native stays silent', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const { transport } = makeTransport();

  const call = transport.callNative('allowNetworkAccess', { url: 'https://x' }, { timeoutMs: 100 });
  t.mock.timers.tick(100);

  await assert.rejects(call, (e: any) => e.code === 'timeout');
});

test('a late native answer after the timeout is ignored', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const { transport, sent, reply } = makeTransport();

  const call = transport.callNative('allowNetworkAccess', { url: 'https://x' }, { timeoutMs: 100 });
  t.mock.timers.tick(100);
  await assert.rejects(call);

  assert.doesNotThrow(() => reply(sent[0].id, { value: { allowed: true } }));
});

test('an answer in time clears the timer', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const { transport, sent, reply } = makeTransport();

  const call = transport.callNative('allowNetworkAccess', { url: 'https://x' }, { timeoutMs: 100 });
  reply(sent[0].id, { value: { allowed: true } });
  t.mock.timers.tick(100);

  assert.deepEqual(await call, { allowed: true });
});
