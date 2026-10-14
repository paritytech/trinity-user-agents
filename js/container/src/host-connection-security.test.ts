import { afterEach, describe, expect, it } from 'bun:test';
import { createContext, runInContext } from 'node:vm';
import { browserGlobals, browserScript, frameBytes } from './test-browser.js';
import { decodeWireMessage, encodeWireMessage, type ProtocolMessage } from '@parity/truapi';
import type { HostConnection } from '@parity/truapi/internal';
import * as W from '@parity/truapi/wire-table';

const source = await browserScript(`
  import { createHostConnection } from '@parity/truapi/internal';
  import { freezePermissionRuntime } from './permission-runtime.ts';
  import { freezeValue } from './freeze.ts';
  import { installFetchGate } from './network.ts';
  import { createPermissionAuthorization } from './network-transport.ts';
  freezePermissionRuntime();
  const connection = createHostConnection('ws://127.0.0.1:1234/?t=execution');
  freezeValue(window, '__HOST_WEBVIEW_MARK__', true);
  freezeValue(window, '__HOST_API_CLIENT__', Object.freeze({
    get client() { return connection.client; },
    subscribeConnectionStatus: connection.subscribeConnectionStatus,
  }));
  Object.defineProperty(window, '__HOST_API_PORT__', {
    get: () => connection.legacyPort,
    set() {},
    configurable: false,
  });
  window.Promise = Promise;
  window.TypeError = TypeError;
  installFetchGate(window, createPermissionAuthorization(window, connection.internal).network);
  globalThis.fixture = {
    connection,
    authorize: () => connection.internal.permissions.authorizeRemotePermission({
      permission: { tag: 'Remote', value: { domains: ['denied.example'] } },
    }),
  };
`);
const cleanups: (() => void)[] = [];
const settle = () => new Promise<void>(resolve => setTimeout(resolve, 0));

async function until(condition: () => boolean): Promise<void> {
  for (let attempt = 0; attempt < 100 && !condition(); attempt++) await settle();
  expect(condition()).toBe(true);
}

function frame(requestId: string, messageType = 0): Uint8Array {
  return encodeWireMessage({
    requestId,
    payload: { traitId: 0, methodId: 0, messageType, value: new Uint8Array() },
  })._unsafeUnwrap();
}

function decode(bytes: Uint8Array): ProtocolMessage {
  return decodeWireMessage(new Uint8Array(bytes))._unsafeUnwrap();
}

function reply(request: ProtocolMessage, granted: boolean): Uint8Array {
  return encodeWireMessage({
    ...request,
    payload: { ...request.payload, messageType: 1, value: Uint8Array.of(0, 0, Number(granted)) },
  })._unsafeUnwrap();
}

function alternateLength(frame: Uint8Array, width: 2 | 4 | 5): Uint8Array {
  const length = frame[0]! / 4;
  let compact = width === 5 ? length : length * 4 + (width === 2 ? 1 : 2);
  const result = new Uint8Array(frame.length + width - 1);
  if (width === 5) result[0] = 3;
  for (let index = width === 5 ? 1 : 0; index < width; index++) {
    result[index] = compact & 255;
    compact >>>= 8;
  }
  result.set(frame.subarray(1), width);
  return result;
}

