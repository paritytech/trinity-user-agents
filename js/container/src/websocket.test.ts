import { describe, expect, it } from 'bun:test';
import { installWebSocketGate } from './websocket.js';

/* eslint-disable @typescript-eslint/no-explicit-any */

function realm() {
  const connections: NativeSocket[] = [];
  const tasks: Array<() => void> = [];
  class NativeSocket extends EventTarget {
    private state = 0;
    private kind = 'blob';
    private selectedProtocol = '';
    private queued = 0;
    readonly sent: any[] = [];
    closeArguments: any[] = [];
    constructor(
      private address: string,
      readonly requested: string[] = [],
    ) {
      super();
      connections.push(this);
    }
    get url() {
      return this.address;
    }
    get readyState() {
      return this.state;
    }
    get binaryType() {
      return this.kind;
    }
    set binaryType(value: string) {
      this.kind = value;
    }
    get bufferedAmount() {
      return this.queued;
    }
    get protocol() {
      return this.selectedProtocol;
    }
    get extensions() {
      return this.state === 1 ? 'permessage-deflate' : '';
    }
    send(data: any) {
      this.sent.push(data);
      this.queued++;
    }
    close(code = 1000, reason = '') {
      this.closeArguments = Array.from(arguments);
      this.state = 3;
      this.dispatchEvent(
        new CloseEvent('close', { code, reason, wasClean: true }),
      );
    }
    open(protocol = '') {
      this.state = 1;
      this.selectedProtocol = protocol;
      this.dispatchEvent(new Event('open'));
    }
    message(data: any) {
      this.dispatchEvent(
        new MessageEvent('message', {
          data,
          origin: 'wss://api.example',
        }),
      );
    }
  }
  const win: any = {
    WebSocket: NativeSocket,
    EventTarget,
    Event,
    MessageEvent,
    CloseEvent,
    URL,
    TypeError,
    DOMException,
    TextEncoder,
    ArrayBuffer,
    Uint8Array,
    DataView,
    Blob,
    document: { baseURI: 'https://product.example/path/' },
    location: { href: 'https://product.example/path/' },
    setTimeout(task: () => void) {
      tasks.push(task);
      return tasks.length;
    },
  };
  return {
    win,
    NativeSocket,
    connections,
    flush() {
      while (tasks.length) tasks.shift()!();
    },
  };
}

function gated(factory?: (win: any) => any) {
  const context = realm();
  const requests: Array<{ url: string; decide: (allowed: boolean) => void }> =
    [];
  let cancelled = 0;
  installWebSocketGate(
    context.win,
    (url, decide) => {
      requests.push({ url, decide });
      return () => {
        cancelled++;
      };
    },
    factory?.(context.win),
  );
  return { ...context, requests, cancellations: () => cancelled };
}

