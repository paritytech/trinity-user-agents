import { describe, expect, it } from 'bun:test';
import { createContext, runInContext } from 'node:vm';
import {
  ConnectionResetError,
  createTransport,
  decodeWireMessage,
  encodeWireMessage,
  MESSAGE_TYPE_RESPONSE,
  scale,
  type HostDevicePermissionRequest,
  VersionedRemotePermissionRequest,
  VersionedRemotePermissionResponse,
  VersionedRemotePermissionError,
  VersionedHostDevicePermissionRequest,
  VersionedHostDevicePermissionResponse,
  VersionedHostDevicePermissionError,
} from '@parity/truapi';
import {
  PERMISSIONS_AUTHORIZE_REMOTE_PERMISSION,
  PERMISSIONS_AUTHORIZE_DEVICE_PERMISSION,
} from '@parity/truapi/wire-table';
import { createInternalClient } from '@parity/truapi/internal';
import { createPermissionAuthorization } from './network-transport.js';
import { installFetchGate } from './network.js';
import { browserGlobals, browserScript, frameBytes } from './test-browser.js';

const container = await browserScript(`
  import { createTransport } from '@parity/truapi';
  import { createInternalClient } from '@parity/truapi/internal';
  import { installContainer } from './container.ts';
  import { createPermissionAuthorization } from './network-transport.ts';
  import { freezePermissionRuntime } from './permission-runtime.ts';
  freezePermissionRuntime();
  const connection = window.__test_permission_connection__;
  const nativeHttp = window.__test_native_http__;
  delete window.__test_permission_connection__;
  delete window.__test_native_http__;
  const client = connection && createInternalClient(createTransport(connection.provider, {
    prepare: () => connection.prepare(),
  }));
  installContainer(createPermissionAuthorization(window, client), { nativeHttp });
`);
const origin = 'https://product.example';
const settle = () => new Promise<void>(resolve => setTimeout(resolve, 0));

function permissionConnection(
  sent: Uint8Array[] = [],
  initiallyOpen = true,
) {
  let ready = initiallyOpen;
  let receive: ((frame: Uint8Array) => void) | undefined;
  let reset: ((error: Error) => void) | undefined;
  let opening: { promise: Promise<void>; resolve(): void; reject(error: Error): void } | undefined;
  const provider = {
    postMessage(frame: Uint8Array) {
      if (!ready) throw new ConnectionResetError();
      sent.push(frame);
    },
    subscribe(callback: (frame: Uint8Array) => void) {
      receive = callback;
      return () => { receive = undefined; };
    },
    subscribeReset(callback: (error: Error) => void) {
      reset = callback;
      return () => { reset = undefined; };
    },
    dispose() {},
  };
  const connection = {
    provider,
    sent,
    get client() {
      return createInternalClient(createTransport(provider, { prepare: () => connection.prepare() }));
    },
    prepare(): Promise<void> {
      if (ready) return Promise.resolve();
      if (!opening) {
        let resolve!: () => void;
        let reject!: (error: Error) => void;
        const promise = new Promise<void>((done, failed) => { resolve = done; reject = failed; });
        opening = { promise, resolve, reject };
      }
      return opening.promise;
    },
    receive(frame: Uint8Array) { receive?.(frame); },
    open() { ready = true; opening?.resolve(); opening = undefined; },
    disconnect() {
      ready = false;
      opening?.reject(new ConnectionResetError());
      opening = undefined;
      reset?.(new ConnectionResetError());
    },
  };
  return connection;
}

function permissionWindow(): Window & typeof globalThis {
  return {
    AbortController,
    URL,
  } as unknown as Window & typeof globalThis;
}

function grant(frame: Uint8Array): Uint8Array {
  const message = decodeWireMessage(frame)._unsafeUnwrap();
  const device = message.payload.methodId === PERMISSIONS_AUTHORIZE_DEVICE_PERMISSION.method;
  message.payload.messageType = MESSAGE_TYPE_RESPONSE;
  message.payload.value = scale.Result(
    device ? VersionedHostDevicePermissionResponse : VersionedRemotePermissionResponse,
    scale.CallError(device ? VersionedHostDevicePermissionError : VersionedRemotePermissionError),
  ).enc({ success: true, value: { tag: 'V1', value: { granted: true } } });
  return encodeWireMessage(message)._unsafeUnwrap();
}