function browser() {
  const sockets: BrowserSocket[] = [];
  const dispatch = EventTarget.prototype.dispatchEvent;
  let RealmBytes: typeof Uint8Array;
  const globals = browserGlobals();
  const { EventTarget: BrowserEvents, MessageEvent: BrowserMessage } = globals;
  class BrowserPort extends BrowserEvents {
    peer!: BrowserPort;
    private started = false;
    private closed = false;
    private pending: unknown[] = [];
    postMessage(value: Uint8Array) {
      if (this.closed || this.peer.closed) return;
      this.peer.pending.push(new RealmBytes(value));
      this.peer.drain();
    }
    start() { this.started = true; this.drain(); }
    close() { this.closed = true; this.pending = []; }
    private drain() {
      if (!this.started || this.closed) return;
      for (const data of this.pending.splice(0)) queueMicrotask(() => {
        if (!this.closed) dispatch.call(this, new BrowserMessage('message', { data }));
      });
    }
  }
  class BrowserChannel {
    private readonly first = new BrowserPort();
    private readonly second = new BrowserPort();
    constructor() { this.first.peer = this.second; this.second.peer = this.first; }
    get port1() { return this.first; }
    get port2() { return this.second; }
  }
  class BrowserSocket extends BrowserEvents {
    private binary = 'blob';
    private closed = false;
    readonly sent: ProtocolMessage[] = [];
    get binaryType() { return this.binary; }
    set binaryType(value: string) { this.binary = value; }
    constructor(readonly url: string) {
      super();
      sockets.push(this);
    }
    open() { dispatch.call(this, new Event('open')); }
    send(value: Uint8Array) {
      const bytes = frameBytes(value).slice();
      const request = decode(bytes);
      this.sent.push(request);
      if (request.payload.traitId === W.SYSTEM_HANDSHAKE.trait &&
          request.payload.methodId === W.SYSTEM_HANDSHAKE.method) {
        this.reply(encodeWireMessage({
          ...request,
          payload: { ...request.payload, messageType: 1, value: Uint8Array.of(0, 0) },
        })._unsafeUnwrap());
      }
    }
    reply(bytes: Uint8Array) { this.replyData(bytes.slice().buffer); }
    replyData(data: unknown) { dispatch.call(this, new BrowserMessage('message', { data })); }
    disconnect() {
      this.closed = true;
      dispatch.call(this, new Event('close'));
    }
    close() {
      if (this.closed) return;
      this.closed = true;
      dispatch.call(this, new Event('close'));
    }
  }
  const requests: string[] = [];
  const native = {
    URL, Request, Response, AbortController, AbortSignal, DOMException,
    location: { href: 'https://product.example/', origin: 'https://product.example' },
    fetch: async (request: Request) => { requests.push(request.url); return new Response('received'); },
    ...globals,
    WebSocket: BrowserSocket,
    MessageChannel: BrowserChannel,
    MessagePort: BrowserPort,
    Event,
    setTimeout,
    clearTimeout,
  };
  const win = Object.assign(new BrowserEvents(), native);
  const context = createContext({ ...native, window: win });
  RealmBytes = runInContext('Uint8Array', context) as typeof Uint8Array;
  runInContext(source, context);
  const { connection, authorize } = context.fixture as {
    connection: HostConnection;
    authorize: () => ReturnType<HostConnection['internal']['permissions']['authorizeRemotePermission']>;
  };
  delete context.fixture;
  cleanups.push(() => connection.dispose());
  return {
    context,
    requests,
    fetch: () => (win as unknown as { fetch: typeof fetch }).fetch('https://denied.example/data'),
    connection,
    authorize,
    sockets,
    port: () => (win as unknown as { __HOST_API_PORT__: MessagePort }).__HOST_API_PORT__,
    async connect() {
      const client = connection.client;
      sockets[sockets.length - 1]!.open();
      let connected = false;
      const unsubscribe = connection.subscribeConnectionStatus(status => { connected = status === 'connected'; });
      await until(() => connected);
      unsubscribe();
      return client;
    },
  };
}

function permissionRequests(socket: ReturnType<typeof browser>['sockets'][number]) {
  return socket.sent.filter(request => request.payload.traitId === W.PERMISSIONS_AUTHORIZE_REMOTE_PERMISSION.trait &&
    request.payload.methodId === W.PERMISSIONS_AUTHORIZE_REMOTE_PERMISSION.method);
}

afterEach(() => {
  for (const cleanup of cleanups.splice(0)) cleanup();
});

