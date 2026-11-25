/* eslint-disable @typescript-eslint/consistent-type-assertions -- Ported verbatim from
   @novasamatech/host-worker-sandbox. Every assertion here narrows a value that has just
   crossed the QuickJS boundary, after a runtime check the compiler cannot follow
   (`typeof v === 'object'`, `ArrayBuffer.isView(v)`). Rewriting the marshalling to satisfy
   the rule would mean re-deriving crypto and fetch bridging that arrived with no tests, so
   the port keeps it as-is; tighten it when this file grows its own coverage. */
import { nanoid } from 'nanoid';
import { type QuickJSContext, type QuickJSHandle } from 'quickjs-emscripten';

import { extractBytesFromVm, sendBytesToVm, withProp } from '../buffers';

export type FetchRequest = {
  url: string;
  method: string;
  headers: [string, string][];
  body: Uint8Array | null;
  signal: AbortSignal;
  /** Forwarded from the in-VM Request — resolver may apply CORS / auth policy. */
  mode: 'cors' | 'no-cors' | 'same-origin' | 'navigate' | string;
  /** Forwarded from the in-VM Request — `'omit'` means "do not attach credentials". */
  credentials: 'omit' | 'include' | 'same-origin' | string;
  /** Forwarded from the in-VM Request — `'manual'` skips redirect-following. */
  redirect: 'follow' | 'error' | 'manual' | string;
};

export type FetchResponse = {
  status: number;
  statusText?: string;
  headers: [string, string][];
  body: Uint8Array;
  url?: string;
  redirected?: boolean;
};

export type FetchResolver = (req: FetchRequest) => Promise<FetchResponse>;

export type FetchOptions = {
  /** Maximum request body size in bytes. Defaults to 100 MiB. */
  maxBodyBytes?: number;
};

const DEFAULT_MAX_BODY_BYTES = 100 * 1024 * 1024;

const BRIDGE_NAME = `__FETCH_BRIDGE_${nanoid()}__`;

