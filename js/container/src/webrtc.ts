/* eslint-disable @typescript-eslint/no-explicit-any */

import { freezeAndDelete, freezeValue } from './freeze.js';
import type { WebRtcAuthorization } from './network-transport.js';

export function installWebRtcPolicy(
  win: any,
  authorize: WebRtcAuthorization | false,
): void {
  const aliases = [
    'RTCPeerConnection',
    'webkitRTCPeerConnection',
    'mozRTCPeerConnection',
  ];
  if (typeof authorize !== 'function') {
    for (const name of aliases) freezeAndDelete(win, name);
    return;
  }
  const apply = Reflect.apply;
  const construct = Reflect.construct;
  const get = Reflect.get;
  const define = Object.defineProperty;
  const NativePromise = win.Promise;
  const NativeError = win.TypeError;
  const NativeProxy = Proxy;
  const finite = Number.isFinite;
  const truncate = Math.trunc;
  const states = new WeakMap<object, Connection>();
  const weakGet = WeakMap.prototype.get;
  const weakSet = WeakMap.prototype.set;
  const installed = new Map<any, any>();

  type Call = {
    method: (...args: any[]) => any;
    args: any[];
    errorCallback: ((error: unknown) => void) | undefined;
    resolve: (value: any) => void;
    reject: (error: unknown) => void;
    next: Call | null;
  };
  type Connection = {
    phase: 'idle' | 'pending' | 'allowed' | 'denied' | 'closed';
    pool: number;
    head: Call | null;
    tail: Call | null;
    cancel: (() => void) | null;
  };

  function state(connection: object): Connection {
    const value = apply(weakGet, states, [connection]);
    if (!value) throw new NativeError('Invalid RTCPeerConnection receiver');
    return value;
  }

  function configuration(input: any): { value: any; pool: number } {
    const result = { value: input, pool: 0 };
    if (input == null) return result;
    if (typeof input !== 'object' && typeof input !== 'function') {
      throw new NativeError('RTCConfiguration must be an object');
    }
    // WebIDL reads inherited fields too; spreading the dictionary would lose them.
    result.value = new NativeProxy(
      {},
      {
        get(_target, name) {
          const value = get(input, name, input);
          if (name !== 'iceCandidatePoolSize') return value;
          const number = value === undefined ? 0 : +value;
          const pool = truncate(number);
          if (!finite(number) || pool < 0 || pool > 255) {
            throw new NativeError(
              'iceCandidatePoolSize must be between 0 and 255',
            );
          }
          result.pool = pool;
          return 0;
        },
      },
    );
    return result;
  }

  function withPool(value: any, pool: number): any {
    define(value, 'iceCandidatePoolSize', {
      value: pool,
      writable: true,
      enumerable: true,
      configurable: true,
    });
    return value;
  }

  function drain(connection: any, current: Connection): void {
    let call = current.head;
    current.head = null;
    current.tail = null;
    while (call) {
      try {
        if (current.phase === 'allowed') {
          call.resolve(apply(call.method, connection, call.args));
        } else {
          const error = new NativeError(
            current.phase === 'closed'
              ? 'WebRTC connection is closed'
              : 'WebRTC access is not allowed',
          );
          if (call.errorCallback) {
            apply(call.errorCallback, undefined, [error]);
            call.resolve(undefined);
          } else {
            call.reject(error);
          }
        }
      } catch (error) {
        call.reject(error);
      }
      call = call.next;
    }
  }

  for (const alias of aliases) {
    const Native = win[alias];
    if (typeof Native !== 'function') continue;
    const previous = installed.get(Native) ?? installed.get(Native.prototype);
    if (previous) {
      freezeValue(win, alias, previous);
      continue;
    }
    const prototype = Native.prototype;
    const nativeClose = prototype.close;
    const nativeGetConfiguration = prototype.getConfiguration;
    const nativeSetConfiguration = prototype.setConfiguration;

    function finish(
      connection: any,
      current: Connection,
      allowed: boolean,
    ): void {
      if (current.phase !== 'pending') return;
      current.phase = allowed === true ? 'allowed' : 'denied';
      current.cancel?.();
      current.cancel = null;
      try {
        if (current.phase === 'allowed' && current.pool !== 0) {
          apply(nativeSetConfiguration, connection, [
            withPool(
              apply(nativeGetConfiguration, connection, []),
              current.pool,
            ),
          ]);
        } else if (current.phase === 'denied') {
          apply(nativeClose, connection, []);
        }
      } catch {
        current.phase = 'denied';
        try {
          apply(nativeClose, connection, []);
        } catch {
          /* already closed */
        }
      }
      drain(connection, current);
    }

    const Guarded = new NativeProxy(Native, {
      construct(_target, args, newTarget) {
        const requested = configuration(args[0]);
        define(args, '0', {
          value: requested.value,
          writable: true,
          enumerable: true,
          configurable: true,
        });
        const connection = construct(Native, args, newTarget);
        apply(weakSet, states, [
          connection,
          {
            phase: 'idle',
            pool: requested.pool,
            head: null,
            tail: null,
            cancel: null,
          },
        ]);
        return connection;
      },
    });
    freezeValue(prototype, 'constructor', Guarded);
    for (const method of [
      'createOffer',
      'createAnswer',
      'setLocalDescription',
      'setRemoteDescription',
      'addIceCandidate',
    ]) {
      const nativeMethod = prototype[method];
      freezeValue(prototype, method, function (this: any, ...args: any[]) {
        return new NativePromise(
          (resolve: (value: any) => void, reject: (error: unknown) => void) => {
            const current = state(this);
            const callbackIndex =
              method === 'createOffer' || method === 'createAnswer' ? 0 : 1;
            const errorCallback =
              typeof args[callbackIndex] === 'function' &&
              typeof args[callbackIndex + 1] === 'function'
                ? args[callbackIndex + 1]
                : undefined;
            const call: Call = {
              method: nativeMethod,
              args,
              errorCallback,
              resolve,
              reject,
              next: null,
            };
            if (current.tail) current.tail.next = call;
            else current.head = call;
            current.tail = call;
            if (current.phase !== 'idle' && current.phase !== 'pending') {
              drain(this, current);
              return;
            }
            if (current.phase === 'pending') return;
            current.phase = 'pending';
            try {
              const cancel = authorize((allowed) =>
                finish(this, current, allowed),
              );
              if (current.phase === 'pending') current.cancel = cancel;
              else cancel();
            } catch {
              finish(this, current, false);
            }
          },
        );
      });
    }
    freezeValue(
      prototype,
      'setConfiguration',
      function (this: any, input: any) {
        const current = state(this);
        if (current.phase === 'allowed')
          return apply(nativeSetConfiguration, this, [input]);
        const requested = configuration(input);
        const result = apply(nativeSetConfiguration, this, [requested.value]);
        current.pool = requested.pool;
        return result;
      },
    );
    freezeValue(prototype, 'getConfiguration', function (this: any) {
      const current = state(this);
      const value = apply(nativeGetConfiguration, this, []);
      return current.phase === 'allowed'
        ? value
        : withPool(value, current.pool);
    });
    freezeValue(prototype, 'close', function (this: any) {
      const current = state(this);
      current.phase = 'closed';
      current.cancel?.();
      current.cancel = null;
      try {
        return apply(nativeClose, this, []);
      } finally {
        drain(this, current);
      }
    });
    installed.set(Native, Guarded);
    installed.set(prototype, Guarded);
    freezeValue(win, alias, Guarded);
  }
}
