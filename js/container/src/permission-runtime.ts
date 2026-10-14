import { freezeValue } from './freeze.js';
import { freezeInternalResults } from '@parity/truapi/internal';

/**
 * Protect shared methods without blocking libraries such as React and Buffer
 * from assigning overrides on their own objects. Frozen data properties would
 * reject those assignments, so setters create receiver-owned properties instead.
 */
function freezePrototype(prototype: object): void {
  for (const name of Reflect.ownKeys(prototype)) {
    const { value, writable, configurable } = Object.getOwnPropertyDescriptor(prototype, name)!;
    if (!writable || !configurable) continue;
    Object.defineProperty(prototype, name, {
      get: () => value,
      set(ownValue: unknown) {
        if (Object.isFrozen(this)) return;
        Object.defineProperty(this, name, { value: ownValue, writable: true, configurable: true, enumerable: true });
      },
    });
  }
  Object.freeze(prototype);
}

export function freezePermissionRuntime(): void {
  // Block inherited then hooks without preventing products' own toString/valueOf assignments.
  Object.preventExtensions(Object.prototype);
  for (const name of [
    'Object', 'Array', // Prevent forged decoded permission fields.
    'Map', 'WeakMap', // Keep pending resolvers and private SDK transports inaccessible.
    'Set', // Prevent interception of private reply listeners.
    'Promise', // Prevent replacement of asynchronous permission results.
    'Number',
    'String', // Reserve host: request IDs for private permission replies.
    'Uint8Array', 'DataView', // Prevent altered request and reply bytes.
    'MessageChannel', 'MessagePort', 'EventTarget', // Keep lazily created legacy endpoints private.
  ] as const) {
    const constructor = globalThis[name];
    if (name !== 'Object' && name !== 'Number') freezePrototype(constructor.prototype);
    Object.freeze(constructor);
    freezeValue(globalThis, name, constructor);
  }
  for (const prototype of [
    TextEncoder.prototype,
    TextDecoder.prototype,
    Object.getPrototypeOf(Uint8Array.prototype),
    Object.getPrototypeOf(Uint8Array),
    Object.getPrototypeOf([][Symbol.iterator]()),
    Object.getPrototypeOf(Object.getPrototypeOf([][Symbol.iterator]())),
    Object.getPrototypeOf(new Map()[Symbol.iterator]()),
    Object.getPrototypeOf(new Set()[Symbol.iterator]()),
  ]) freezePrototype(prototype);
  freezeValue(globalThis, 'BigInt', BigInt);
  freezeValue(globalThis, 'setTimeout', setTimeout);
  freezeValue(globalThis, 'clearTimeout', clearTimeout);
  freezeInternalResults();
}
