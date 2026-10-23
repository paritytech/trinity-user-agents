import { describe, expect, it } from 'bun:test';
import {
  encodeWireMessage,
  MESSAGE_TYPE_RECEIVE,
  MESSAGE_TYPE_START,
  type WebSocketWireProvider,
} from '@parity/truapi';
import { CHAIN_FOLLOW_HEAD_SUBSCRIBE, RENDERER_ACTION_SUBSCRIBE } from '@parity/truapi/wire-table';

import { logActionStreams } from './action-stream-log.js';

function frame(requestId: string, ids: { trait: number; method: number }, messageType: number) {
  return encodeWireMessage({
    requestId,
    payload: { traitId: ids.trait, methodId: ids.method, messageType, value: new Uint8Array() },
  })._unsafeUnwrap();
}

function fakeProvider() {
  let receive: (frame: Uint8Array) => void = () => {};
  let close: (error: Error) => void = () => {};
  const provider: WebSocketWireProvider = {
    opened: Promise.resolve(),
    postMessage() {},
    subscribe(callback) {
      receive = callback;
      return () => {};
    },
    subscribeClose(callback) {
      close = callback;
      return () => {};
    },
    dispose() {},
  };
  return { provider, receive: (frame: Uint8Array) => receive(frame), close: (error: Error) => close(error) };
}

describe('logActionStreams', () => {
  // A connection reset ends the product's action stream without opening a new
  // one. Only a second start on the new connection shows the product reopened
  // it; without one, every later press is held by the host for a stream that
  // never comes back.
  it('shows whether the product reopens its action stream after the connection is lost', () => {
    const lines: string[] = [];
    const first = fakeProvider();
    const providers = [first.provider, fakeProvider().provider];
    const connect = logActionStreams(() => providers.shift()!, (line) => lines.push(line));

    const firstSocket = connect('ws://host');
    firstSocket.subscribe(() => {});
    firstSocket.subscribeClose?.(() => {});
    firstSocket.postMessage(frame('host:1', RENDERER_ACTION_SUBSCRIBE, MESSAGE_TYPE_START));
    firstSocket.postMessage(frame('host:2', CHAIN_FOLLOW_HEAD_SUBSCRIBE, MESSAGE_TYPE_START));
    first.receive(frame('host:1', RENDERER_ACTION_SUBSCRIBE, MESSAGE_TYPE_RECEIVE));
    first.close(new Error('Socket is not connected'));

    const secondSocket = connect('ws://host');
    secondSocket.postMessage(frame('host:3', RENDERER_ACTION_SUBSCRIBE, MESSAGE_TYPE_START));

    expect(lines).toEqual([
      '[truapi] renderer action stream host:1 started',
      '[truapi] renderer action stream host:1 delivered an action',
      '[truapi] host connection lost (Socket is not connected); ended action streams: renderer host:1',
      '[truapi] renderer action stream host:3 started',
    ]);
  });
});