describe('permission connection recovery', () => {
  it('fails interrupted requests and accepts a new decision after reconnect', async () => {
    const connection = permissionConnection();
    const authorize = createPermissionAuthorization(permissionWindow(), connection.client);
    const decisions: [string, boolean][] = [];
    authorize.network('https://old.example', (allowed) => decisions.push(['old', allowed]));
    await settle();
    expect(connection.sent).toHaveLength(1);
    connection.disconnect();
    await settle();
    authorize.network('https://new.example', (allowed) => decisions.push(['new', allowed]));
    connection.open();
    await settle();
    connection.receive(grant(connection.sent[0]!));
    connection.receive(grant(connection.sent[1]!));
    await settle();
    expect({
      decisions,
      ids: new Set(connection.sent.map(frame => decodeWireMessage(frame)._unsafeUnwrap().requestId)).size,
    }).toEqual({
      decisions: [['old', false], ['new', true]],
      ids: 2,
    });
  });

  it('denies every interrupted operation even when a callback throws or reenters', async () => {
    const connection = permissionConnection();
    const authorize = createPermissionAuthorization(permissionWindow(), connection.client);
    const decisions: [string, boolean][] = [];
    authorize.network('https://first.example', allowed => {
      decisions.push(['first', allowed]);
      authorize.network('https://fresh.example', fresh => decisions.push(['fresh', fresh]));
      throw new Error('product callback');
    });
    authorize.network('https://second.example', allowed => decisions.push(['second', allowed]));
    await settle();
    connection.disconnect();
    await settle();
    connection.open();
    await settle();
    connection.receive(grant(connection.sent[2]!));
    await settle();
    expect(decisions).toEqual([['first', false], ['second', false], ['fresh', true]]);
  });

  it('denies a request only once when sending throws', async () => {
    const connection = permissionConnection();
    connection.provider.postMessage = () => { throw new Error('socket lost'); };
    const authorize = createPermissionAuthorization(permissionWindow(), connection.client);
    const decisions: boolean[] = [];
    authorize.network('https://api.example', allowed => decisions.push(allowed));
    await settle();
    expect(decisions).toEqual([false]);
  });
});