// ─── In-VM source: Headers, Request, Response, fetch ─────────────────────────
//
// Depends on globals already injected by the sandbox: TextEncoder, TextDecoder,
// AbortController, AbortSignal, DOMException, Blob, FormData.
//
// Calls back to the host through a bridge function passed via globalThis at
// injection time, captured into a local closure, then deleted from globalThis
// so sandbox code can't reach it directly. Bridge signature:
//   (method, url, headers, bodyBytes, mode, credentials, redirect, registerAbort) => Promise<{...}>
// where registerAbort is a function the host calls with a callback that fires
// when the VM-side AbortSignal aborts.
//
// Reference: https://fetch.spec.whatwg.org/
const SOURCE = `(() => {
  const __bridge = globalThis['${BRIDGE_NAME}'];
  delete globalThis['${BRIDGE_NAME}'];

  // Capture Blob's internal bytes accessor into a closure local. Sandbox.ts
  // deletes Blob.__getBytes after this IIFE runs, so sandbox code can't reach
  // it; the multipart encoder and body extraction use the captured reference.
  const __getBlobBytes = Blob.__getBytes;

  const utf8Encoder = new TextEncoder();
  const utf8Decoder = new TextDecoder();

  // ───────── Headers ─────────
  const HEADER_NAME_RE = /^[!#$%&'*+\\-.^_\`|~0-9A-Za-z]+$/;
  function isValidHeaderName(name) { return typeof name === 'string' && HEADER_NAME_RE.test(name); }
  function isValidHeaderValue(value) { return typeof value === 'string' && !/[\\0\\r\\n]/.test(value); }
  function normalizeValue(v) { return String(v).replace(/^[\\t ]+|[\\t ]+$/g, ''); }

  class Headers {
    constructor(init) {
      Object.defineProperty(this, '_map', { value: new Map(), enumerable: false });
      if (init == null) return;
      if (init instanceof Headers) {
        for (const [k, v] of init.entries()) this.append(k, v);
      } else if (Array.isArray(init)) {
        for (const pair of init) {
          if (!Array.isArray(pair) || pair.length !== 2) throw new TypeError('Headers init pair must be a 2-tuple');
          this.append(pair[0], pair[1]);
        }
      } else if (typeof init === 'object') {
        for (const k of Object.keys(init)) this.append(k, init[k]);
      }
    }
    append(name, value) {
      if (!isValidHeaderName(name)) throw new TypeError('Invalid header name');
      const v = normalizeValue(value);
      if (!isValidHeaderValue(v)) throw new TypeError('Invalid header value');
      const key = name.toLowerCase();
      const e = this._map.get(key);
      if (e) e.values.push(v);
      else this._map.set(key, { name, values: [v] });
    }
    delete(name) {
      if (!isValidHeaderName(name)) throw new TypeError('Invalid header name');
      this._map.delete(name.toLowerCase());
    }
    get(name) {
      if (!isValidHeaderName(name)) throw new TypeError('Invalid header name');
      const e = this._map.get(name.toLowerCase());
      return e ? e.values.join(', ') : null;
    }
    has(name) {
      if (!isValidHeaderName(name)) throw new TypeError('Invalid header name');
      return this._map.has(name.toLowerCase());
    }
    set(name, value) {
      if (!isValidHeaderName(name)) throw new TypeError('Invalid header name');
      const v = normalizeValue(value);
      if (!isValidHeaderValue(v)) throw new TypeError('Invalid header value');
      this._map.set(name.toLowerCase(), { name, values: [v] });
    }
    getSetCookie() {
      const e = this._map.get('set-cookie');
      return e ? [...e.values] : [];
    }
    *entries() {
      const sorted = [...this._map.keys()].sort();
      for (const key of sorted) {
        const e = this._map.get(key);
        if (key === 'set-cookie') for (const v of e.values) yield [key, v];
        else yield [key, e.values.join(', ')];
      }
    }
    *keys() { for (const [k] of this.entries()) yield k; }
    *values() { for (const [, v] of this.entries()) yield v; }
    forEach(cb, thisArg) { for (const [k, v] of this.entries()) cb.call(thisArg, v, k, this); }
  }
  Headers.prototype[Symbol.iterator] = Headers.prototype.entries;
  Object.defineProperty(Headers.prototype, Symbol.toStringTag, { value: 'Headers', configurable: true });

  function headersToArray(h) {
    const out = [];
    for (const [k, v] of h.entries()) out.push([k, v]);
    return out;
  }

  // ───────── Body extraction ─────────
  function encodeMultipart(formData, boundary) {
    const parts = [];
    for (const [name, value] of formData.entries()) {
      const escapedName = name.replace(/"/g, '%22').replace(/\\r\\n|\\r|\\n/g, '\\r\\n');
      let head, bodyBytes;
      if (value instanceof Blob) {
        head =
          '--' + boundary + '\\r\\n' +
          'Content-Disposition: form-data; name="' + escapedName + '"; filename="blob"\\r\\n' +
          'Content-Type: ' + (value.type || 'application/octet-stream') + '\\r\\n\\r\\n';
        bodyBytes = __getBlobBytes(value);
      } else {
        head =
          '--' + boundary + '\\r\\n' +
          'Content-Disposition: form-data; name="' + escapedName + '"\\r\\n\\r\\n';
        bodyBytes = utf8Encoder.encode(String(value));
      }
      parts.push(utf8Encoder.encode(head));
      parts.push(bodyBytes);
      parts.push(utf8Encoder.encode('\\r\\n'));
    }
    parts.push(utf8Encoder.encode('--' + boundary + '--\\r\\n'));
    let total = 0;
    for (const p of parts) total += p.byteLength;
    const out = new Uint8Array(total);
    let off = 0;
    for (const p of parts) { out.set(p, off); off += p.byteLength; }
    return out;
  }

  function extractBody(init) {
    if (init == null) return { bytes: null, contentType: null };
    if (typeof init === 'string') {
      return { bytes: utf8Encoder.encode(init), contentType: 'text/plain;charset=UTF-8' };
    }
    if (init instanceof Blob) {
      return { bytes: __getBlobBytes(init).slice(), contentType: init.type || null };
    }
    if (init instanceof FormData) {
      const boundary = '----formdata-host-worker-' + Math.random().toString(36).slice(2);
      return { bytes: encodeMultipart(init, boundary), contentType: 'multipart/form-data; boundary=' + boundary };
    }
    if (init instanceof ArrayBuffer) {
      return { bytes: new Uint8Array(init).slice(), contentType: null };
    }
    if (ArrayBuffer.isView(init)) {
      return { bytes: new Uint8Array(init.buffer, init.byteOffset, init.byteLength).slice(), contentType: null };
    }
    throw new TypeError('Unsupported body init type');
  }

  // ───────── Body mixin ─────────
  const _bodyState = new WeakMap();
  function setBody(target, bytes) { _bodyState.set(target, { bytes, used: false }); }

  function bodyMixin(klass) {
    klass.prototype.arrayBuffer = function() {
      const s = _bodyState.get(this);
      if (!s) return Promise.resolve(new ArrayBuffer(0));
      if (s.used) return Promise.reject(new TypeError('Body already used'));
      s.used = true;
      return Promise.resolve(s.bytes.slice().buffer);
    };
    klass.prototype.bytes = function() {
      const s = _bodyState.get(this);
      if (!s) return Promise.resolve(new Uint8Array(0));
      if (s.used) return Promise.reject(new TypeError('Body already used'));
      s.used = true;
      return Promise.resolve(s.bytes.slice());
    };
    klass.prototype.text = function() {
      const s = _bodyState.get(this);
      if (!s) return Promise.resolve('');
      if (s.used) return Promise.reject(new TypeError('Body already used'));
      s.used = true;
      return Promise.resolve(utf8Decoder.decode(s.bytes));
    };
    klass.prototype.json = function() { return this.text().then(t => JSON.parse(t)); };
    klass.prototype.blob = function() {
      const s = _bodyState.get(this);
      if (!s) return Promise.resolve(new Blob([], { type: '' }));
      if (s.used) return Promise.reject(new TypeError('Body already used'));
      s.used = true;
      const ct = (this.headers && this.headers.get('content-type')) || '';
      return Promise.resolve(new Blob([s.bytes], { type: ct }));
    };
    klass.prototype.formData = function() {
      return Promise.reject(new TypeError('formData() body parsing is not supported in this sandbox'));
    };
    Object.defineProperty(klass.prototype, 'bodyUsed', {
      get() { const s = _bodyState.get(this); return s ? s.used : false; },
    });
  }

  // ───────── Request ─────────
  class Request {
    constructor(input, init = {}) {
      let url;
      let baseInit = {};
      if (input instanceof Request) {
        url = input.url;
        baseInit = {
          method: input.method,
          headers: new Headers(input.headers),
          signal: input.signal,
          mode: input.mode,
          credentials: input.credentials,
          cache: input.cache,
          redirect: input.redirect,
          referrer: input.referrer,
          referrerPolicy: input.referrerPolicy,
          integrity: input.integrity,
          keepalive: input.keepalive,
        };
      } else {
        url = String(input);
      }

      const merged = Object.assign({}, baseInit, init);
      const method = String(merged.method || 'GET').toUpperCase();
      if (method === 'CONNECT' || method === 'TRACE' || method === 'TRACK') {
        throw new TypeError('Forbidden method: ' + method);
      }

      const headers = new Headers(merged.headers);

      let bodyInfo = null;
      if (merged.body != null) {
        if (method === 'GET' || method === 'HEAD') {
          throw new TypeError('Cannot include body with GET or HEAD');
        }
        bodyInfo = extractBody(merged.body);
        if (bodyInfo.contentType && !headers.has('content-type')) {
          headers.set('content-type', bodyInfo.contentType);
        }
      }

      const signal = merged.signal instanceof AbortSignal ? merged.signal : new AbortController().signal;

      Object.defineProperty(this, 'url', { value: url, enumerable: true });
      Object.defineProperty(this, 'method', { value: method, enumerable: true });
      Object.defineProperty(this, 'headers', { value: headers, enumerable: true });
      Object.defineProperty(this, 'mode', { value: merged.mode || 'cors', enumerable: true });
      Object.defineProperty(this, 'credentials', { value: merged.credentials || 'same-origin', enumerable: true });
      Object.defineProperty(this, 'cache', { value: merged.cache || 'default', enumerable: true });
      Object.defineProperty(this, 'redirect', { value: merged.redirect || 'follow', enumerable: true });
      Object.defineProperty(this, 'referrer', { value: merged.referrer == null ? 'about:client' : String(merged.referrer), enumerable: true });
      Object.defineProperty(this, 'referrerPolicy', { value: merged.referrerPolicy || '', enumerable: true });
      Object.defineProperty(this, 'integrity', { value: String(merged.integrity || ''), enumerable: true });
      Object.defineProperty(this, 'keepalive', { value: !!merged.keepalive, enumerable: true });
      Object.defineProperty(this, 'signal', { value: signal, enumerable: true });

      if (bodyInfo && bodyInfo.bytes) setBody(this, bodyInfo.bytes);
    }
    clone() {
      const s = _bodyState.get(this);
      if (s && s.used) throw new TypeError('Cannot clone used body');
      const cloned = new Request(this.url, {
        method: this.method,
        headers: headersToArray(this.headers),
        signal: this.signal,
        mode: this.mode,
        credentials: this.credentials,
        cache: this.cache,
        redirect: this.redirect,
        referrer: this.referrer,
        referrerPolicy: this.referrerPolicy,
        integrity: this.integrity,
        keepalive: this.keepalive,
      });
      if (s) setBody(cloned, s.bytes.slice());
      return cloned;
    }
  }
  bodyMixin(Request);
  Object.defineProperty(Request.prototype, Symbol.toStringTag, { value: 'Request', configurable: true });

  // ───────── Response ─────────
  class Response {
    constructor(body = null, init = {}) {
      const status = init.status == null ? 200 : Number(init.status);
      if (status < 200 || status > 599) throw new RangeError('Status out of range');
      const statusText = init.statusText == null ? '' : String(init.statusText);
      const headers = new Headers(init.headers);

      let bodyBytes = null, contentType = null;
      if (body != null) {
        const info = extractBody(body);
        bodyBytes = info.bytes;
        contentType = info.contentType;
        if (contentType && !headers.has('content-type')) headers.set('content-type', contentType);
      }

      Object.defineProperty(this, 'status', { value: status, enumerable: true });
      Object.defineProperty(this, 'statusText', { value: statusText, enumerable: true });
      Object.defineProperty(this, 'headers', { value: headers, enumerable: true });
      Object.defineProperty(this, 'ok', { value: status >= 200 && status < 300, enumerable: true });
      Object.defineProperty(this, 'redirected', { value: !!init.__redirected, enumerable: true });
      Object.defineProperty(this, 'url', { value: String(init.__url || ''), enumerable: true });
      Object.defineProperty(this, 'type', { value: String(init.type || 'default'), enumerable: true });

      if (bodyBytes) setBody(this, bodyBytes);
    }
    clone() {
      const s = _bodyState.get(this);
      if (s && s.used) throw new TypeError('Cannot clone used body');
      return new Response(s ? s.bytes.slice() : null, {
        status: this.status,
        statusText: this.statusText,
        headers: headersToArray(this.headers),
        __url: this.url,
        __redirected: this.redirected,
        type: this.type,
      });
    }
    static error() { return new Response(null, { status: 200, type: 'error' }); }
    static redirect(url, status = 302) {
      if (!(status === 301 || status === 302 || status === 303 || status === 307 || status === 308)) {
        throw new RangeError('Invalid redirect status');
      }
      return new Response(null, { status, headers: { Location: String(url) } });
    }
    static json(data, init = {}) {
      const text = JSON.stringify(data);
      const headers = new Headers(init.headers);
      if (!headers.has('content-type')) headers.set('content-type', 'application/json');
      return new Response(text, Object.assign({}, init, { headers: headersToArray(headers) }));
    }
  }
  bodyMixin(Response);
  Object.defineProperty(Response.prototype, Symbol.toStringTag, { value: 'Response', configurable: true });

  // ───────── fetch ─────────
  globalThis.Headers = Headers;
  globalThis.Request = Request;
  globalThis.Response = Response;

  globalThis.fetch = async function fetch(input, init) {
    const request = (input instanceof Request && init === undefined) ? input : new Request(input, init);

    if (request.signal && request.signal.aborted) {
      throw request.signal.reason || new DOMException('The operation was aborted', 'AbortError');
    }

    const headersArr = headersToArray(request.headers);
    const bodyState = _bodyState.get(request);
    if (bodyState && bodyState.used) {
      throw new TypeError('Request body has already been consumed');
    }
    const bodyBytes = bodyState ? bodyState.bytes : new Uint8Array(0);
    // Spec: fetch reads (and thereby consumes) the request body.
    if (bodyState) bodyState.used = true;

    const result = await __bridge(
      request.method,
      request.url,
      headersArr,
      bodyBytes,
      request.mode,
      request.credentials,
      request.redirect,
      function registerAbort(cb) {
        if (request.signal.aborted) cb();
        else request.signal.addEventListener('abort', () => cb(), { once: true });
      },
    );

    return new Response(result.body, {
      status: result.status,
      statusText: result.statusText,
      headers: result.headers,
      __url: result.url || request.url,
      __redirected: !!result.redirected,
    });
  };
})();`;

