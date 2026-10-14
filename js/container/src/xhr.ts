/* eslint-disable @typescript-eslint/no-explicit-any */

import { freezeCustom, freezeValue } from './freeze.js';
import type { NetworkAuthorization } from './network-transport.js';

interface RequestState {
  url: string;
  method: string;
  sameOrigin: boolean;
  supported: boolean;
  pending: boolean;
  nativeStarted: boolean;
  body: boolean;
  overrideState: 0 | 4 | undefined;
  startedAt: number;
  waited: number;
  timeout: number;
  deadline: ReturnType<typeof setTimeout> | undefined;
  cancel: (() => void) | undefined;
}

export function installXhrGate(
  win: typeof globalThis,
  authorize: NetworkAuthorization,
): void {
  const NativeXhr = win.XMLHttpRequest;
  if (!NativeXhr) return;
  const prototype = NativeXhr.prototype;
  const apply = Reflect.apply;
  const descriptor = Object.getOwnPropertyDescriptor;
  const nativeOpen = prototype.open;
  const nativeSend = prototype.send;
  const nativeAbort = prototype.abort;
  const nativeHeader = prototype.setRequestHeader;
  const nativeMime = prototype.overrideMimeType;
  const ready = descriptor(prototype, 'readyState')!.get!;
  const upload = descriptor(prototype, 'upload')!.get!;
  const timeout = descriptor(prototype, 'timeout')!;
  const credentials = descriptor(prototype, 'withCredentials')!;
  const responseType = descriptor(prototype, 'responseType')!;
  const dispatch = win.EventTarget.prototype.dispatchEvent;
  const NativeEvent = win.Event;
  const NativeProgress = win.ProgressEvent;
  const NativeError = win.DOMException;
  const NativeTypeError = win.TypeError;
  const NativeURL = win.URL;
  const href = descriptor(NativeURL.prototype, 'href')!.get!;
  const origin = descriptor(NativeURL.prototype, 'origin')!.get!;
  const protocol = descriptor(NativeURL.prototype, 'protocol')!.get!;
  const host = descriptor(NativeURL.prototype, 'host')!.get!;
  const baseURI = win.Node && descriptor(win.Node.prototype, 'baseURI')?.get;
  const uppercase = String.prototype.toUpperCase;
  const codeUnit = String.prototype.charCodeAt;
  const stringify = String;
  const states = new WeakMap<XMLHttpRequest, RequestState>();
  const weakGet = WeakMap.prototype.get;
  const weakSet = WeakMap.prototype.set;
  const now = win.performance.now.bind(win.performance);
  const schedule = win.setTimeout.bind(win);
  const unschedule = win.clearTimeout.bind(win);
  const maximum = Math.max;
  const NativeBytes = win.Uint8Array;
  const bufferLength = descriptor(
    win.ArrayBuffer.prototype,
    'byteLength',
  )!.get!;
  const viewPrototype = Object.getPrototypeOf(NativeBytes.prototype);
  const viewBuffer = descriptor(viewPrototype, 'buffer')!.get!;
  const viewOffset = descriptor(viewPrototype, 'byteOffset')!.get!;
  const viewLength = descriptor(viewPrototype, 'byteLength')!.get!;
  const dataBuffer = descriptor(win.DataView.prototype, 'buffer')!.get!;
  const dataOffset = descriptor(win.DataView.prototype, 'byteOffset')!.get!;
  const dataLength = descriptor(win.DataView.prototype, 'byteLength')!.get!;
  const isView = win.ArrayBuffer.isView;
  const blobSize = descriptor(win.Blob.prototype, 'size')!.get!;
  const NativeForm = win.FormData;
  const formEach = NativeForm.prototype.forEach;
  const formAppend = NativeForm.prototype.append;
  const NativeParams = win.URLSearchParams;
  const paramsString = NativeParams.prototype.toString;
  const nodeType = win.Node && descriptor(win.Node.prototype, 'nodeType')?.get;
  const cloneNode = win.Node?.prototype.cloneNode;

  function lockAccessor(
    name: string,
    get: (this: XMLHttpRequest) => any,
    set?: (this: XMLHttpRequest, value: any) => void,
  ): void {
    const marker = {};
    let verifying = true;
    freezeCustom(
      prototype,
      name,
      {
        get(this: XMLHttpRequest) {
          if (verifying && this === prototype) return marker;
          return apply(get, this, []);
        },
        set,
      },
      (value) => value === marker,
    );
    verifying = false;
  }

  function request(xhr: XMLHttpRequest): RequestState | undefined {
    apply(ready, xhr, []);
    return apply(weakGet, states, [xhr]);
  }

  function current(xhr: XMLHttpRequest, state: RequestState): boolean {
    return apply(weakGet, states, [xhr]) === state;
  }

  function invalid(): never {
    throw new NativeError(
      'The request is not open or has already been sent',
      'InvalidStateError',
    );
  }

  function byteString(value: any): string {
    if (typeof value === 'symbol')
      throw new NativeTypeError('Cannot convert a Symbol to a string');
    const text = stringify(value);
    for (let index = 0; index < text.length; index++) {
      if (apply(codeUnit, text, [index]) > 255)
        throw new NativeTypeError(
          'ByteString contains a character outside the byte range',
        );
    }
    return text;
  }

  function sendable(
    xhr: XMLHttpRequest,
    state: RequestState | undefined,
  ): asserts state is RequestState {
    if (
      !state ||
      !current(xhr, state) ||
      state.pending ||
      state.nativeStarted ||
      state.overrideState !== undefined ||
      apply(ready, xhr, []) !== 1
    )
      invalid();
  }

  function urlOrigin(url: URL): string {
    const value = apply(origin, url, []);
    return value === 'null'
      ? `${apply(protocol, url, [])}//${apply(host, url, [])}`
      : value;
  }
  const productOrigin = urlOrigin(new NativeURL(win.location.href));

  function cancel(state: RequestState): void {
    state.pending = false;
    state.cancel?.();
    state.cancel = undefined;
    if (state.deadline !== undefined) unschedule(state.deadline);
    state.deadline = undefined;
  }

  function fail(xhr: XMLHttpRequest, state: RequestState, type: string): void {
    if (!current(xhr, state) || !state.pending) return;
    cancel(state);
    apply(nativeAbort, xhr, []);
    state.overrideState = 4;
    apply(dispatch, xhr, [new NativeEvent('readystatechange')]);
    if (!current(xhr, state)) return;
    if (state.body) {
      const target = apply(upload, xhr, []);
      apply(dispatch, target, [new NativeProgress(type)]);
      if (!current(xhr, state)) return;
      apply(dispatch, target, [new NativeProgress('loadend')]);
      if (!current(xhr, state)) return;
    }
    apply(dispatch, xhr, [new NativeProgress(type)]);
    if (current(xhr, state))
      apply(dispatch, xhr, [new NativeProgress('loadend')]);
  }

  function deadline(xhr: XMLHttpRequest, state: RequestState): void {
    if (state.deadline !== undefined) unschedule(state.deadline);
    state.deadline = undefined;
    if (state.timeout !== 0) {
      state.deadline = schedule(
        () => fail(xhr, state, 'timeout'),
        maximum(0, state.timeout - (now() - state.startedAt)),
      );
    }
  }

  function copyBytes(
    buffer: ArrayBuffer,
    offset: number,
    length: number,
  ): Uint8Array {
    apply(bufferLength, buffer, []);
    const source = new NativeBytes(buffer, offset, length);
    const copy = new NativeBytes(length);
    for (let index = 0; index < length; index++) copy[index] = source[index]!;
    return copy;
  }

  function snapshot(body: any): any {
    if (body === undefined || body === null) return null;
    if (isView(body)) {
      try {
        return copyBytes(
          apply(viewBuffer, body, []),
          apply(viewOffset, body, []),
          apply(viewLength, body, []),
        );
      } catch {
        return copyBytes(
          apply(dataBuffer, body, []),
          apply(dataOffset, body, []),
          apply(dataLength, body, []),
        );
      }
    }
    let length: number | undefined;
    try {
      length = apply(bufferLength, body, []);
    } catch {
      /* another body type */
    }
    if (length !== undefined) return copyBytes(body, 0, length);
    try {
      apply(blobSize, body, []);
      return body;
    } catch {
      /* another body type */
    }
    if (nodeType && cloneNode) {
      let document = false;
      try {
        document = apply(nodeType, body, []) === 9;
      } catch {
        /* another body type */
      }
      if (document) return apply(cloneNode, body, [true]);
    }
    try {
      const copy = new NativeForm();
      apply(formEach, body, [
        (value: string | Blob, name: string) => {
          apply(formAppend, copy, [name, value]);
        },
      ]);
      return copy;
    } catch {
      /* another body type */
    }
    try {
      return new NativeParams(apply(paramsString, body, []));
    } catch {
      /* another body type */
    }
    if (typeof body === 'symbol')
      throw new NativeTypeError('Cannot convert a Symbol to a string');
    return stringify(body);
  }

  freezeValue(
    prototype,
    'open',
    function (
      this: XMLHttpRequest,
      method: string,
      url: string | URL,
      ...rest: any[]
    ) {
      request(this);
      method = byteString(method);
      if (typeof url === 'symbol')
        throw new NativeTypeError('Cannot convert a Symbol to a string');
      const inputUrl = stringify(url);
      function credential(index: number): string | null {
        const value = rest.length > index ? rest[index] : null;
        if (value === null || value === undefined) return null;
        if (typeof value === 'symbol')
          throw new NativeTypeError('Cannot convert a Symbol to a string');
        return stringify(value);
      }
      const username = credential(1);
      const password = credential(2);
      let destination: URL;
      try {
        destination = new NativeURL(
          inputUrl,
          baseURI ? apply(baseURI, win.document, []) : win.document.baseURI,
        );
      } catch {
        throw new NativeError('Invalid XMLHttpRequest URL', 'SyntaxError');
      }
      if (rest.length && !rest[0]) {
        throw new NativeError(
          'synchronous XMLHttpRequest is not supported',
          'InvalidAccessError',
        );
      }
      const previous = request(this);
      const oldTimeout = apply(timeout.get!, this, []);
      const oldState = apply(ready, this, []);
      const state: RequestState = {
        url: apply(href, destination, []),
        method: apply(uppercase, method, []),
        sameOrigin: urlOrigin(destination) === productOrigin,
        supported:
          apply(protocol, destination, []) === 'http:' ||
          apply(protocol, destination, []) === 'https:',
        pending: false,
        nativeStarted: false,
        body: false,
        overrideState: undefined,
        startedAt: 0,
        waited: 0,
        timeout: previous?.timeout ?? oldTimeout,
        deadline: undefined,
        cancel: undefined,
      };
      apply(weakSet, states, [this, state]);
      try {
        apply(timeout.set!, this, [state.timeout]);
        apply(nativeOpen, this, [method, state.url, true, username, password]);
      } catch (error) {
        apply(weakSet, states, [this, previous]);
        apply(timeout.set!, this, [oldTimeout]);
        throw error;
      }
      if (previous) cancel(previous);
      if (
        current(this, state) &&
        oldState === 1 &&
        previous?.overrideState !== undefined
      )
        apply(dispatch, this, [new NativeEvent('readystatechange')]);
    },
  );

  freezeValue(prototype, 'send', function (this: XMLHttpRequest, body?: any) {
    const state = request(this);
    sendable(this, state);
    const payload =
      state.method === 'GET' || state.method === 'HEAD' ? null : snapshot(body);
    sendable(this, state);
    state.pending = true;
    state.body = payload !== null;
    state.startedAt = now();
    deadline(this, state);
    let sending = true;
    const failSend = (type: string) => {
      if (sending) schedule(() => fail(this, state, type), 0);
      else fail(this, state, type);
    };
    const decided = (allowed: boolean) => {
      if (!current(this, state) || !state.pending) return;
      if (allowed !== true) return failSend('error');
      state.waited = now() - state.startedAt;
      if (state.timeout && state.waited >= state.timeout)
        return failSend('timeout');
      cancel(state);
      state.nativeStarted = true;
      apply(timeout.set!, this, [
        state.timeout ? maximum(1, state.timeout - state.waited) : 0,
      ]);
      try {
        apply(nativeSend, this, [payload]);
      } catch {
        state.pending = true;
        state.nativeStarted = false;
        failSend('error');
      }
    };
    if (state.sameOrigin) decided(true);
    else if (!state.supported) decided(false);
    else {
      try {
        const cancellation = authorize(state.url, decided);
        if (!state.pending) cancellation();
        else state.cancel = cancellation;
      } catch {
        failSend('error');
      }
    }
    sending = false;
  });

  freezeValue(prototype, 'abort', function (this: XMLHttpRequest) {
    const state = request(this);
    if (state?.pending) {
      fail(this, state, 'abort');
      if (current(this, state) && state.overrideState === 4)
        state.overrideState = 0;
    } else {
      if (state?.overrideState !== undefined) state.overrideState = 0;
      apply(nativeAbort, this, []);
    }
  });

  freezeValue(
    prototype,
    'setRequestHeader',
    function (this: XMLHttpRequest, ...args: any[]) {
      if (args.length < 2)
        throw new NativeTypeError('setRequestHeader requires two arguments');
      const name = byteString(args[0]);
      const value = byteString(args[1]);
      const state = request(this);
      if (state?.pending || state?.overrideState !== undefined) invalid();
      return apply(nativeHeader, this, [name, value]);
    },
  );
  freezeValue(
    prototype,
    'overrideMimeType',
    function (this: XMLHttpRequest, ...args: any[]) {
      if (request(this)?.overrideState === 4) invalid();
      return apply(nativeMime, this, args);
    },
  );

  lockAccessor('readyState', function (this: XMLHttpRequest) {
    const state = request(this);
    return state?.overrideState ?? apply(ready, this, []);
  });
  lockAccessor(
    'timeout',
    function (this: XMLHttpRequest) {
      return request(this)?.timeout ?? apply(timeout.get!, this, []);
    },
    function (this: XMLHttpRequest, value: number) {
      apply(timeout.set!, this, [value]);
      const state = request(this);
      if (!state) return;
      state.timeout = apply(timeout.get!, this, []);
      if (state.pending) deadline(this, state);
      else if (state.nativeStarted && apply(ready, this, []) !== 4)
        apply(timeout.set!, this, [
          state.timeout ? maximum(1, state.timeout - state.waited) : 0,
        ]);
    },
  );
  lockAccessor(
    'withCredentials',
    credentials.get!,
    function (this: XMLHttpRequest, value: boolean) {
      const state = request(this);
      if (state?.pending || state?.overrideState === 4) invalid();
      apply(credentials.set!, this, [value]);
    },
  );
  lockAccessor(
    'responseType',
    responseType.get!,
    function (this: XMLHttpRequest, value: XMLHttpRequestResponseType) {
      if (request(this)?.overrideState === 4) invalid();
      apply(responseType.set!, this, [value]);
    },
  );
  freezeValue(prototype, 'constructor', NativeXhr);
  freezeValue(win, 'XMLHttpRequest', NativeXhr);
}