function browser(
  authorize?: (domain: string) => boolean | Promise<boolean>,
  pageUrl = `${origin}/index.html`,
  transport: 'ready' | 'connecting' = 'ready',
  transformReply: (bytes: Uint8Array) => Uint8Array = (bytes) => bytes,
  authorizeWebRtc: () => boolean | Promise<boolean> = () => false,
  authorizeDevice: (request: HostDevicePermissionRequest) => boolean | Promise<boolean> = () => false,
  nativeHttp = false,
) {
  class BrowserRequest extends Request {
    constructor(input: RequestInfo | URL, init?: RequestInit) {
      super(
        input instanceof Request ? input : new URL(String(input), pageUrl),
        init,
      );
    }
  }
  for (const [name, descriptor] of Object.entries(
    Object.getOwnPropertyDescriptors(Request.prototype),
  )) {
    Object.defineProperty(BrowserRequest.prototype, name, {
      ...descriptor,
      configurable: true,
    });
  }

  const requests: Request[] = [];
  const sent: Uint8Array[] = [];
  const sockets: BrowserSocket[] = [];
  const deadlines: (() => void)[] = [];
  const sdkHandler = () => {};
  const sdkPort = { onmessage: sdkHandler };
  const globals = browserGlobals();
  const { EventTarget: BrowserEvents, MessageEvent: BrowserMessage } = globals;
  const connection = permissionConnection([], transport === 'ready');
  const send = connection.provider.postMessage;
  connection.provider.postMessage = frame => {
    send(frame);
    handle(frame, connection.receive);
  };
  const prepare = connection.prepare;
  connection.prepare = () => {
    queueMicrotask(() => connection.open());
    return prepare();
  };
  function handle(message: Uint8Array, deliver: (data: Uint8Array) => void) {
    message = frameBytes(message);
    sent.push(message);
    const decoded = decodeWireMessage(message)._unsafeUnwrap();
    if (decoded.payload.messageType === 4) return;
    const device = decoded.payload.methodId === PERMISSIONS_AUTHORIZE_DEVICE_PERMISSION.method;
    expect({
      trait: decoded.payload.traitId,
      method: decoded.payload.methodId,
      kind: 'request',
    }).toEqual(device ? PERMISSIONS_AUTHORIZE_DEVICE_PERMISSION : PERMISSIONS_AUTHORIZE_REMOTE_PERMISSION);
    let decision: boolean | Promise<boolean>;
    if (device) {
      decision = authorizeDevice(VersionedHostDevicePermissionRequest.dec(decoded.payload.value).value);
    } else {
      const { permission } = VersionedRemotePermissionRequest.dec(decoded.payload.value).value;
      if (permission.tag === 'Remote') {
        expect(permission.value.domains).toHaveLength(1);
        decision = authorize!(permission.value.domains[0]!);
      } else {
        expect(permission).toEqual({ tag: 'WebRtc' });
        decision = authorizeWebRtc();
      }
    }
    Promise.resolve(decision).then(
      (granted) => {
        const reply = encodeWireMessage({
          requestId: decoded.requestId,
          payload: {
            ...decoded.payload,
            messageType: MESSAGE_TYPE_RESPONSE,
            value: scale
              .Result(
                device ? VersionedHostDevicePermissionResponse : VersionedRemotePermissionResponse,
                scale.CallError(device ? VersionedHostDevicePermissionError : VersionedRemotePermissionError),
              )
              .enc({ success: true, value: { tag: 'V1', value: { granted } } }),
          },
        })._unsafeUnwrap();
        deliver(transformReply(reply));
      },
      () => connection.disconnect(),
    );
  }
  class BrowserSocket extends BrowserEvents {
    static CONNECTING = 0;
    static OPEN = 1;
    static CLOSING = 2;
    static CLOSED = 3;
    private state = 0;
    private binary = 'blob';
    get url() { return this.destination; }
    get readyState() { return this.state; }
    get bufferedAmount() { return 0; }
    get extensions() { return ''; }
    get protocol() { return ''; }
    get binaryType() { return this.binary; }
    set binaryType(value: string) { this.binary = value; }
    constructor(private destination: string) {
      super();
      sockets.push(this);
      queueMicrotask(() => {
        this.state = 1;
        this.dispatchEvent(new Event('open'));
      });
    }
    send(frame: Uint8Array | string) {
      this.dispatchEvent(new BrowserMessage('message', { data: frame }));
    }
    close() {
      this.state = 3;
      queueMicrotask(() => this.dispatchEvent(new CloseEvent('close', { code: 1000, wasClean: true })));
    }
  }
  class NativeXhr {}
  const context = createContext({
    ...globals,
    URL,
    Request: BrowserRequest,
    Response,
    AbortSignal,
    AbortController,
    DOMException,
    Event,
    CloseEvent,
    Blob,
    MessageChannel: class {},
    MessagePort: class {},
    setTimeout(callback: () => void, delay: number) {
      deadlines.push(callback);
      return setTimeout(callback, delay);
    },
    clearTimeout,
    WebSocket: BrowserSocket,
    XMLHttpRequest: nativeHttp ? NativeXhr : undefined,
    WebTransport: class {},
    Worker: class {},
    SharedWorker: class {},
    navigator: {},
    document: { createElement: () => ({}) },
    location: { href: pageUrl, origin: new URL(pageUrl).origin },
    fetch: async (input: RequestInfo | URL, init?: RequestInit) => {
      requests.push(new BrowserRequest(input, init));
      return new Response('received');
    },
    __HOST_API_PORT__: sdkPort,
    __test_permission_connection__: authorize ? connection : undefined,
    __test_native_http__: nativeHttp,
  });
  runInContext('window = globalThis', context);
  const FrameBytes = runInContext('Uint8Array', context);
  const receive = connection.receive;
  connection.receive = frame => receive(ArrayBuffer.isView(frame) ? new FrameBytes(frame) : frame);
  runInContext(`
    window.mediaCalls = [];
    window.navigator.mediaDevices = new (class {
      getUserMedia(constraints) {
        mediaCalls.push(constraints);
        return Promise.resolve('capture');
      }
    })();
    window.RTCPeerConnection = class {
      constructor(config = {}) { this.config = { ...config, iceCandidatePoolSize: config.iceCandidatePoolSize ?? 0 }; }
      getConfiguration() { return { ...this.config }; }
      setConfiguration(config) { this.config = { ...config }; }
      createOffer() { return Promise.resolve({ type: 'offer', sdp: 'native' }); }
      createAnswer() { return Promise.resolve({ type: 'answer', sdp: 'native' }); }
      setLocalDescription() { return Promise.resolve(); }
      setRemoteDescription() { return Promise.resolve(); }
      addIceCandidate() { return Promise.resolve(); }
      close() {}
    };
  `, context);
  const nativeFetch = context.fetch as typeof fetch;
  runInContext(container, context);
  return {
    context,
    requests,
    sent,
    sockets,
    sdkPort,
    sdkHandler,
    connection,
    deadlines,
    nativeFetch,
    NativeXhr,
    fetch: context.fetch as typeof fetch,
  };
}