// ─── Host bridge ──────────────────────────────────────────────────────────────

function buildResponseHandle(vm: QuickJSContext, toUint8ArrayFn: QuickJSHandle, resp: FetchResponse): QuickJSHandle {
  const obj = vm.newObject();

  const statusH = vm.newNumber(resp.status);
  vm.setProp(obj, 'status', statusH);
  statusH.dispose();

  const stH = vm.newString(resp.statusText ?? '');
  vm.setProp(obj, 'statusText', stH);
  stH.dispose();

  const headersArr = vm.newArray();
  for (let i = 0; i < resp.headers.length; i++) {
    const entry = resp.headers[i];
    if (!entry) continue;
    const [k, v] = entry;
    const pair = vm.newArray();
    const kH = vm.newString(k);
    const vH = vm.newString(v);
    vm.setProp(pair, 0, kH);
    vm.setProp(pair, 1, vH);
    kH.dispose();
    vH.dispose();
    vm.setProp(headersArr, i, pair);
    pair.dispose();
  }
  vm.setProp(obj, 'headers', headersArr);
  headersArr.dispose();

  let bodyHandle: QuickJSHandle | undefined;
  try {
    bodyHandle = sendBytesToVm(vm, toUint8ArrayFn, resp.body);
    vm.setProp(obj, 'body', bodyHandle);
  } catch (e) {
    console.error('[Sandbox] fetch: failed to send response body to VM', e);
  } finally {
    bodyHandle?.dispose();
  }

  if (resp.url) {
    const urlH = vm.newString(resp.url);
    vm.setProp(obj, 'url', urlH);
    urlH.dispose();
  }
  vm.setProp(obj, 'redirected', resp.redirected ? vm.true : vm.false);

  return obj;
}