describe('shared connection permission isolation', () => {
  it.each(['value', 'get'])('keeps permission replies private with poisoned Object.prototype %s properties', async property => {
    const host = browser();
    runInContext(`
      globalThis.exposed = [];
      const intercept = function () { exposed.push(this); this.granted = true; };
      globalThis.poisoned = Object.getOwnPropertyNames(Object.prototype).every(name =>
        Reflect.defineProperty(Object.prototype, name, { ${property}: intercept }));
    `, host.context);
    await host.connect();
    const decision = host.authorize();
    await until(() => permissionRequests(host.sockets[0]!).length === 1);
    host.sockets[0]!.reply(reply(permissionRequests(host.sockets[0]!)[0]!, false));
    expect({ decision: (await decision)._unsafeUnwrap(), poisoned: host.context.poisoned,
      exposed: host.context.exposed }).toEqual({
      decision: { granted: false }, poisoned: true, exposed: [],
    });
  });

  for (const [name, attack] of [
    ['Set iteration', `
      const original = Set.prototype[Symbol.iterator];
      Set.prototype[Symbol.iterator] = function* () {
        for (const listener of Reflect.apply(original, this, [])) {
          yield message => {
            if (message instanceof Uint8Array) message[message.length - 1] = 1;
            listener(message);
          };
        }
      };
    `],
    ['SDK result methods', `
      const result = window.__HOST_API_CLIENT__.client.system.handshake();
      Object.getPrototypeOf(result).then = function (resolve) {
        return Promise.resolve(resolve({ isOk: () => true, value: { granted: true } }));
      };
    `],
  ]) {
    it(`cannot make native fetch run after a denied reply through ${name}`, async () => {
      const host = browser();
      await host.connect();
      runInContext(`try { ${attack} } catch (error) { if (!(error instanceof TypeError)) throw error; }`, host.context);
      const result = host.fetch().then(() => 'allowed', () => 'denied');
      await until(() => host.requests.length > 0 || permissionRequests(host.sockets[0]!).length === 1);
      const request = permissionRequests(host.sockets[0]!)[0];
      if (request) host.sockets[0]!.reply(reply(request, false));
      expect({ decision: await result, requests: host.requests }).toEqual({ decision: 'denied', requests: [] });
    });
  }

  it('uses captured native methods after product replacements and reconnect', async () => {
    const host = browser();
    runInContext(`
      'use strict';
      const intercepted = () => { throw new Error('product replacement'); };
      Function.prototype.call = intercepted;
      Function.prototype.apply = intercepted;
      Function.prototype.bind = intercepted;
      Reflect.apply = intercepted;
      Reflect.construct = intercepted;
      globalThis.Function = intercepted;
      globalThis.Reflect = {};
      globalThis.Symbol = intercepted;
      Object.defineProperty(MessageEvent.prototype, 'data', { get: intercepted });
      globalThis.MessageEvent = intercepted;
    `, host.context);
    for (const granted of [false, true]) {
      await host.connect();
      const result = host.fetch().then(() => 'allowed', () => 'denied');
      const socket = host.sockets[host.sockets.length - 1]!;
      await until(() => permissionRequests(socket).length === 1);
      socket.reply(reply(permissionRequests(socket)[0]!, granted));
      expect({ decision: await result, requests: host.requests }).toEqual({
        decision: granted ? 'allowed' : 'denied',
        requests: granted ? ['https://denied.example/data'] : [],
      });
      if (!granted) {
        socket.disconnect();
        await until(() => host.sockets.length === 2);
      }
    }
  });

  for (const [name, attack] of [
    ['MessageChannel', `
      const NativeChannel = MessageChannel;
      globalThis.MessageChannel = class extends NativeChannel {
        constructor() { super(); exposed.push(this.port2); }
      };
      const { get } = Object.getOwnPropertyDescriptor(NativeChannel.prototype, 'port2');
      Reflect.defineProperty(NativeChannel.prototype, 'port2', {
        get() { const port = Reflect.apply(get, this, []); exposed.push(port); return port; },
      });
    `],
    ['MessagePort', `
      const start = MessagePort.prototype.start;
      MessagePort.prototype.start = function () { exposed.push(this); return Reflect.apply(start, this, []); };
    `],
    ['EventTarget', `
      const add = EventTarget.prototype.addEventListener;
      EventTarget.prototype.addEventListener = function (...args) { exposed.push(this); return Reflect.apply(add, this, args); };
    `],
  ]) {
    it(`keeps the lazy legacy endpoint private after ${name} replacement`, async () => {
      const host = browser();
      runInContext(`globalThis.exposed = []; ${attack}`, host.context);
      const port = host.port();
      await host.connect();
      port.postMessage(frame('p:legacy'));
      await until(() => host.sockets[0]!.sent.some(request => request.requestId === 'p:legacy'));
      const result = host.fetch().then(() => 'allowed', () => 'denied');
      await until(() => permissionRequests(host.sockets[0]!).length === 1);
      host.sockets[0]!.reply(reply(permissionRequests(host.sockets[0]!)[0]!, false));
      expect({ decision: await result, exposed: host.context.exposed, requests: host.requests }).toEqual({
        decision: 'denied', exposed: [], requests: [],
      });
    });
  }

  it('protects later connections while unrelated built-ins remain mutable', async () => {
    const host = browser();
    runInContext(`
      'use strict';
      Date.now = () => 0;
      globalThis.Date = class {};
      Object.defineProperty(ArrayBuffer.prototype, 'byteLength', { get() { throw new Error('buffer intercepted'); } });
      globalThis.ArrayBuffer = class {};
      globalThis.TextEncoder = class {};
      globalThis.TextDecoder = class {};
      Number.prototype.toString = () => 'replaced';
      BigInt.prototype.toString = () => 'replaced';
      BigInt.asUintN = () => 0n;
    `, host.context);
    const port = host.port();
    await host.connect();
    port.postMessage(frame('p:protected'));
    await until(() => host.sockets[0]!.sent.some(request => request.requestId === 'p:protected'));
    const decision = host.authorize();
    await until(() => permissionRequests(host.sockets[0]!).length === 1);
    host.sockets[0]!.reply(reply(permissionRequests(host.sockets[0]!)[0]!, false));
    expect((await decision)._unsafeUnwrap()).toEqual({ granted: false });
  });

  it('recovers without calling a close method installed on the public legacy port', async () => {
    const host = browser();
    const port = host.port();
    const client = await host.connect();
    let intercepted = false;
    Object.defineProperty(port, 'close', { value() {
      intercepted = true;
      throw new Error('product port close');
    } });
    const decision = Promise.resolve(host.authorize()).catch(error => error.name);
    await until(() => permissionRequests(host.sockets[0]!).length === 1);
    host.sockets[0]!.disconnect();
    expect(await decision).toBe('ConnectionResetError');
    await until(() => host.sockets.length === 2);
    await host.connect();
    port.postMessage(frame('p:retired'));
    await settle();
    expect({
      intercepted,
      sameClient: host.connection.client === client,
      forwarded: host.sockets[1]!.sent.some(request => request.requestId === 'p:retired'),
    }).toEqual({ intercepted: false, sameClient: true, forwarded: false });
  });

  it('accepts legacy assignment without replacing the pinned port or client', () => {
    const host = browser();
    const port = host.port();
    const client = host.connection.client;
    runInContext(`
      (() => {
        'use strict';
        window.__HOST_API_PORT__ = { postMessage() { throw new Error('replacement port'); } };
        window.__HOST_API_CLIENT__ = { client: {} };
        window.__HOST_WEBVIEW_MARK__ = false;
      })();
    `, host.context);
    expect({
      samePort: host.port() === port,
      sameClient: runInContext('window.__HOST_API_CLIENT__.client', host.context) === client,
      hosted: runInContext('window.__HOST_WEBVIEW_MARK__', host.context),
      sockets: host.sockets.length,
    }).toEqual({ samePort: true, sameClient: true, hosted: true, sockets: 1 });
  });

  it('keeps authorization private when public methods and shared prototypes are replaced', async () => {
    const host = browser();
    const client = await host.connect();
    runInContext(`
      globalThis.exposed = [];
      const client = window.__HOST_API_CLIENT__.client;
      client.permissions.requestRemotePermission = () => ({ granted: true });
      Object.getPrototypeOf(client.permissions).requestDevicePermission = () => ({ granted: true });
      for (const poison of [
        () => Object.defineProperty(Object.prototype, 'value', { set(value) { exposed.push(value); } }),
        () => { Map.prototype.set = function () { exposed.push(this); }; },
        () => { WeakMap.prototype.get = function () { exposed.push(this); }; },
        () => { Promise.prototype.then = function () { exposed.push(this); }; },
        () => { Object.fromEntries = () => ({ granted: true }); },
        () => { Uint8Array.prototype.set = function () { exposed.push(this); }; },
        () => { EventTarget.prototype.addEventListener = function () { throw new Error('listener intercepted'); }; },
      ]) { try { poison(); } catch {} }
      globalThis.WebSocket = function () { throw new Error('constructor intercepted'); };
      performance.now = () => { exposed.push('clock'); throw new Error('clock intercepted'); };
      globalThis.performance = { now() { exposed.push('clock'); throw new Error('clock replaced'); } };
      window.WebSocket.prototype.send = function () { throw new Error('send intercepted'); };
      window.WebSocket.prototype.close = function () { throw new Error('close intercepted'); };
      for (const name of ['port1', 'port2']) {
        Reflect.defineProperty(MessageChannel.prototype, name, { get() { throw new Error('channel intercepted'); } });
      }
    `, host.context);
    const port = host.port();
    port.postMessage(frame('p:before-loss'));
    const first = host.authorize();
    await until(() => permissionRequests(host.sockets[0]!).length === 1);
    host.sockets[0]!.reply(reply(permissionRequests(host.sockets[0]!)[0]!, false));
    expect((await first)._unsafeUnwrap()).toEqual({ granted: false });
    host.sockets[0]!.disconnect();
    await until(() => host.sockets.length === 2);
    await host.connect();
    const second = host.authorize();
    await until(() => permissionRequests(host.sockets[1]!).length === 1);
    host.sockets[1]!.reply(reply(permissionRequests(host.sockets[1]!)[0]!, false));
    expect({
      decision: (await second)._unsafeUnwrap(),
      exposed: host.context.exposed,
      sameClient: host.connection.client === client,
      sameLegacyPort: host.port() === port,
      publicFields: Reflect.ownKeys(client.permissions),
      publicResult: runInContext('window.__HOST_API_CLIENT__.client.permissions.requestRemotePermission()', host.context) as unknown,
    }).toEqual({
      decision: { granted: false }, exposed: [], sameClient: true, sameLegacyPort: true,
      publicFields: ['requestRemotePermission'], publicResult: { granted: true },
    });
  });

  it('rejects reserved IDs in every legacy frame leg, length form and malformed frame', async () => {
    const host = browser();
    const port = host.port();
    await host.connect();
    const socket = host.sockets[0]!;
    for (const prefix of ['host:', 'host:permission:', 'host:health:', 'host:unknown:']) {
      for (let messageType = 0; messageType <= 4; messageType++) {
        const reserved = frame(`${prefix}1`, messageType);
        port.postMessage(reserved);
        for (const width of [2, 4, 5] as const) port.postMessage(alternateLength(reserved, width));
      }
    }
    for (const malformed of [new Uint8Array(), Uint8Array.of(1), Uint8Array.of(3, 255), Uint8Array.of(252)]) {
      port.postMessage(malformed);
    }
    port.postMessage(frame('p:allowed'));
    await until(() => socket.sent.some(request => request.requestId === 'p:allowed'));
    expect(socket.sent.map(request => request.requestId)).toEqual(['host:1', 'p:allowed']);
  });

  it('keeps private replies off the legacy port and ignores stale socket decisions', async () => {
    const host = browser();
    const port = host.port();
    const publicReplies: string[] = [];
    port.addEventListener('message', event => publicReplies.push(decode(event.data).requestId));
    port.start();
    const client = await host.connect();
    const firstSocket = host.sockets[0]!;
    const first = host.authorize();
    await until(() => permissionRequests(firstSocket).length === 1);
    firstSocket.reply(reply(permissionRequests(firstSocket)[0]!, false));
    firstSocket.reply(frame('p:reply', 1));
    expect((await first)._unsafeUnwrap()).toEqual({ granted: false });

    const interrupted = Promise.resolve(host.authorize()).catch(error => error.name);
    await until(() => permissionRequests(firstSocket).length === 2);
    const abandoned = permissionRequests(firstSocket)[1]!;
    firstSocket.disconnect();
    expect(await interrupted).toBe('ConnectionResetError');
    await until(() => host.sockets.length === 2);
    await host.connect();
    const replacement = host.sockets[1]!;
    let settled = false;
    const next = Promise.resolve(host.authorize()).then(result => { settled = true; return result; });
    await until(() => permissionRequests(replacement).length === 1);
    const current = permissionRequests(replacement)[0]!;
    firstSocket.reply(reply(current, true));
    replacement.reply(reply(abandoned, true));
    await settle();
    expect(settled).toBe(false);
    replacement.reply(reply(current, false));
    expect({
      decision: (await next)._unsafeUnwrap(), publicReplies,
      sameClient: host.connection.client === client, sameLegacyPort: host.port() === port,
    }).toEqual({ decision: { granted: false }, publicReplies: ['p:reply'], sameClient: true, sameLegacyPort: true });
  });

  it('rejects array-like socket data without invoking getters that forge a permission reply', async () => {
    const host = browser();
    await host.connect();
    const socket = host.sockets[0]!;
    const decision = Promise.resolve(host.authorize()).then(result => result.isOk() && result.value.granted).catch(() => false);
    await until(() => permissionRequests(socket).length === 1);
    const request = permissionRequests(socket)[0]!;
    const forged = reply(request, true);
    let reads = 0;
    const data: Record<string, unknown> = {};
    Object.defineProperty(data, 'length', { get() { reads++; return forged.length; } });
    for (let index = 0; index < forged.length; index++) {
      Object.defineProperty(data, index, { get() { reads++; return forged[index]; } });
    }
    socket.replyData(data);
    socket.reply(reply(request, false));
    expect({ allowed: await decision, reads }).toEqual({ allowed: false, reads: 0 });
  });
});