describe('container fetch authorization', () => {
  it('preserves native HTTP interception while still checking new WebSockets', async () => {
    const authorized: string[] = [];
    const realm = browser(
      domain => { authorized.push(domain); return false; },
      undefined, 'ready', undefined, undefined, undefined, true,
    );
    await realm.fetch('https://api.example/native');
    await runInContext(`
      window.remote = new WebSocket('wss://api.example/socket');
      new Promise(resolve => remote.addEventListener('close', resolve, { once: true }));
    `, realm.context);
    expect({
      nativeFetch: realm.fetch === realm.nativeFetch,
      nativeXhr: realm.context.XMLHttpRequest === realm.NativeXhr,
      authorized,
      requests: realm.requests.map(request => request.url),
      sockets: realm.sockets.length,
    }).toEqual({
      nativeFetch: true,
      nativeXhr: true,
      authorized: ['api.example'],
      requests: ['https://api.example/native'],
      sockets: 0,
    });
  });

  it('requires permission for every origin when the runtime has no page URL', async () => {
    const runtime: typeof globalThis = Object.create(globalThis);
    const requested: string[] = [];
    installFetchGate(runtime, (url, decide) => {
      requested.push(url);
      decide(false);
      return () => {};
    });
    for (const url of ['https://product.example/', 'https://api.example/']) {
      await expect(runtime.fetch(url)).rejects.toThrow('Network access is not allowed');
    }
    expect(requested).toEqual(['https://product.example/', 'https://api.example/']);
  });

  it('uses one Remote decision per WebSocket whether authorization is ready or connecting', async () => {
    for (const transport of ['ready', 'connecting'] as const) {
      const authorized: string[] = [];
      const realm = browser((url) => {
        authorized.push(url);
        return authorized.length === 1;
      }, undefined, transport);
      await runInContext(`
        window.remote = new WebSocket('wss://api.example/socket');
        new Promise((resolve, reject) => {
          remote.addEventListener('open', resolve, { once: true });
          remote.addEventListener('error', reject, { once: true });
        });
      `, realm.context);
      const result = await runInContext(`
        new Promise(resolve => {
          remote.addEventListener('message', event => resolve({ data: event.data, target: event.target === remote }), { once: true });
          remote.send('first');
        });
      `, realm.context);
      expect(result).toEqual({ data: 'first', target: true });
      await runInContext(`
        window.denied = new WebSocket.prototype.constructor('wss://api.example/socket');
        new Promise(resolve => denied.addEventListener('close', resolve, { once: true }));
      `, realm.context);
      const second = await runInContext(`
        new Promise(resolve => {
          remote.addEventListener('message', event => resolve(event.data), { once: true });
          remote.send('still open');
        });
      `, realm.context);
      expect({
        authorized,
        sockets: realm.sockets.map(socket => socket.url),
        second,
      }).toEqual({
        authorized: ['api.example', 'api.example'],
        sockets: ['wss://api.example/socket'],
        second: 'still open',
      });
    }
  });

  it('requires a Remote decision for product-created localhost sockets', async () => {
    const authorized: string[] = [];
    const realm = browser(url => { authorized.push(url); return false; });
    for (const token of ['secret', 'other']) {
      await runInContext(`
        window.bridge = new WebSocket('ws://127.0.0.1:1234/?t=${token}');
        new Promise(resolve => bridge.addEventListener('close', resolve, { once: true }));
      `, realm.context);
    }
    expect({ authorized, sockets: realm.sockets.map(socket => socket.url) }).toEqual({
      authorized: ['127.0.0.1', '127.0.0.1'],
      sockets: [],
    });
  });

  it('authorizes each capture through the internal SDK client', async () => {
    for (const transport of ['ready', 'connecting'] as const) {
      const decisions: HostDevicePermissionRequest[] = [];
      const grants = [true, true, false, true, false];
      const realm = browser(() => false, undefined, transport, (bytes) => bytes,
        () => false, (request) => {
          decisions.push(request);
          return grants[decisions.length - 1]!;
        });
      const capture = (audio: boolean, video: boolean) => runInContext(
        `navigator.mediaDevices.getUserMedia({ audio: ${audio}, video: ${video} })`, realm.context);
      expect(await capture(true, true)).toBe('capture');
      await expect(capture(true, true)).rejects.toMatchObject({ name: 'NotAllowedError' });
      expect(await capture(true, false)).toBe('capture');
      await expect(capture(false, true)).rejects.toMatchObject({ name: 'NotAllowedError' });
      expect({ decisions, captures: realm.context.mediaCalls }).toEqual({
        decisions: ['Camera', 'Microphone', 'Camera', 'Microphone', 'Camera'],
        captures: [{ audio: true, video: true }, { audio: true, video: false }],
      });
    }
  });

  it('cancels whichever media permission is pending without continuing capture', async () => {
    for (const cancelAfterCamera of [false, true]) {
      const frames: Uint8Array[] = [];
      const connection = permissionConnection(frames);
      const { media } = createPermissionAuthorization(
        permissionWindow(), connection.client,
      );
      if (!media) throw new Error('Expected media authorization transport');
      const decisions: boolean[] = [];
      const cancel = media(true, true, (allowed) => decisions.push(allowed));
      await settle();
      if (cancelAfterCamera) connection.receive(grant(frames[0]!));
      await settle();
      cancel();
      const requests = frames.filter(frame => decodeWireMessage(frame)._unsafeUnwrap().payload.messageType === 0);
      connection.receive(grant(requests[requests.length - 1]!));
      await settle();
      expect({
        decisions,
        requested: requests.map((frame) =>
          VersionedHostDevicePermissionRequest.dec(
            decodeWireMessage(frame)._unsafeUnwrap().payload.value,
          ).value,
        ),
      }).toEqual({
        decisions: [],
        requested: cancelAfterCamera ? ['Camera', 'Microphone'] : ['Camera'],
      });
      media(false, false, (allowed) => decisions.push(allowed));
      await settle();
      expect(decisions).toEqual([false]);
    }
  });

  it('keeps completed camera consent while authorizing the microphone after reconnect', async () => {
    const connection = permissionConnection();
    const { media } = createPermissionAuthorization(permissionWindow(), connection.client);
    if (!media) throw new Error('Expected media authorization transport');
    const decisions: boolean[] = [];
    media(true, true, allowed => decisions.push(allowed));
    await settle();
    connection.receive(grant(connection.sent[0]!));
    connection.disconnect();
    connection.open();
    await settle();
    connection.receive(grant(connection.sent[1]!));
    await settle();
    expect({ decisions, requested: connection.sent.map(frame =>
      VersionedHostDevicePermissionRequest.dec(decodeWireMessage(frame)._unsafeUnwrap().payload.value).value),
    }).toEqual({ decisions: [true], requested: ['Camera', 'Microphone'] });
  });

  it('authorizes each peer connection through the internal SDK client', async () => {
    let authorizations = 0;
    const realm = browser(() => false, undefined, 'ready', (bytes) => bytes,
      () => ++authorizations === 1);
    const first = runInContext('new RTCPeerConnection()', realm.context);
    expect(await first.createOffer()).toEqual({ type: 'offer', sdp: 'native' });
    expect(await first.createOffer()).toEqual({ type: 'offer', sdp: 'native' });
    const second = runInContext('new RTCPeerConnection()', realm.context);
    await expect(second.createOffer()).rejects.toThrow('WebRTC access is not allowed');
    expect({ authorizations, fetches: realm.requests.length }).toEqual({
      authorizations: 2,
      fetches: 0,
    });
    first.close();
    second.close();
  });

  it('sends authorization through the internal SDK client without replacing the legacy port', async () => {
    const realm = browser(() => true);
    await realm.fetch('https://api.example/data');
    expect({
      sent: realm.sent.length,
      sdkHandler: realm.sdkPort.onmessage,
    }).toEqual({ sent: 1, sdkHandler: realm.sdkHandler });
  });
  it('uses an existing grant immediately for a cross-origin fetch', async () => {
    const authorized: string[] = [];
    const realm = browser(async (url) => {
      authorized.push(url);
      return true;
    });
    const response = await realm.fetch('https://api.example/data');
    expect({
      status: response.status,
      authorized,
      requested: realm.requests.map((request) => request.url),
    }).toEqual({
      status: 200,
      authorized: ['api.example'],
      requested: ['https://api.example/data'],
    });
  });

  it('waits for SDK readiness without creating another socket', async () => {
    const authorized: string[] = [];
    const realm = browser(
      (url) => {
        authorized.push(url);
        return true;
      },
      undefined,
      'connecting',
    );
    await realm.fetch('https://api.example/data');
    expect({
      authorized,
      sockets: realm.sockets.map((socket) => socket.url),
      requests: realm.requests.length,
    }).toEqual({
      authorized: ['api.example'],
      sockets: [],
      requests: 1,
    });
  });

  it('correlates concurrent replies and rejects stale grants for a different request', async () => {
    const decisions = new Map<string, (allowed: boolean) => void>();
    const realm = browser(
      (url) =>
        new Promise((resolve) => {
          decisions.set(url, resolve);
        }),
    );
    const denied = realm.fetch('https://denied.example/data');
    const granted = realm.fetch('https://allowed.example/data');
    await settle();
    decisions.get('allowed.example')!(true);
    await granted;
    const stale = decodeWireMessage(realm.sent[1]!)._unsafeUnwrap();
    stale.payload.messageType = MESSAGE_TYPE_RESPONSE;
    stale.payload.value = scale
      .Result(
        VersionedRemotePermissionResponse,
        scale.CallError(VersionedRemotePermissionError),
      )
      .enc({ success: true, value: { tag: 'V1', value: { granted: true } } });
    realm.connection.receive(encodeWireMessage(stale)._unsafeUnwrap());
    decisions.get('denied.example')!(false);
    await expect(denied).rejects.toThrow('Network access is not allowed');
    expect(realm.requests.map((request) => request.url)).toEqual([
      'https://allowed.example/data',
    ]);
  });

  for (const corruption of [
    'method',
    'message type',
    'truncated payload',
    'invalid boolean',
  ]) {
    it(`rejects a grant reply with ${corruption}`, async () => {
      const realm = browser(
        () => true,
        undefined,
        'ready',
        (frame) => {
          const decoded = decodeWireMessage(frame)._unsafeUnwrap();
          if (corruption === 'method') decoded.payload.methodId++;
          if (corruption === 'message type') decoded.payload.messageType++;
          if (corruption === 'truncated payload')
            decoded.payload.value = decoded.payload.value.slice(0, -1);
          if (corruption === 'invalid boolean')
            decoded.payload.value[decoded.payload.value.length - 1] = 2;
          return encodeWireMessage(decoded)._unsafeUnwrap();
        },
      );
      const pending = realm.fetch('https://denied.example/data');
      const rejected = pending.catch(error => error);
      await settle();
      for (const deadline of realm.deadlines) deadline();
      expect(await rejected).toMatchObject({ message: 'Network access is not allowed' });
      expect(realm.requests).toEqual([]);
    });
  }

  it('encodes concrete domains with the generated permission schema', async () => {
    const authorized: string[] = [];
    const realm = browser((url) => {
      authorized.push(url);
      return true;
    });
    const urls = [
      'https://API.EXAMPLE:8443/short',
      'https://Bücher.example/雪',
      `https://${'a'.repeat(100)}.example/path`,
      `https://${'b'.repeat(16_400)}.example/path`,
    ];
    for (const url of urls) await realm.fetch(url);
    expect(authorized).toEqual(urls.map((url) => new URL(url).hostname));
  });

  it('denies interrupted fetches and authorizes later fetches after reconnect', async () => {
    let interrupted = true;
    const realm = browser(() => interrupted ? new Promise(() => {}) : true);
    const pending = realm.fetch('https://api.example/pending');
    await settle();
    realm.connection.disconnect();
    await expect(pending).rejects.toThrow('Network access is not allowed');
    interrupted = false;
    await realm.fetch('https://api.example/later');
    expect({ frames: realm.sent.length, requests: realm.requests.map(request => request.url) }).toEqual({
      frames: 2,
      requests: ['https://api.example/later'],
    });
  });

  it('executes an authorized fetch when the connection closes after its reply', async () => {
    const realm = browser(() => new Promise(() => {}));
    const pending = realm.fetch('https://api.example/authorized');
    await settle();
    realm.connection.receive(grant(realm.sent[0]!));
    realm.connection.disconnect();
    await pending;
    expect(realm.requests.map(request => request.url)).toEqual(['https://api.example/authorized']);
  });

  it('bounds an unanswered permission request and ignores a late approval', async () => {
    let reply!: (allowed: boolean) => void;
    const realm = browser(
      () =>
        new Promise((resolve) => {
          reply = resolve;
        }),
    );
    const pending = realm.fetch('https://denied.example/data');
    await settle();
    realm.deadlines[0]!();
    await expect(pending).rejects.toThrow('Network access is not allowed');
    reply(true);
    await Promise.resolve();
    expect(realm.requests).toEqual([]);
  });

  it('ignores forged public SDK replies and a replacement legacy authorization hook', async () => {
    const realm = browser(() => false);
    realm.context.__truapi_network__ = async () => true;
    realm.context.__HOST_API_PORT__ = { onmessage: null, postMessage() {} };
    await expect(realm.fetch('https://denied.example/data')).rejects.toThrow(
      'Network access is not allowed',
    );
    expect(realm.requests).toEqual([]);
  });

  for (const [name, attack] of [
    ['Object.fromEntries', `
      const original = Object.fromEntries;
      Object.fromEntries = entries => {
        const result = original(entries);
        if ('granted' in result) result.granted = true;
        return result;
      };
    `],
    ['Array.map', `
      const original = Array.prototype.map;
      Array.prototype.map = function (...args) {
        const result = Reflect.apply(original, this, args);
        for (const entry of result)
          if (Array.isArray(entry) && entry[0] === 'granted') entry[1] = true;
        return result;
      };
    `],
    ['Map.set', `
      const original = Map.prototype.set;
      Map.prototype.set = function (key, value) {
        if (typeof value?.resolve === 'function') value.resolve(Uint8Array.of(0, 0, 1));
        return Reflect.apply(original, this, [key, value]);
      };
    `],
    ['the Map constructor', `
      const NativeMap = Map;
      window.Map = class extends NativeMap {
        get(key) {
          const entry = super.get(key);
          return Array.isArray(entry) && entry[0] === 'V1'
            ? ['V1', { dec: () => ({ granted: true }) }] : entry;
        }
      };
    `],
    ['WeakMap.get', `
      WeakMap.prototype.get = () => ({
        request: () => Promise.resolve({ isOk: () => true, value: { granted: true } }),
      });
    `],
    ['Promise.then', `
      const original = Promise.prototype.then;
      Promise.prototype.then = function (fulfilled, rejected) {
        return Reflect.apply(original, this, [value => {
          if (value?.value && 'granted' in value.value) value.value.granted = true;
          return fulfilled ? fulfilled(value) : value;
        }, rejected]);
      };
    `],
    ['an inherited then hook', `
      Object.defineProperty(Object.prototype, 'then', { value(resolve) {
        Object.defineProperty(this, 'then', { value: undefined });
        if (this.value && 'granted' in this.value) this.value.granted = true;
        resolve(this);
      } });
    `],
    ['DataView.getUint8', `
      const original = DataView.prototype.getUint8;
      DataView.prototype.getUint8 = function (offset) {
        return this.byteLength === 3 && offset === 2 ? 1 : Reflect.apply(original, this, [offset]);
      };
    `],
    ['TextEncoder.encode', `
      const original = TextEncoder.prototype.encode;
      TextEncoder.prototype.encode = function (text) {
        return Reflect.apply(original, this, [text === 'denied.example' ? 'allowed.example' : text]);
      };
    `],
  ]) {
    it(`cannot make native fetch run without its grant through ${name}`, async () => {
      const authorized: string[] = [];
      const realm = browser(domain => { authorized.push(domain); return domain === 'allowed.example'; });
      runInContext(`try { ${attack} } catch (error) { if (!(error instanceof TypeError)) throw error; }`, realm.context);
      const decision = await realm.fetch('https://denied.example/data').then(() => 'allowed', () => 'denied');
      expect({ decision, authorized, requests: realm.requests.map(request => request.url) }).toEqual({
        decision: 'denied', authorized: ['denied.example'], requests: [],
      });
    });
  }

  it('sends no network request when Rust denies authorization', async () => {
    const realm = browser(async () => false);
    await expect(realm.fetch('https://denied.example/data')).rejects.toThrow(
      'Network access is not allowed',
    );
    expect(realm.requests).toEqual([]);
  });

  it('fails closed when the host transport is missing', async () => {
    const realm = browser();
    await expect(realm.fetch('https://denied.example/data')).rejects.toThrow(
      'Network access is not allowed',
    );
    expect({
      requests: realm.requests,
      peerConnection: realm.context.RTCPeerConnection,
    }).toEqual({ requests: [], peerConnection: undefined });
  });

  it('fails closed when the host authorization fails', async () => {
    const realm = browser(async () => {
      throw new Error('host disconnected');
    });
    await expect(realm.fetch('https://denied.example/data')).rejects.toThrow(
      'Network access is not allowed',
    );
    expect(realm.requests).toEqual([]);
  });

  it('leaves same-origin resources available without prompting', async () => {
    const realm = browser();
    await realm.fetch('/asset.json');
    expect(realm.requests.map((request) => request.url)).toEqual([
      `${origin}/asset.json`,
    ]);
  });

  it('does not consume a remote grant for same-origin fetches', async () => {
    const authorized: string[] = [];
    const realm = browser(async (url) => {
      authorized.push(url);
      return true;
    });
    await realm.fetch('/asset.json');
    expect(authorized).toEqual([]);
  });

  it('does not treat distinct native product origins as the same null origin', async () => {
    const realm = browser(undefined, 'polkadot://product/index.html');
    await realm.fetch('/asset.json');
    await expect(realm.fetch('polkadot://other/asset.json')).rejects.toThrow(
      'Network access is not allowed',
    );
    expect(realm.requests.map((request) => request.url)).toEqual([
      'polkadot://product/asset.json',
    ]);
  });

  it('does not authorize non-HTTP remote requests', async () => {
    const authorized: string[] = [];
    const realm = browser(async (url) => {
      authorized.push(url);
      return true;
    });
    for (const url of ['file:///secret', 'https://*.example/data']) {
      await expect(realm.fetch(url)).rejects.toThrow('Network access is not allowed');
    }
    expect({ authorized, requests: realm.requests }).toEqual({
      authorized: [],
      requests: [],
    });
  });

  it('uses current host decisions after a grant is revoked', async () => {
    let granted = true;
    const realm = browser(async () => granted);
    await realm.fetch('https://api.example/data');
    granted = false;
    await expect(realm.fetch('https://api.example/data')).rejects.toThrow(
      'Network access is not allowed',
    );
    expect(realm.requests.map((request) => request.url)).toEqual([
      'https://api.example/data',
    ]);
  });

  it('snapshots the destination, headers and body before awaiting permission', async () => {
    let grant!: (allowed: boolean) => void;
    const authorized: string[] = [];
    const realm = browser((url) => {
      authorized.push(url);
      return new Promise<boolean>((resolve) => {
        grant = resolve;
      });
    });
    const url = new URL('https://api.example/data');
    const headers = new Headers({ 'x-product': 'original' });
    const options = { method: 'POST', body: 'original', headers };
    const pending = realm.fetch(url, options);
    url.hostname = 'denied.example';
    headers.set('x-product', 'changed');
    options.body = 'changed';
    await settle();
    grant(true);
    await pending;
    const request = realm.requests[0];
    expect({
      authorized,
      url: request.url,
      method: request.method,
      body: await request.text(),
      header: request.headers.get('x-product'),
    }).toEqual({
      authorized: ['api.example'],
      url: 'https://api.example/data',
      method: 'POST',
      body: 'original',
      header: 'original',
    });
  });

  it('preserves Request input and fetch overrides', async () => {
    const realm = browser(async () => true);
    const input = new Request('https://api.example/data', {
      method: 'POST',
      body: 'body',
      credentials: 'include',
    });
    await realm.fetch(input, {
      headers: { 'x-product': 'override' },
      redirect: 'error',
    });
    const request = realm.requests[0];
    expect({
      url: request.url,
      method: request.method,
      body: await request.text(),
      credentials: request.credentials,
      redirect: request.redirect,
      header: request.headers.get('x-product'),
    }).toEqual({
      url: 'https://api.example/data',
      method: 'POST',
      body: 'body',
      credentials: 'include',
      redirect: 'error',
      header: 'override',
    });
  });

  it('rejects an aborted request while permission is still pending', async () => {
    let grant!: (allowed: boolean) => void;
    const realm = browser(
      () =>
        new Promise<boolean>((resolve) => {
          grant = resolve;
        }),
    );
    const controller = new AbortController();
    const pending = realm.fetch('https://api.example/data', {
      signal: controller.signal,
    });
    await settle();
    controller.abort(new Error('cancelled'));
    await expect(pending).rejects.toThrow('cancelled');
    grant(true);
    await Promise.resolve();
    expect(realm.requests).toEqual([]);
  });

  it('does not prompt for an already aborted request', async () => {
    const authorized: string[] = [];
    const realm = browser(async (url) => {
      authorized.push(url);
      return true;
    });
    const controller = new AbortController();
    controller.abort(new Error('cancelled'));
    await expect(
      realm.fetch('https://api.example/data', { signal: controller.signal }),
    ).rejects.toThrow('cancelled');
    expect({ authorized, requests: realm.requests }).toEqual({
      authorized: [],
      requests: [],
    });
  });

  it('checks native Request URLs even when the product changes their getters', async () => {
    const authorized: string[] = [];
    const realm = browser(async (url) => {
      authorized.push(url);
      return false;
    });
    runInContext(
      `Object.defineProperty(Request.prototype, 'url', { get() { return '${origin}/asset.json'; } });`,
      realm.context,
    );
    await expect(realm.fetch('https://denied.example/data')).rejects.toThrow(
      'Network access is not allowed',
    );
    expect({ authorized, requests: realm.requests }).toEqual({
      authorized: ['denied.example'],
      requests: [],
    });
  });

  it('blocks workers that would otherwise have an unguarded fetch', () => {
    const realm = browser();
    expect({
      worker: realm.context.Worker,
      sharedWorker: realm.context.SharedWorker,
    }).toEqual({ worker: undefined, sharedWorker: undefined });
  });

  it('blocks WebTransport egress outside the HTTP request gate', () => {
    expect(browser().context.WebTransport).toBeUndefined();
  });
});