export function injectFetch(
  vm: QuickJSContext,
  toUint8ArrayFn: QuickJSHandle,
  resolver: FetchResolver,
  options: FetchOptions = {},
): VoidFunction {
  let disposed = false;
  const maxBodyBytes = options.maxBodyBytes ?? DEFAULT_MAX_BODY_BYTES;
  // Track host-side abort callbacks so we can null them out at dispose.
  const inflightControllers = new Set<AbortController>();

  const bridge = vm.newFunction(BRIDGE_NAME, (methodH, urlH, headersH, bodyH, modeH, credentialsH, redirectH, abortRegH) => {
    const method = vm.getString(methodH);
    const url = vm.getString(urlH);
    const headers = (vm.dump(headersH) ?? []) as [string, string][];
    const mode = vm.getString(modeH);
    const credentials = vm.getString(credentialsH);
    const redirect = vm.getString(redirectH);

    // Check size before extracting — the cap exists to bound host allocation.
    const bodyByteLength = withProp(vm, bodyH, 'byteLength', h => vm.getNumber(h));
    if (bodyByteLength > maxBodyBytes) {
      const deferred = vm.newPromise();
      const errHandle = vm.newError(`fetch body exceeds maximum allowed size: ${bodyByteLength} > ${maxBodyBytes}`);
      deferred.reject(errHandle);
      errHandle.dispose();
      if (!disposed) vm.runtime.executePendingJobs(-1);
      return deferred.handle;
    }

    const bodyBytes = extractBytesFromVm(vm, bodyH);
    const body = bodyBytes.byteLength === 0 ? null : bodyBytes;

    const hostAC = new AbortController();
    inflightControllers.add(hostAC);

    const abortCb = vm.newFunction('__abortCb', () => {
      hostAC.abort();
    });
    const regRes = vm.callFunction(abortRegH, vm.undefined, abortCb);
    abortCb.dispose();
    if (regRes.error) regRes.error.dispose();
    else regRes.value.dispose();

    const deferred = vm.newPromise();

    Promise.resolve()
      .then(() => resolver({ url, method, headers, body, signal: hostAC.signal, mode, credentials, redirect }))
      .then(resp => {
        inflightControllers.delete(hostAC);
        if (disposed) return;
        const respHandle = buildResponseHandle(vm, toUint8ArrayFn, resp);
        deferred.resolve(respHandle);
        respHandle.dispose();
      })
      .catch(err => {
        inflightControllers.delete(hostAC);
        if (disposed) return;
        const message = err instanceof Error ? err.message : String(err);
        const errHandle = vm.newError(message);
        // Preserve resolver-supplied error name (`TypeError` for network errors,
        // `AbortError` for aborts, etc.) so sandbox code can branch on it.
        if (err instanceof Error && err.name && err.name !== 'Error') {
          const nameHandle = vm.newString(err.name);
          vm.setProp(errHandle, 'name', nameHandle);
          nameHandle.dispose();
        }
        deferred.reject(errHandle);
        errHandle.dispose();
      })
      .finally(() => {
        if (!disposed) vm.runtime.executePendingJobs(-1);
      });

    return deferred.handle;
  });

  vm.setProp(vm.global, BRIDGE_NAME, bridge);
  bridge.dispose();

  const result = vm.evalCode(SOURCE);
  if (result.error) {
    const msg = vm.dump(result.error);
    result.error.dispose();
    throw new Error(`Failed to inject fetch: ${JSON.stringify(msg)}`);
  }
  result.value.dispose();

  return () => {
    disposed = true;
    for (const ac of inflightControllers) ac.abort();
    inflightControllers.clear();
  };
}