describe('WebSocket connection permission', () => {
  it('omits an absent close code instead of passing undefined to the native WebIDL conversion', () => {
    const { win, requests, connections } = gated();
    const socket = new win.WebSocket('wss://api.example');
    requests[0]!.decide(true);
    connections[0]!.open();
    socket.close();
    expect(connections[0]!.closeArguments).toEqual([]);
  });

  it.each(['browser', 'script'])('connects only after consent and reuses it for all messages in %s', (runtime) => {
    const { win, requests, connections } = gated();
    if (runtime === 'script') delete win.document;
    const socket = new win.WebSocket('https://API.EXAMPLE/chat', ['chat']);
    expect([
      socket.readyState,
      socket.url,
      connections.length,
      requests.map((r) => r.url),
    ]).toEqual([0, 'wss://api.example/chat', 0, ['wss://api.example/chat']]);
    expect(() => socket.send('early')).toThrow('not open');
    socket.binaryType = 'arraybuffer';
    requests[0]!.decide(true);
    connections[0]!.open('chat');
    const bytes = new Uint8Array([1, 2]);
    socket.send('hello');
    socket.send(bytes);
    socket.send({ toString: () => 'converted' });
    expect([
      socket.readyState,
      socket.protocol,
      socket.extensions,
      socket.binaryType,
      socket.bufferedAmount,
      connections[0]!.sent,
      requests.length,
    ]).toEqual([
      1,
      'chat',
      'permessage-deflate',
      'arraybuffer',
      3,
      ['hello', bytes, 'converted'],
      1,
    ]);
    new win.WebSocket('wss://api.example/chat');
    expect(requests.length).toBe(2);
  });

  it('reports denial asynchronously as error and close without opening a socket', () => {
    const { win, requests, connections, flush } = gated();
    const socket = new win.WebSocket('wss://blocked.example');
    const events: any[] = [];
    socket.onerror = (event: Event) =>
      events.push([event.type, event.target, socket.readyState]);
    socket.onclose = (event: CloseEvent) =>
      events.push([event.type, event.code, event.wasClean]);
    requests[0]!.decide(false);
    requests[0]!.decide(true);
    expect([connections.length, events]).toEqual([0, []]);
    flush();
    expect(events).toEqual([
      ['error', socket, 3],
      ['close', 1006, false],
    ]);
  });

  it('cancels a pending decision and ignores approval arriving after close', () => {
    const { win, requests, connections, flush, cancellations } = gated();
    const socket = new win.WebSocket('wss://api.example');
    socket.close(1000, 'cancelled');
    expect([socket.readyState, cancellations()]).toEqual([2, 1]);
    requests[0]!.decide(true);
    flush();
    expect([socket.readyState, connections.length]).toEqual([3, 0]);
  });

  it('does not expose its native backend through forwarded events', () => {
    const { win, requests, connections } = gated();
    const socket = new win.WebSocket('wss://api.example');
    const received: any[] = [];
    socket.onmessage = function (this: any, event: MessageEvent) {
      received.push([
        this,
        event.target,
        event.currentTarget,
        event.composedPath(),
        event.data,
        event.origin,
        event.source,
        event.ports,
      ]);
    };
    requests[0]!.decide(true);
    connections[0]!.open();
    connections[0]!.message('hello');
    const bytes = new ArrayBuffer(2);
    connections[0]!.message(bytes);
    expect(received).toEqual([
      [
        socket,
        socket,
        socket,
        [socket],
        'hello',
        'wss://api.example',
        null,
        [],
      ],
      [socket, socket, socket, [socket], bytes, 'wss://api.example', null, []],
    ]);
    expect(Object.values(socket)).not.toContain(connections[0]);
  });

  it('preserves event listener ordering and handler replacement', () => {
    const { win, requests, connections } = gated();
    const socket = new win.WebSocket('wss://api.example');
    const order: string[] = [];
    socket.addEventListener('message', () => order.push('first'));
    socket.onmessage = () => order.push('old');
    socket.addEventListener('message', () => order.push('last'));
    socket.onmessage = () => order.push('replacement');
    requests[0]!.decide(true);
    connections[0]!.message('test');
    socket.onmessage = null;
    connections[0]!.message('test');
    expect(order).toEqual(['first', 'replacement', 'last', 'first', 'last']);
  });

  it('requires permission even for the known bridge URL and keeps constructors gated', () => {
    const { win, requests, connections, NativeSocket } = gated();
    const internal = new win.WebSocket('ws://127.0.0.1:9000/?t=secret');
    expect([requests.length, connections.length]).toEqual([1, 0]);
    expect(internal.constructor).toBe(win.WebSocket);
    new internal.constructor('wss://api.example');
    new win.WebSocket.prototype.constructor('wss://second.example');
    new win.WebSocket('ws://127.0.0.1:9000/?t=other');
    expect([requests.length, connections.length]).toEqual([4, 0]);
    requests[0]!.decide(true);
    expect(connections.map(connection => connection.url)).toEqual([
      'ws://127.0.0.1:9000/?t=secret',
    ]);
    expect(Object.getPrototypeOf(win.WebSocket)).not.toBe(NativeSocket);
    expect([
      win.WebSocket.CONNECTING,
      win.WebSocket.OPEN,
      win.WebSocket.CLOSING,
      win.WebSocket.CLOSED,
      win.WebSocket.prototype.OPEN,
    ]).toEqual([0, 1, 2, 3, 1]);
  });

  it('validates URLs and protocols before asking permission, including empty fragments', () => {
    const { win, requests, connections } = gated();
    for (const [url, protocols] of [
      ['file:///private', undefined],
      ['wss://api.example/#', undefined],
      ['wss://api.example/#fragment', undefined],
      ['wss://api.example', ['a', 'a']],
      ['wss://api.example', ['']],
      ['wss://api.example', ['not valid']],
    ])
      expect(() => new win.WebSocket(url, protocols)).toThrow();
    expect(() => new win.WebSocket()).toThrow();
    expect(() => new win.WebSocket(Symbol('url'))).toThrow();
    expect([requests, connections]).toEqual([[], []]);
  });

  it('snapshots relative URLs and mutable protocol sequences at construction', () => {
    const { win, requests, connections } = gated();
    const protocols = ['original'];
    const socket = new win.WebSocket('../chat', protocols);
    protocols[0] = 'changed';
    win.document.baseURI = 'https://other.example/';
    requests[0]!.decide(true);
    expect([
      socket.url,
      connections[0]!.url,
      connections[0]!.requested,
    ]).toEqual([
      'wss://product.example/chat',
      'wss://product.example/chat',
      ['original'],
    ]);
  });

  it('rejects invalid close arguments without cancelling a valid pending connection', () => {
    const { win, requests, connections } = gated();
    const socket = new win.WebSocket('wss://api.example');
    expect(() => socket.close(1001)).toThrow();
    expect(() => socket.close(1000, 'x'.repeat(124))).toThrow();
    expect(socket.readyState).toBe(0);
    requests[0]!.decide(true);
    expect(connections.length).toBe(1);
  });

  it('does not construct the native backend during a synchronous permission callback', () => {
    const { win, connections, flush } = realm();
    installWebSocketGate(win, (_url, decide) => {
      decide(true);
      expect(connections).toEqual([]);
      return () => {};
    });
    const socket = new win.WebSocket('wss://api.example');
    expect([socket.readyState, connections.length]).toEqual([0, 0]);
    flush();
    expect(connections.length).toBe(1);
  });

  it('cannot send before permission by shadowing public socket properties', () => {
    const { win, requests, connections, flush } = gated();
    const socket = new win.WebSocket('wss://api.example');
    Object.defineProperties(socket, {
      readyState: { value: 1 },
      url: { value: 'wss://other.example' },
      binaryType: { value: 'arraybuffer' },
    });
    expect(() => socket.send('forged')).toThrow('not open');
    requests[0]!.decide(false);
    flush();
    expect([requests.map((request) => request.url), connections]).toEqual([
      ['wss://api.example/'],
      [],
    ]);
  });

  it('keeps private state away from poisoned Object and collection prototypes', () => {
    const { win, requests, connections } = gated();
    const stolen: any[] = [];
    const originals = {
      weakGet: WeakMap.prototype.get,
      weakSet: WeakMap.prototype.set,
      apply: Reflect.apply,
    };
    let socket: any;
    try {
      for (const name of ['backend', 'cancel']) {
        Object.defineProperty(Object.prototype, name, {
          configurable: true,
          get() {
            stolen.push(this);
            return undefined;
          },
        });
      }
      WeakMap.prototype.get = function () {
        stolen.push(this);
        return undefined;
      };
      WeakMap.prototype.set = function () {
        stolen.push(this);
        return this;
      };
      Reflect.apply = function () {
        stolen.push(this);
        return undefined;
      };
      socket = new win.WebSocket('wss://api.example');
      void socket.readyState;
      void socket.protocol;
      requests[0]!.decide(true);
    } finally {
      for (const name of ['backend', 'cancel'])
        delete (Object.prototype as any)[name];
      WeakMap.prototype.get = originals.weakGet;
      WeakMap.prototype.set = originals.weakSet;
      Reflect.apply = originals.apply;
    }
    expect([stolen, connections[0]!.url, socket.url]).toEqual([
      [],
      'wss://api.example/',
      'wss://api.example/',
    ]);
  });

  it('does not expose a trusted backend through inherited descriptor getters', () => {
    const context = gated(() => (url: string) => {
      const backend = new EventTarget() as any;
      Object.assign(backend, {
        url,
        readyState: 0,
        binaryType: 'blob',
        bufferedAmount: 0,
        protocol: '',
        extensions: '',
        send() {},
        close() {},
      });
      return backend;
    });
    const socket = new context.win.WebSocket('wss://api.example');
    const stolen: any[] = [];
    try {
      const getter = Object.assign(Object.create(null), {
        configurable: true,
        value: function (this: any) {
          stolen.push(this);
          return 1;
        },
      });
      const setter = Object.assign(Object.create(null), {
        configurable: true,
        value: function (this: any) {
          stolen.push(this);
        },
      });
      Object.defineProperty(Object.prototype, 'get', getter);
      Object.defineProperty(Object.prototype, 'set', setter);
      context.requests[0]!.decide(true);
      void socket.readyState;
      socket.binaryType = 'arraybuffer';
    } finally {
      delete (Object.prototype as any).get;
      delete (Object.prototype as any).set;
    }
    expect(stolen).toEqual([]);
  });

  it('reads the caller protocol iterator once and protects the deferred snapshot', () => {
    const { win, requests, connections } = gated();
    let reads = 0;
    const input = {
      get [Symbol.iterator]() {
        reads++;
        return function* () {
          yield 'chat';
        };
      },
    };
    new win.WebSocket('wss://api.example', input);
    const original = Array.prototype[Symbol.iterator];
    let snapshot: string[];
    try {
      Array.prototype[Symbol.iterator] = function* () {
        yield 'forged';
      } as any;
      requests[0]!.decide(true);
      snapshot = Array.from(connections[0]!.requested);
      connections[0]!.open();
    } finally {
      Array.prototype[Symbol.iterator] = original;
    }
    expect([reads, snapshot!]).toEqual([1, ['chat']]);
  });

  it('uses the supplied trusted backend only after permission', () => {
    const created: any[] = [];
    const context = gated((win) => (url: string, protocols: string[]) => {
      const backend = new EventTarget() as any;
      Object.assign(backend, {
        url,
        protocols,
        readyState: 0,
        binaryType: 'blob',
        bufferedAmount: 0,
        protocol: '',
        extensions: '',
        send() {},
        close() {},
      });
      created.push(backend);
      return backend;
    });
    const socket = new context.win.WebSocket('wss://api.example', 'chat');
    expect(created).toEqual([]);
    context.requests[0]!.decide(true);
    const received: unknown[] = [];
    socket.onmessage = (event: MessageEvent) =>
      received.push([event.data, event.target]);
    created[0].dispatchEvent(new MessageEvent('message', { data: 'brokered' }));
    expect([
      created[0].url,
      created[0].protocols,
      received,
      context.connections,
    ]).toEqual(['wss://api.example/', ['chat'], [['brokered', socket]], []]);
  });
});
