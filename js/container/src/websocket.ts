/* eslint-disable @typescript-eslint/no-explicit-any */

import { freezeValue } from './freeze.js';
import type { NetworkAuthorization } from './network-transport.js';

export interface WebSocketBackend extends EventTarget {
  readonly url: string;
  readonly readyState: number;
  readonly bufferedAmount: number;
  readonly extensions: string;
  readonly protocol: string;
  binaryType: BinaryType;
  send(data: string | ArrayBufferLike | Blob | ArrayBufferView): void;
  close(code?: number, reason?: string): void;
}

export type WebSocketBackendFactory = (
  url: string,
  protocols: string[],
) => WebSocketBackend;

interface SocketState {
  url: string;
  phase: number;
  binaryType: BinaryType;
  bufferedAmount: number;
  pending: boolean;
  cancel?: () => void;
  backend?: {
    read(name: string): any;
    binaryType(value: BinaryType): void;
    send(data: any): void;
    close(code?: number, reason?: string): void;
  };
  handlers: Record<string, { callback: any; listener: (event: Event) => void }>;
}

export function installWebSocketGate(
  win: typeof globalThis,
  authorize: NetworkAuthorization,
  factory?: WebSocketBackendFactory,
): void {
  const NativeSocket = win.WebSocket;
  if (!NativeSocket) return;
  const apply = Reflect.apply;
  const descriptor = Object.getOwnPropertyDescriptor;
  const define = Object.defineProperty;
  const getPrototype = Object.getPrototypeOf;
  const freeze = Object.freeze;
  const create = Object.create;
  const owns = Object.prototype.hasOwnProperty;
  const nativePrototype = NativeSocket.prototype;
  const nativeSend = nativePrototype.send;
  const nativeClose = nativePrototype.close;
  const nativeProperties: Record<string, PropertyDescriptor> = create(null);
  const propertyNames = [
    'readyState',
    'bufferedAmount',
    'extensions',
    'protocol',
    'binaryType',
  ];
  for (const name of propertyNames)
    nativeProperties[name] = descriptor(nativePrototype, name)!;
  const NativeTarget = win.EventTarget;
  const add = NativeTarget.prototype.addEventListener;
  const remove = NativeTarget.prototype.removeEventListener;
  const dispatch = NativeTarget.prototype.dispatchEvent;
  const NativeEvent = win.Event;
  const NativeMessage = win.MessageEvent;
  const NativeClose = win.CloseEvent;
  const messageData = descriptor(NativeMessage.prototype, 'data')!.get!;
  const messageOrigin = descriptor(NativeMessage.prototype, 'origin')!.get!;
  const messageId = descriptor(NativeMessage.prototype, 'lastEventId')!.get!;
  const closeCode = descriptor(NativeClose.prototype, 'code')!.get!;
  const closeReason = descriptor(NativeClose.prototype, 'reason')!.get!;
  const closeClean = descriptor(NativeClose.prototype, 'wasClean')!.get!;
  const NativeError = win.DOMException;
  const NativeTypeError = win.TypeError;
  const NativeURL = win.URL;
  const href = descriptor(NativeURL.prototype, 'href')!.get!;
  const scheme = descriptor(NativeURL.prototype, 'protocol')!;
  const baseURI = win.Node && descriptor(win.Node.prototype, 'baseURI')?.get;
  const stringify = String;
  const indexOf = String.prototype.indexOf;
  const test = RegExp.prototype.test;
  const protocolToken = /^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/;
  const iterator = Symbol.iterator;
  const states = new WeakMap<object, SocketState>();
  const weakGet = WeakMap.prototype.get;
  const weakSet = WeakMap.prototype.set;
  const schedule = win.setTimeout.bind(win);
  const encoder = new win.TextEncoder();
  const encode = win.TextEncoder.prototype.encode;
  const bufferLength = descriptor(
    win.ArrayBuffer.prototype,
    'byteLength',
  )!.get!;
  const viewPrototype = getPrototype(win.Uint8Array.prototype);
  const viewLength = descriptor(viewPrototype, 'byteLength')!.get!;
  const dataLength = descriptor(win.DataView.prototype, 'byteLength')!.get!;
  const blobSize = descriptor(win.Blob.prototype, 'size')!.get!;
  const isView = win.ArrayBuffer.isView;
  const floor = Math.floor;
  const min = Math.min;
  const max = Math.max;

  function state(socket: object): SocketState {
    const value = apply(weakGet, states, [socket]);
    if (!value) throw new NativeTypeError('Illegal WebSocket receiver');
    return value;
  }

  function text(value: any): string {
    if (typeof value === 'symbol')
      throw new NativeTypeError('Cannot convert a Symbol to a string');
    return stringify(value);
  }

  function protocols(value: any): string[] {
    const result: string[] = [];
    if (value !== undefined) {
      const method =
        value !== null &&
        (typeof value === 'object' || typeof value === 'function')
          ? value[iterator]
          : undefined;
      if (method !== undefined && method !== null) {
        const sequence: any = apply(method, value, []);
        if (
          sequence === null ||
          (typeof sequence !== 'object' && typeof sequence !== 'function')
        )
          throw new NativeTypeError('Invalid WebSocket protocol iterator');
        const next = sequence.next;
        while (true) {
          const item: any = apply(next, sequence, []);
          if (
            item === null ||
            (typeof item !== 'object' && typeof item !== 'function')
          )
            throw new NativeTypeError(
              'Invalid WebSocket protocol iterator result',
            );
          if (item.done) break;
          result[result.length] = text(item.value);
        }
      } else result[0] = text(value);
    }
    for (let index = 0; index < result.length; index++) {
      const protocol = result[index]!;
      if (!apply(test, protocolToken, [protocol]))
        throw new NativeError('Invalid WebSocket protocol', 'SyntaxError');
      for (let previous = 0; previous < index; previous++) {
        if (result[previous] === protocol)
          throw new NativeError('Duplicate WebSocket protocol', 'SyntaxError');
      }
    }
    const iteration = create(null);
    iteration.value = function () {
      let index = 0;
      return {
        next() {
          return index < result.length
            ? { value: result[index++], done: false }
            : { value: undefined, done: true };
        },
      };
    };
    define(result, iterator, iteration);
    freeze(result);
    return result;
  }

  function cancel(current: SocketState): void {
    current.pending = false;
    current.cancel?.();
    current.cancel = undefined;
  }

  function fail(socket: GatedWebSocket, current: SocketState): void {
    cancel(current);
    current.phase = 2;
    schedule(() => {
      current.phase = 3;
      apply(dispatch, socket, [new NativeEvent('error')]);
      apply(dispatch, socket, [
        new NativeClose('close', {
          code: 1006,
          reason: '',
          wasClean: false,
        }),
      ]);
    }, 0);
  }

  function connect(
    socket: GatedWebSocket,
    current: SocketState,
    requested: string[],
  ): void {
    const backend = factory
      ? factory(current.url, requested)
      : new NativeSocket(current.url, requested);
    const properties: Record<string, PropertyDescriptor> = create(null);
    for (let index = 0; index < propertyNames.length; index++) {
      const name = propertyNames[index]!;
      if (!factory) properties[name] = nativeProperties[name]!;
      else {
        let object: any = backend;
        while (object && !properties[name]) {
          properties[name] = descriptor(object, name)!;
          object = getPrototype(object);
        }
      }
    }
    const send = factory ? backend.send : nativeSend;
    const close = factory ? backend.close : nativeClose;
    current.backend = {
      read(name) {
        const property = properties[name];
        const get =
          property && apply(owns, property, ['get']) ? property.get : undefined;
        return get ? apply(get, backend, []) : (backend as any)[name];
      },
      binaryType(value) {
        const property = properties.binaryType;
        const set =
          property && apply(owns, property, ['set']) ? property.set : undefined;
        if (set) apply(set, backend, [value]);
        else backend.binaryType = value;
      },
      send(data) {
        apply(send, backend, [data]);
      },
      close(code, reason) {
        apply(close, backend, code === undefined ? [] : [code, reason]);
      },
    };
    current.backend.binaryType(current.binaryType);
    apply(add, backend, [
      'open',
      () => {
        apply(dispatch, socket, [new NativeEvent('open')]);
      },
    ]);
    apply(add, backend, [
      'message',
      (event: MessageEvent) => {
        apply(dispatch, socket, [
          new NativeMessage('message', {
            data: apply(messageData, event, []),
            origin: apply(messageOrigin, event, []),
            lastEventId: apply(messageId, event, []),
            source: null,
            ports: [],
          }),
        ]);
      },
    ]);
    apply(add, backend, [
      'error',
      () => {
        apply(dispatch, socket, [new NativeEvent('error')]);
      },
    ]);
    apply(add, backend, [
      'close',
      (event: CloseEvent) => {
        apply(dispatch, socket, [
          new NativeClose('close', {
            code: apply(closeCode, event, []),
            reason: apply(closeReason, event, []),
            wasClean: apply(closeClean, event, []),
          }),
        ]);
      },
    ]);
  }

  function validateClose(code: any, reason: any): [number | undefined, string] {
    let converted: number | undefined;
    if (code !== undefined) {
      const value = +code;
      const clamped = value !== value ? 0 : min(65535, max(0, value));
      const lower = floor(clamped);
      converted =
        clamped - lower === 0.5
          ? lower % 2 === 0
            ? lower
            : lower + 1
          : floor(clamped + 0.5);
      if (converted !== 1000 && (converted < 3000 || converted > 4999))
        throw new NativeError(
          'Invalid WebSocket close code',
          'InvalidAccessError',
        );
    }
    const description = reason === undefined ? '' : text(reason);
    if (apply(encode, encoder, [description]).length > 123)
      throw new NativeError(
        'WebSocket close reason is too long',
        'SyntaxError',
      );
    return [converted, description];
  }

  function payload(data: any): any {
    if (isView(data)) return data;
    try {
      apply(bufferLength, data, []);
      return data;
    } catch {
      /* another data type */
    }
    try {
      apply(blobSize, data, []);
      return data;
    } catch {
      /* another data type */
    }
    return text(data);
  }

  function dataSize(data: any): number {
    if (isView(data)) {
      try {
        return apply(viewLength, data, []);
      } catch {
        return apply(dataLength, data, []);
      }
    }
    try {
      return apply(bufferLength, data, []);
    } catch {
      /* another data type */
    }
    try {
      return apply(blobSize, data, []);
    } catch {
      /* another data type */
    }
    return apply(encode, encoder, [text(data)]).length;
  }

  class GatedWebSocket extends NativeTarget {
    constructor(input: string | URL, offered?: string | string[]) {
      super();
      if (!arguments.length)
        throw new NativeTypeError('WebSocket requires a URL');
      const originalUrl = text(input);
      const requested = protocols(offered);
      let url: URL;
      try {
        url = new NativeURL(
          originalUrl,
          baseURI ? apply(baseURI, win.document, []) : win.document?.baseURI,
        );
      } catch {
        throw new NativeError('Invalid WebSocket URL', 'SyntaxError');
      }
      const protocol = apply(scheme.get!, url, []);
      if (protocol === 'http:') apply(scheme.set!, url, ['ws:']);
      else if (protocol === 'https:') apply(scheme.set!, url, ['wss:']);
      else if (protocol !== 'ws:' && protocol !== 'wss:')
        throw new NativeError('Invalid WebSocket URL scheme', 'SyntaxError');
      const address = apply(href, url, []);
      if (apply(indexOf, address, ['#']) !== -1)
        throw new NativeError(
          'WebSocket URLs cannot contain fragments',
          'SyntaxError',
        );
      const current: SocketState = {
        url: address,
        phase: 0,
        binaryType: 'blob',
        bufferedAmount: 0,
        pending: true,
        backend: undefined,
        cancel: undefined,
        handlers: create(null),
      };
      apply(weakSet, states, [this, current]);
      let constructing = true;
      const decided = (allowed: boolean) => {
        if (constructing) {
          schedule(() => decided(allowed), 0);
          return;
        }
        if (!current.pending) return;
        cancel(current);
        if (allowed !== true) return fail(this, current);
        try {
          connect(this, current, requested);
        } catch {
          fail(this, current);
        }
      };
      try {
        const cancellation = authorize(address, decided);
        if (!current.pending) cancellation();
        else current.cancel = cancellation;
      } catch {
        fail(this, current);
      }
      constructing = false;
    }

    get url() {
      return state(this).url;
    }
    get readyState() {
      const current = state(this);
      return current.backend?.read('readyState') ?? current.phase;
    }
    get bufferedAmount() {
      const current = state(this);
      return current.backend?.read('bufferedAmount') ?? current.bufferedAmount;
    }
    get protocol() {
      return state(this).backend?.read('protocol') ?? '';
    }
    get extensions() {
      return state(this).backend?.read('extensions') ?? '';
    }
    get binaryType() {
      return state(this).binaryType;
    }
    set binaryType(value: BinaryType) {
      const current = state(this);
      const converted = text(value);
      if (converted !== 'blob' && converted !== 'arraybuffer') return;
      current.binaryType = converted;
      current.backend?.binaryType(converted);
    }
    send(data: any): void {
      const current = state(this);
      if (!arguments.length)
        throw new NativeTypeError('WebSocket.send requires data');
      const converted = payload(data);
      if ((current.backend?.read('readyState') ?? current.phase) === 0)
        throw new NativeError('WebSocket is not open', 'InvalidStateError');
      if (current.backend) current.backend.send(converted);
      else current.bufferedAmount += dataSize(converted);
    }
    close(code?: number, reason?: string): void {
      const current = state(this);
      const converted = validateClose(code, reason);
      if (current.backend) current.backend.close(converted[0], converted[1]);
      else if (current.phase === 0) fail(this, current);
    }
  }

  for (const name of ['open', 'message', 'error', 'close']) {
    define(GatedWebSocket.prototype, `on${name}`, {
      configurable: false,
      enumerable: true,
      get() {
        return state(this).handlers[name]?.callback ?? null;
      },
      set(value: any) {
        const current = state(this);
        const previous = current.handlers[name];
        if (typeof value !== 'function') {
          if (previous) apply(remove, this, [name, previous.listener]);
          delete current.handlers[name];
        } else if (previous) previous.callback = value;
        else {
          const handler = {
            callback: value,
            listener: (event: Event) => {
              apply(handler.callback, this, [event]);
            },
          };
          current.handlers[name] = handler;
          apply(add, this, [name, handler.listener]);
        }
      },
    });
  }
  for (const name of [
    'url',
    'readyState',
    'bufferedAmount',
    'protocol',
    'extensions',
    'binaryType',
  ])
    define(GatedWebSocket.prototype, name, {
      ...descriptor(GatedWebSocket.prototype, name),
      configurable: false,
    });
  freezeValue(GatedWebSocket.prototype, 'send', GatedWebSocket.prototype.send);
  freezeValue(
    GatedWebSocket.prototype,
    'close',
    GatedWebSocket.prototype.close,
  );
  const constants = ['CONNECTING', 'OPEN', 'CLOSING', 'CLOSED'];
  for (let index = 0; index < constants.length; index++) {
    freezeValue(GatedWebSocket, constants[index]!, index);
    freezeValue(GatedWebSocket.prototype, constants[index]!, index);
  }
  freezeValue(nativePrototype, 'constructor', GatedWebSocket);
  freezeValue(GatedWebSocket.prototype, 'constructor', GatedWebSocket);
  freezeValue(win, 'WebSocket', GatedWebSocket);
}
