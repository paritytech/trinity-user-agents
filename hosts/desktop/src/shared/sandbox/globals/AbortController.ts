import { type QuickJSContext } from 'quickjs-emscripten';

// Spec-compliant AbortController / AbortSignal for the QuickJS sandbox.
// Defined as in-VM source so we get real classes (working `instanceof`), prototype
// dispatch, and don't have to manage QuickJS handle lifetimes for every listener.
//
// Requires `DOMException` to already exist on globalThis — inject DOMGeneric first.
//
// References:
//   https://dom.spec.whatwg.org/#interface-abortcontroller
//   https://dom.spec.whatwg.org/#interface-AbortSignal
//
// Scope: AbortSignal stores listeners for any event type but only ever fires
// 'abort'. There is no generic EventTarget — that's the only event the spec
// itself ever fires at an AbortSignal.
const SOURCE = `(() => {
  const internal = new WeakMap();

  function normalizeListener(listener) {
    if (typeof listener === 'function') return listener;
    if (listener && typeof listener.handleEvent === 'function') return listener;
    return null;
  }

  function invokeListener(listener, event) {
    if (typeof listener === 'function') {
      listener.call(event.currentTarget, event);
    } else {
      listener.handleEvent(event);
    }
  }

  class AbortSignal {
    constructor() {
      internal.set(this, {
        aborted: false,
        reason: undefined,
        onabort: null,
        // type -> Array<{ listener, once, capture }>
        listeners: new Map(),
        // Cleanups for AbortSignal.any dependents
        dependents: [],
      });
    }

    get aborted() { return internal.get(this).aborted; }
    get reason() { return internal.get(this).reason; }

    get onabort() { return internal.get(this).onabort; }
    set onabort(value) {
      internal.get(this).onabort = typeof value === 'function' ? value : null;
    }

    throwIfAborted() {
      const state = internal.get(this);
      if (state.aborted) throw state.reason;
    }

    addEventListener(type, listener, options) {
      const fn = normalizeListener(listener);
      if (fn === null) return;
      const opts =
        typeof options === 'boolean' ? { capture: options } :
        options == null ? {} : options;
      const once = !!opts.once;
      const capture = !!opts.capture;
      const t = String(type);

      const state = internal.get(this);
      let bucket = state.listeners.get(t);
      if (!bucket) {
        bucket = [];
        state.listeners.set(t, bucket);
      }
      // EventTarget de-dupe: same (listener, capture) pair is ignored.
      for (const entry of bucket) {
        if (entry.listener === fn && entry.capture === capture) return;
      }
      bucket.push({ listener: fn, once, capture });

      // Spec for AbortSignal-as-listener-removal: if options.signal is an aborted
      // AbortSignal, the listener is never added.
      if (opts.signal && opts.signal instanceof AbortSignal) {
        if (opts.signal.aborted) {
          this.removeEventListener(t, fn, { capture });
          return;
        }
        const self = this;
        const cleanup = () => self.removeEventListener(t, fn, { capture });
        opts.signal.addEventListener('abort', cleanup, { once: true });
      }
    }

    removeEventListener(type, listener, options) {
      const fn = normalizeListener(listener);
      if (fn === null) return;
      const capture =
        typeof options === 'boolean' ? options :
        options && typeof options === 'object' ? !!options.capture : false;
      const state = internal.get(this);
      const bucket = state.listeners.get(String(type));
      if (!bucket) return;
      const idx = bucket.findIndex(e => e.listener === fn && e.capture === capture);
      if (idx !== -1) bucket.splice(idx, 1);
    }

    dispatchEvent(event) {
      // Minimal: only dispatch by type, no propagation phases.
      const state = internal.get(this);
      const type = event && typeof event.type === 'string' ? event.type : '';
      const bucket = state.listeners.get(type);
      let canceled = false;

      // Mutate target / currentTarget so listeners see the AbortSignal.
      try { event.target = this; } catch {}
      try { event.currentTarget = this; } catch {}

      if (bucket) {
        const snapshot = bucket.slice();
        for (const entry of snapshot) {
          if (entry.once) {
            const i = bucket.indexOf(entry);
            if (i !== -1) bucket.splice(i, 1);
          }
          // Per spec, listener exceptions are reported but don't stop dispatch.
          try { invokeListener(entry.listener, event); }
          catch (e) { try { console.error(e); } catch {} }
        }
      }
      return !canceled;
    }

    static abort(reason) {
      const sig = new AbortSignal();
      signalAbort(sig, reason);
      return sig;
    }

    static timeout(milliseconds) {
      const ms = Number(milliseconds);
      if (!Number.isFinite(ms) || ms < 0) {
        throw new TypeError('AbortSignal.timeout: milliseconds must be a non-negative finite number');
      }
      const sig = new AbortSignal();
      setTimeout(() => {
        signalAbort(sig, new DOMException('The operation timed out.', 'TimeoutError'));
      }, ms);
      return sig;
    }

    static any(signals) {
      const result = new AbortSignal();
      const list = [];
      for (const s of signals) {
        if (!(s instanceof AbortSignal)) {
          throw new TypeError('AbortSignal.any: every entry must be an AbortSignal');
        }
        list.push(s);
      }
      for (const s of list) {
        if (s.aborted) {
          signalAbort(result, s.reason);
          return result;
        }
      }
      const onAbort = function (event) {
        // Detach from every other source so we don't keep them alive via the
        // listener closure for a result we'll never settle again.
        for (const s of list) s.removeEventListener('abort', onAbort);
        signalAbort(result, event.currentTarget.reason);
      };
      for (const s of list) {
        s.addEventListener('abort', onAbort, { once: true });
      }
      return result;
    }
  }

  Object.defineProperty(AbortSignal.prototype, Symbol.toStringTag, {
    value: 'AbortSignal', configurable: true,
  });

  function signalAbort(signal, reason) {
    const state = internal.get(signal);
    if (state.aborted) return;
    state.aborted = true;
    state.reason =
      reason !== undefined
        ? reason
        : new DOMException('signal is aborted without reason', 'AbortError');

    const event = {
      type: 'abort',
      target: signal,
      currentTarget: signal,
      defaultPrevented: false,
      cancelable: false,
      bubbles: false,
    };

    // onabort fires before addEventListener listeners (spec: it's just an
    // event handler attribute, registered as a listener, but Chromium/WebKit
    // fire onabort first since it was implicitly registered first).
    if (typeof state.onabort === 'function') {
      try { state.onabort.call(signal, event); }
      catch (e) { try { console.error(e); } catch {} }
    }

    signal.dispatchEvent(event);
  }

  class AbortController {
    constructor() {
      const signal = new AbortSignal();
      Object.defineProperty(this, 'signal', {
        value: signal, writable: false, configurable: false, enumerable: true,
      });
    }
    abort(reason) {
      signalAbort(this.signal, reason);
    }
  }

  Object.defineProperty(AbortController.prototype, Symbol.toStringTag, {
    value: 'AbortController', configurable: true,
  });

  globalThis.AbortController = AbortController;
  globalThis.AbortSignal = AbortSignal;
})();`;

export function injectAbortController(vm: QuickJSContext) {
  const result = vm.evalCode(SOURCE);
  if (result.error) {
    const msg = vm.dump(result.error);
    result.error.dispose();
    throw new Error(`Failed to inject AbortController: ${JSON.stringify(msg)}`);
  }
  result.value.dispose();
}
