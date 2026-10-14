import { describe, expect, it } from 'bun:test';
import { createContext, runInContext } from 'node:vm';
import { browserGlobals, browserScript } from './test-browser.js';

const source = await browserScript(`
  import { freezePermissionRuntime } from './permission-runtime.ts';
  freezePermissionRuntime();
`);

function browser() {
  const context = createContext({
    ...browserGlobals(),
    MessageChannel: class {},
    MessagePort: class {},
    setTimeout,
    clearTimeout,
  });
  runInContext('window = globalThis', context);
  runInContext(source, context);
  return context;
}

describe('product library compatibility', () => {
  it('ignores shared prototype assignments without interrupting product startup', () => {
    const context = browser();
    expect(runInContext(`
      'use strict';
      const replacement = () => { throw new Error('shared method replaced'); };
      Array.prototype.push = replacement;
      Uint8Array.prototype.set = replacement;
      TextEncoder.prototype.encode = replacement;
      const values = [];
      values.push('ready');
      const bytes = new Uint8Array(2);
      bytes.set(new TextEncoder().encode('Hi'));
      ({ values, bytes: [...bytes] });
    `, context)).toEqual({ values: ['ready'], bytes: [72, 105] });
  });

  it('allows products to register the observable interoperability symbol', () => {
    const context = browser();
    expect(runInContext(`
      Reflect.defineProperty(Symbol, 'observable', { value: Symbol('observable') });
    `, context)).toBe(true);
  });

  it('allows metadata builders to attach their own call property to a function', () => {
    const context = browser();
    expect(runInContext(`
      const getLookupEntryDef = () => 'lookup';
      const metadata = { version: 15 };
      const getCall = () => ({ name: 'transfer' });
      const lookup = Object.assign(getLookupEntryDef, { metadata, call: getCall() });
      ({ result: lookup(), metadata: lookup.metadata, call: lookup.call });
    `, context)).toEqual({
      result: 'lookup',
      metadata: { version: 15 },
      call: { name: 'transfer' },
    });
  });

  it('allows product objects to define their own primitive conversions in strict mode', () => {
    const context = browser();
    expect(runInContext(`
      (() => {
        'use strict';
        const amount = {};
        amount.toString = () => '12 DOT';
        amount.valueOf = () => 12;
        return { text: String(amount), value: Number(amount) };
      })();
    `, context)).toEqual({ text: '12 DOT', value: 12 });
  });

  it('allows React promise inheritance', async () => {
    const context = browser();
    expect(await runInContext(`
      (async () => {
        'use strict';
        function ReactPromise() {}
        ReactPromise.prototype = Object.create(Promise.prototype);
        ReactPromise.prototype.then = function (resolve) { resolve('React is ready'); };
        return await Promise.resolve(new ReactPromise());
      })();
    `, context)).toBe('React is ready');
  });

  it('allows Next and webpack array hooks', () => {
    const context = browser();
    expect(runInContext(`
      const delivered = [];
      const flight = [];
      (() => {
        'use strict';
        flight.push = (frame) => delivered.push(frame);
        flight.push('server component');
      })();
      const chunks = [];
      chunks.push = function (original, chunk) {
        delivered.push(chunk);
        return original(chunk);
      }.bind(null, chunks.push.bind(chunks));
      const length = chunks.push('module');
      ({ delivered, flight: [...flight], chunks: [...chunks], length });
    `, context)).toEqual({
      delivered: ['server component', 'module'],
      flight: [],
      chunks: ['module'],
      length: 1,
    });
  });

  it('allows Buffer-style typed array overrides', () => {
    const context = browser();
    expect(runInContext(`
      (() => {
        'use strict';
        const prototype = Object.getPrototypeOf(Uint8Array.prototype);
        const nativeSlice = prototype.slice;
        function ProductBuffer(...args) {
          return Object.setPrototypeOf(new Uint8Array(...args), ProductBuffer.prototype);
        }
        Object.setPrototypeOf(ProductBuffer.prototype, Uint8Array.prototype);
        Object.setPrototypeOf(ProductBuffer, Uint8Array);
        ProductBuffer.prototype.toString = function () { return 'buffer:' + this.join(','); };
        ProductBuffer.prototype.slice = function (start, end) {
          return Reflect.apply(nativeSlice, this, [start, end]);
        };
        const buffer = new ProductBuffer([72, 105]);
        buffer[Symbol.iterator] = function* () { yield this[1]; yield this[0]; };
        return {
          text: buffer.toString(), sliced: buffer.slice(1).toString(), iterated: [...buffer],
        };
      })();
    `, context)).toEqual({
      text: 'buffer:72,105',
      sliced: 'buffer:105',
      iterated: [105, 72],
    });
  });
});
