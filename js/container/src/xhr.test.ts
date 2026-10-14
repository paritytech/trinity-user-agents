import { describe, expect, it } from 'bun:test';
import { installXhrGate } from './xhr.js';
import { reportLockdownFailures, resetLockdownFailures } from './freeze.js';

/* eslint-disable @typescript-eslint/no-explicit-any */

function realm() {
  const calls: any[] = [];
  let clock = 0;
  let sequence = 0;
  const timers = new Map<number, { at: number; callback: () => void }>();
  class Progress extends Event {
    lengthComputable = false;
    loaded = 0;
    total = 0;
  }
  class Xhr extends EventTarget {
    static UNSENT = 0;
    static OPENED = 1;
    static DONE = 4;
    #state = 0;
    #sent = false;
    #timeout = 0;
    #credentials = false;
    #responseType = '';
    #status = 0;
    #response: any = '';
    method = '';
    url = '';
    headers: Record<string, string> = {};
    #upload = new EventTarget();
    get upload() {
      return this.#upload;
    }
    get readyState() {
      return this.#state;
    }
    get status() {
      return this.#status;
    }
    get statusText() {
      return this.#status ? 'OK' : '';
    }
    get response() {
      return this.#responseType && !this.#status ? null : this.#response;
    }
    get responseText() {
      return this.#response;
    }
    get responseURL() {
      return this.#status ? this.url : '';
    }
    get timeout() {
      return this.#timeout;
    }
    set timeout(value: number) {
      this.#timeout = Number(value) >>> 0;
    }
    get withCredentials() {
      return this.#credentials;
    }
    set withCredentials(value: boolean) {
      if (this.#sent || this.#state > 1)
        throw new DOMException('', 'InvalidStateError');
      this.#credentials = !!value;
    }
    get responseType() {
      return this.#responseType;
    }
    set responseType(value: string) {
      if (this.#state >= 3) throw new DOMException('', 'InvalidStateError');
      this.#responseType = String(value);
    }
    open(
      method: string,
      url: string,
      _async = true,
      username?: string,
      password?: string,
    ) {
      method = String(method);
      username = username == null ? undefined : String(username);
      password = password == null ? undefined : String(password);
      if (!/^[A-Za-z]+$/.test(method))
        throw new DOMException('', 'SyntaxError');
      this.method = method.toUpperCase();
      this.url = String(url);
      this.headers = {};
      this.#sent = false;
      this.#status = 0;
      this.#response = '';
      calls.push({
        kind: 'open',
        method: this.method,
        url: this.url,
        username,
        password,
      });
      if (this.#state !== 1) {
        this.#state = 1;
        this.dispatchEvent(new Event('readystatechange'));
      }
    }
    setRequestHeader(name: string, value: string) {
      name = String(name);
      value = String(value);
      if (this.#state !== 1 || this.#sent)
        throw new DOMException('', 'InvalidStateError');
      this.headers[name] = value;
    }
    overrideMimeType(_value: string) {
      if (this.#state >= 3) throw new DOMException('', 'InvalidStateError');
    }
    send(body: any = null) {
      if (this.#state !== 1 || this.#sent)
        throw new DOMException('', 'InvalidStateError');
      this.#sent = true;
      calls.push({
        kind: 'send',
        url: this.url,
        method: this.method,
        body,
        headers: { ...this.headers },
        credentials: this.#credentials,
        timeout: this.#timeout,
        responseType: this.#responseType,
      });
      this.dispatchEvent(new Progress('loadstart'));
    }
    abort() {
      if (this.#sent) {
        this.#sent = false;
        this.#state = 4;
        this.dispatchEvent(new Event('readystatechange'));
        this.dispatchEvent(new Progress('abort'));
        this.dispatchEvent(new Progress('loadend'));
      }
      if (this.#state === 4) this.#state = 0;
    }
    getResponseHeader(name: string) {
      return this.#status && name === 'X-Test' ? 'native' : null;
    }
    getAllResponseHeaders() {
      return this.#status ? 'x-test: native\r\n' : '';
    }
    complete(response: any) {
      this.#sent = false;
      this.#state = 4;
      this.#status = 200;
      this.#response = response;
      this.dispatchEvent(new Event('readystatechange'));
      this.dispatchEvent(new Progress('load'));
      this.dispatchEvent(new Progress('loadend'));
    }
  }
  const win = {
    XMLHttpRequest: Xhr,
    Event,
    EventTarget,
    ProgressEvent: Progress,
    DOMException,
    TypeError,
    URL,
    ArrayBuffer,
    Uint8Array,
    DataView,
    Blob,
    FormData,
    URLSearchParams,
    location: { href: 'https://product.example/app/index.html' },
    document: { baseURI: 'https://product.example/app/index.html' },
    performance: { now: () => clock },
    setTimeout(callback: () => void, delay: number) {
      const id = ++sequence;
      timers.set(id, { at: clock + delay, callback });
      return id;
    },
    clearTimeout(id: number) {
      timers.delete(id);
    },
  };
  return {
    win,
    calls,
    Xhr,
    advance(milliseconds: number) {
      clock += milliseconds;
      for (const [id, timer] of [...timers]) {
        if (timer.at <= clock) {
          timers.delete(id);
          timer.callback();
        }
      }
    },
  };
}

function gated() {
  const fixture = realm();
  resetLockdownFailures();
  const requests: {
    url: string;
    decide: (allowed: boolean) => void;
    cancelled: boolean;
  }[] = [];
  installXhrGate(fixture.win as any, (url, decide) => {
    const request = { url, decide, cancelled: false };
    requests.push(request);
    return () => {
      request.cancelled = true;
    };
  });
  reportLockdownFailures();
  return {
    ...fixture,
    requests,
    sends: () => fixture.calls.filter((call) => call.kind === 'send'),
  };
}

function events(xhr: any) {
  const result: [string, number, number][] = [];
  for (const type of [
    'readystatechange',
    'loadstart',
    'load',
    'error',
    'abort',
    'timeout',
    'loadend',
  ])
    xhr.addEventListener(type, () =>
      result.push([type, xhr.readyState, xhr.status]),
    );
  return result;
}

describe('XHR permission gating', () => {
  it('keeps open synchronous and preserves native request and response handling after one approval', () => {
    const { win, requests, sends } = gated();
    const xhr = new win.XMLHttpRequest();
    const observed = events(xhr);
    xhr.open('POST', 'https://api.example/data', true, 'user', 'password');
    xhr.setRequestHeader('X-Test', 'value');
    xhr.withCredentials = true;
    xhr.responseType = 'json';
    xhr.send('body');
    expect([
      xhr.readyState,
      requests.map((request) => request.url),
      sends(),
    ]).toEqual([1, ['https://api.example/data'], []]);
    requests[0]!.decide(true);
    const response = { ok: true };
    xhr.complete(response);
    expect(sends()).toEqual([
      {
        kind: 'send',
        url: 'https://api.example/data',
        method: 'POST',
        body: 'body',
        headers: { 'X-Test': 'value' },
        credentials: true,
        timeout: 0,
        responseType: 'json',
      },
    ]);
    expect(xhr.response).toBe(response);
    expect([
      xhr.getResponseHeader('X-Test'),
      xhr.getAllResponseHeaders(),
      xhr.responseURL,
    ]).toEqual(['native', 'x-test: native\r\n', 'https://api.example/data']);
    expect(observed).toEqual([
      ['readystatechange', 1, 0],
      ['loadstart', 1, 0],
      ['readystatechange', 4, 200],
      ['load', 4, 200],
      ['loadend', 4, 200],
    ]);
  });

  it('denies before sending and reports the terminal state normal XHR clients expect', () => {
    const { win, requests, sends } = gated();
    const xhr = new win.XMLHttpRequest();
    xhr.open('GET', 'https://api.example/data');
    const observed = events(xhr);
    xhr.send();
    requests[0]!.decide(false);
    requests[0]!.decide(true);
    expect({
      sends: sends(),
      state: xhr.readyState,
      status: xhr.status,
      response: xhr.response,
      events: observed,
    }).toEqual({
      sends: [],
      state: 4,
      status: 0,
      response: '',
      events: [
        ['readystatechange', 4, 0],
        ['error', 4, 0],
        ['loadend', 4, 0],
      ],
    });
    expect(() => xhr.send()).toThrow();
    expect(() => xhr.setRequestHeader('X-Test', 'late')).toThrow();
  });

  it('authorizes each send independently while same-product requests need no grant', () => {
    const { win, requests, sends } = gated();
    const xhr = new win.XMLHttpRequest();
    xhr.open('GET', './asset');
    xhr.send();
    expect([requests.length, sends().map((call) => call.url)]).toEqual([
      0,
      ['https://product.example/app/asset'],
    ]);
    xhr.open('GET', 'https://api.example/first');
    xhr.send();
    requests[0]!.decide(true);
    xhr.open('GET', 'https://api.example/second');
    xhr.send();
    requests[1]!.decide(false);
    expect(sends().map((call) => call.url)).toEqual([
      'https://product.example/app/asset',
      'https://api.example/first',
    ]);
  });

  it('aborts pending consent without later sending and can be reopened normally', () => {
    const { win, requests, sends } = gated();
    const xhr = new win.XMLHttpRequest();
    xhr.open('GET', 'https://api.example/old');
    const observed = events(xhr);
    xhr.send();
    xhr.abort();
    requests[0]!.decide(true);
    expect([xhr.readyState, requests[0]!.cancelled, sends(), observed]).toEqual(
      [
        0,
        true,
        [],
        [
          ['readystatechange', 4, 0],
          ['abort', 4, 0],
          ['loadend', 4, 0],
        ],
      ],
    );
    xhr.open('GET', 'https://api.example/new');
    expect(observed[observed.length - 1]).toEqual(['readystatechange', 1, 0]);
    xhr.send();
    requests[1]!.decide(true);
    expect(sends().map((call) => call.url)).toEqual([
      'https://api.example/new',
    ]);
  });

  it('reopening cancels the old request and reentrant error handlers cannot revive it', () => {
    const { win, requests, sends } = gated();
    const xhr = new win.XMLHttpRequest();
    xhr.open('GET', 'https://api.example/old');
    xhr.send();
    xhr.open('GET', 'https://api.example/new');
    xhr.send();
    requests[0]!.decide(true);
    expect([requests[0]!.cancelled, sends()]).toEqual([true, []]);
    xhr.addEventListener(
      'error',
      () => {
        xhr.open('GET', 'https://api.example/retry');
        xhr.send();
      },
      { once: true },
    );
    requests[1]!.decide(false);
    requests[1]!.decide(true);
    requests[2]!.decide(true);
    expect(sends().map((call) => call.url)).toEqual([
      'https://api.example/retry',
    ]);
  });

  it('preserves a pending request when a replacement open fails validation', () => {
    const { win, requests, sends } = gated();
    const xhr = new win.XMLHttpRequest();
    xhr.open('GET', 'https://api.example/valid');
    xhr.send();
    expect(() =>
      xhr.open('bad method', 'https://api.example/invalid'),
    ).toThrow();
    requests[0]!.decide(true);
    expect(sends().map((call) => call.url)).toEqual([
      'https://api.example/valid',
    ]);
  });

  it('rejects duplicate send and late headers or credentials during authorization', () => {
    const { win, requests, sends } = gated();
    const xhr = new win.XMLHttpRequest();
    xhr.open('GET', 'https://api.example/data');
    xhr.send();
    expect(() => xhr.send()).toThrow();
    expect(() => xhr.setRequestHeader('X-Test', 'late')).toThrow();
    expect(() => {
      xhr.withCredentials = true;
    }).toThrow();
    expect([requests.length, sends()]).toEqual([1, []]);
  });

  it('times out pending authorization and deducts permission time from native timeout', () => {
    const { win, requests, sends, advance } = gated();
    const xhr = new win.XMLHttpRequest();
    xhr.open('GET', 'https://api.example/expired');
    xhr.timeout = 10;
    const observed = events(xhr);
    xhr.send();
    advance(10);
    requests[0]!.decide(true);
    expect([sends(), observed]).toEqual([
      [],
      [
        ['readystatechange', 4, 0],
        ['timeout', 4, 0],
        ['loadend', 4, 0],
      ],
    ]);
    xhr.open('GET', 'https://api.example/allowed');
    xhr.timeout = 100;
    xhr.send();
    advance(25);
    xhr.timeout = 80;
    requests[1]!.decide(true);
    expect([xhr.timeout, sends()[0].timeout]).toEqual([80, 55]);
  });

  it('rejects synchronous XHR and defers immediate send failures', () => {
    const { win } = gated();
    const xhr = new win.XMLHttpRequest();
    for (const async of [false, undefined, null, 0]) {
      expect(() =>
        xhr.open('GET', 'https://api.example/data', async as any),
      ).toThrow('synchronous');
    }
    for (const failure of [
      'unsupported scheme',
      'closed transport',
      'transport error',
    ]) {
      const { win, calls, advance } = realm();
      let requests = 0;
      installXhrGate(win as any, (_url, decide) => {
        requests++;
        if (failure === 'transport error')
          throw new Error('transport closed');
        decide(false);
        return () => {};
      });
      const url =
        failure === 'unsupported scheme'
          ? 'file:///secret'
          : 'https://api.example/data';
      const xhr = new win.XMLHttpRequest();
      xhr.open('GET', url);
      xhr.send();
      const observed = events(xhr);
      expect([xhr.readyState, observed]).toEqual([1, []]);
      advance(0);
      expect([xhr.readyState, observed]).toEqual([
        4,
        [
          ['readystatechange', 4, 0],
          ['error', 4, 0],
          ['loadend', 4, 0],
        ],
      ]);
      expect([requests, calls.filter((call) => call.kind === 'send')]).toEqual([
        failure === 'unsupported scheme' ? 0 : 1,
        [],
      ]);

      xhr.open('GET', url);
      xhr.send();
      observed.length = 0;
      xhr.abort();
      const aborted: typeof observed = [
        ['readystatechange', 4, 0],
        ['abort', 4, 0],
        ['loadend', 4, 0],
      ];
      expect([xhr.readyState, observed]).toEqual([0, aborted]);
      advance(0);
      expect([xhr.readyState, observed]).toEqual([0, aborted]);

      xhr.open('GET', url);
      xhr.send();
      xhr.open('GET', './asset');
      xhr.send();
      observed.length = 0;
      advance(0);
      expect([xhr.readyState, observed]).toEqual([1, []]);
    }
  });
});

describe('XHR snapshots and recovered methods', () => {
  it('does not authorize an outer send after body conversion already sent the request', () => {
    const { win, requests, sends } = gated();
    const xhr = new win.XMLHttpRequest();
    xhr.open('POST', 'https://api.example/data');
    expect(() =>
      xhr.send({
        toString() {
          xhr.send('inner');
          return 'outer';
        },
      }),
    ).toThrow();
    expect(requests.length).toBe(1);
    requests[0]!.decide(true);
    expect(sends().map((call) => call.body)).toEqual(['inner']);
  });

  it('rechecks the send flag after header argument conversion reenters send', () => {
    const { win, requests, sends } = gated();
    const xhr = new win.XMLHttpRequest();
    xhr.open('GET', 'https://api.example/data');
    expect(() =>
      xhr.setRequestHeader(
        {
          toString() {
            xhr.send();
            return 'X-Test';
          },
        } as any,
        'late',
      ),
    ).toThrow();
    requests[0]!.decide(true);
    expect(sends().map((call) => call.headers)).toEqual([{}]);
  });

  it('snapshots credentials before open can dispatch reentrant product code', () => {
    const { win, requests, sends } = gated();
    const xhr = new win.XMLHttpRequest();
    const username = {
      toString() {
        xhr.open('GET', 'https://allowed.example/reentrant');
        return 'user';
      },
    };
    xhr.open('GET', 'https://blocked.example/actual', true, username as any);
    xhr.send();
    expect(requests.map((request) => request.url)).toEqual([
      'https://blocked.example/actual',
    ]);
    requests[0]!.decide(false);
    expect(sends()).toEqual([]);
  });

  it('does not use product-controlled iterators when forwarding open arguments', () => {
    const { win, requests } = gated();
    const xhr = new win.XMLHttpRequest();
    const iterator = Array.prototype[Symbol.iterator];
    try {
      Array.prototype[Symbol.iterator] = (() => {
        throw new Error('product iterator');
      }) as any;
      xhr.open('GET', 'https://api.example/data', true, 'user', 'password');
    } finally {
      Array.prototype[Symbol.iterator] = iterator;
    }
    xhr.send();
    expect(requests.map((request) => request.url)).toEqual([
      'https://api.example/data',
    ]);
  });

  it('uses the native upload target even if the product shadows the upload property', () => {
    const { win, requests, sends } = gated();
    const xhr = new win.XMLHttpRequest();
    const uploadEvents: string[] = [];
    xhr.upload.addEventListener('error', (event) =>
      uploadEvents.push(event.type),
    );
    xhr.upload.addEventListener('loadend', (event) =>
      uploadEvents.push(event.type),
    );
    Object.defineProperty(xhr, 'upload', {
      get() {
        throw new Error('product getter');
      },
    });
    xhr.open('POST', 'https://api.example/data');
    xhr.send('body');
    requests[0]!.decide(false);
    expect([sends(), xhr.readyState, uploadEvents]).toEqual([
      [],
      4,
      ['error', 'loadend'],
    ]);
  });

  it('captures a mutable URL once and ignores later base URL changes', () => {
    const { win, requests, sends } = gated();
    let reads = 0;
    const xhr = new win.XMLHttpRequest();
    xhr.open('GET', {
      toString: () => {
        reads++;
        return 'https://api.example/original';
      },
    } as any);
    win.document.baseURI = 'https://different.example/';
    xhr.send();
    requests[0]!.decide(true);
    expect([reads, requests[0]!.url, sends()[0].url]).toEqual([
      1,
      'https://api.example/original',
      'https://api.example/original',
    ]);
  });

  it('snapshots mutable bodies while preserving native body types', () => {
    const { win, requests, sends } = gated();
    const bytes = new Uint8Array([1, 2, 3, 4]);
    const params = new URLSearchParams('value=before');
    const form = new FormData();
    form.append('value', 'before');
    const bodies = [bytes.subarray(1, 3), params, form];
    for (const body of bodies) {
      const xhr = new win.XMLHttpRequest();
      xhr.open('POST', 'https://api.example/data');
      xhr.send(body);
    }
    bytes.fill(9);
    params.set('value', 'after');
    form.set('value', 'after');
    for (const request of requests) request.decide(true);
    const sent = sends().map((call) => call.body);
    expect([
      Array.from(sent[0]),
      sent[1].toString(),
      sent[2].get('value'),
    ]).toEqual([[2, 3], 'value=before', 'before']);
    expect(sent[1]).toBeInstanceOf(URLSearchParams);
    expect(sent[2]).toBeInstanceOf(FormData);
  });

  it('does not coerce ignored GET or HEAD bodies', () => {
    const { win, requests, sends } = gated();
    for (const method of ['GET', 'HEAD']) {
      const xhr = new win.XMLHttpRequest();
      xhr.open(method, 'https://api.example/data');
      xhr.send({
        toString() {
          throw new Error('ignored body');
        },
      });
      requests[requests.length - 1]!.decide(true);
    }
    expect(sends().map((call) => call.body)).toEqual([null, null]);
  });

  it('keeps constructor/prototype routes gated and captured intrinsics private', () => {
    const { win, Xhr, requests, sends } = gated();
    const Recovered = win.XMLHttpRequest.prototype.constructor as typeof Xhr;
    const xhr = new Recovered();
    expect(xhr).toBeInstanceOf(win.XMLHttpRequest);
    Xhr.prototype.open.call(xhr, 'GET', 'https://api.example/data');
    const originalApply = Reflect.apply;
    const originalGet = WeakMap.prototype.get;
    const originalSet = WeakMap.prototype.set;
    let leaked = false;
    try {
      Reflect.apply = (() => {
        leaked = true;
        throw new Error('leaked apply');
      }) as any;
      WeakMap.prototype.get = (() => {
        leaked = true;
        throw new Error('leaked get');
      }) as any;
      WeakMap.prototype.set = (() => {
        leaked = true;
        throw new Error('leaked set');
      }) as any;
      Xhr.prototype.send.call(xhr);
      requests[0]!.decide(false);
    } finally {
      Reflect.apply = originalApply;
      WeakMap.prototype.get = originalGet;
      WeakMap.prototype.set = originalSet;
    }
    expect([leaked, requests.length, sends(), xhr.readyState]).toEqual([
      false,
      1,
      [],
      4,
    ]);
    expect(() =>
      Object.defineProperty(Xhr.prototype, 'send', { value() {} }),
    ).toThrow();
  });
});
