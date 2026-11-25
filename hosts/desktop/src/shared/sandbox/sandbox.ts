import { type WireProvider } from '@parity/truapi';
import { type QuickJSAsyncContext, type QuickJSContext, type QuickJSHandle, type VmPropertyDescriptor } from 'quickjs-emscripten';
import { newQuickJSAsyncWASMModule } from 'quickjs-emscripten';

import { extractBytesFromVm, sendBytesToVm } from './buffers';
import { injectAbortController } from './globals/AbortController';
import { injectBlob } from './globals/Blob';
import { injectDOMGeneric } from './globals/DOMGeneric';
import { injectFormData } from './globals/FormData';
import { injectTextDecoder } from './globals/TextDecoder';
import { injectTextEncoder } from './globals/TextEncoder';
import { injectConsole } from './globals/console';
import { injectCrypto } from './globals/crypto';
import { type SubtleResolver } from './globals/cryptoSubtle';
import { injectCryptoSubtle } from './globals/cryptoSubtle';
import { type FetchResolver } from './globals/fetch';
import { injectFetch } from './globals/fetch';
import { injectIntervals, injectQueueMicrotask, injectTimeouts } from './globals/timers';
import { type Logger, createLogger } from './logger';
import { type ModuleId, type ModuleResolver } from './moduleLoader';
import { createModuleLoader } from './moduleLoader';

export type Sandbox = {
  provider: WireProvider;
  // `name` identifies the entrypoint module so its relative imports can resolve
  // against it. Required when `resolveModule` is configured; ignored otherwise.
  run: (code: string | Uint8Array, options?: { name?: ModuleId }) => Promise<void>;
  dispose: VoidFunction;
};

export type SandboxOptions = {
  // Host hook for `fetch()` inside the sandbox. If omitted, `fetch` is not injected.
  fetchResolver?: FetchResolver;
  // Maximum request body size for fetch(), in bytes. Defaults to 100 MiB.
  fetchMaxBodyBytes?: number;
  // Host hook for `crypto.subtle.*` inside the sandbox. If omitted, `crypto.subtle`
  // is not injected (only `crypto.getRandomValues` is available).
  subtleResolver?: SubtleResolver;
  // Host hook returning ES module source on demand. When provided, worker code
  // may use `import` / `export` and dynamic `import()`. When omitted, `import`
  // declarations fail at load time.
  resolveModule?: ModuleResolver;
};

// Shared mutable state for the onmessage property descriptor.
// Extracted as a plain object so it can be captured in function expressions
// without aliasing `this` (which is banned by @typescript-eslint/no-this-alias).
type OnMessageState = { handle: QuickJSHandle | null };

function makeOnMessageDescriptor(state: OnMessageState, vm: QuickJSContext): VmPropertyDescriptor<QuickJSHandle> {
  // Regular function expressions have implicit `this: any`, which satisfies
  // `(this: QuickJSHandle) => QuickJSHandle` without needing an `as` cast.
  return {
    configurable: false,
    enumerable: true,
    get() {
      return state.handle?.dup() ?? vm.null;
    },
    set(handlerHandle: QuickJSHandle) {
      state.handle?.dispose();
      const value = vm.dump(handlerHandle);
      state.handle = value == null ? null : handlerHandle.dup();
    },
  };
}

// Cap on registered `message` listeners per port. Sandbox code that registers
// fresh arrow functions in a tight loop would otherwise grow the host-side
// handle array unbounded — every entry pins a VM-side function, exhausting the
// QuickJS heap. 32 is far above any legitimate use.
const MAX_MESSAGE_LISTENERS = 32;

class SandboxPort {
  private readonly logger: Logger;

  private readonly vm: QuickJSContext;
  private readonly onMessageState: OnMessageState = { handle: null };
  private readonly messageListeners: QuickJSHandle[] = [];
  private readonly subscribers = new Set<(message: Uint8Array) => void>();
  private readonly toUint8ArrayFn: QuickJSHandle;
  // VM-side `===` helper. Required because quickjs-emscripten allocates a fresh
  // heap wrapper per `dup()` of the same VM value, so `Lifetime.value` is not a
  // reliable identity key for handles to the same JS function.
  private readonly equalsFn: QuickJSHandle;
  private disposed = false;
  private closed = false;
  readonly provider: WireProvider;

  constructor(productId: string, vm: QuickJSContext, toUint8ArrayFn: QuickJSHandle) {
    this.logger = createLogger(productId);
    this.vm = vm;
    this.toUint8ArrayFn = toUint8ArrayFn;

    const eqResult = vm.evalCode('(a, b) => a === b');
    if (eqResult.error) {
      const msg = vm.dump(eqResult.error);
      eqResult.error.dispose();
      throw new Error(`Sandbox setup error: ${JSON.stringify(msg)}`);
    }
    this.equalsFn = eqResult.value;

    this.provider = this.makeProvider();
  }

  private isSameHandle(a: QuickJSHandle, b: QuickJSHandle): boolean {
    const result = this.vm.callFunction(this.equalsFn, this.vm.undefined, a, b);
    if (result.error) {
      result.error.dispose();
      return false;
    }
    const dumped = this.vm.dump(result.value);
    result.value.dispose();
    return dumped === true;
  }

  buildHandle(): QuickJSHandle {
    const { vm } = this;
    const port = vm.newObject();

    // sandbox → host: extract Uint8Array bytes from the QuickJS handle and notify subscribers
    const postMessageFn = vm.newFunction('postMessage', dataHandle => {
      try {
        const bytes = extractBytesFromVm(vm, dataHandle);
        for (const subscriber of this.subscribers) {
          subscriber(bytes);
        }
      } catch (e) {
        this.logger.error('[Sandbox] port.postMessage: failed to extract bytes', e);
      }
    });
    vm.setProp(port, 'postMessage', postMessageFn);
    postMessageFn.dispose();

    // host → sandbox via addEventListener('message', handler)
    const addEventListenerFn = vm.newFunction('addEventListener', (typeHandle, handlerHandle) => {
      if (vm.getString(typeHandle) !== 'message') return;
      if (vm.typeof(handlerHandle) !== 'function') return;
      // Spec: EventTarget.addEventListener with the same listener is a no-op.
      if (this.messageListeners.some(h => this.isSameHandle(h, handlerHandle))) return;
      if (this.messageListeners.length >= MAX_MESSAGE_LISTENERS) {
        // Defensive cap: prevent runaway growth from sandbox code registering
        // fresh closures in a loop. We silently drop additional registrations
        // rather than throwing — DOM addEventListener is no-throw on dedup, so
        // a throw here would be the only path to surface "limit reached".
        this.logger.error(`[Sandbox] port.addEventListener: ignoring listener; cap of ${MAX_MESSAGE_LISTENERS} reached`);
        return;
      }
      this.messageListeners.push(handlerHandle.dup());
    });
    vm.setProp(port, 'addEventListener', addEventListenerFn);
    addEventListenerFn.dispose();

    const removeEventListenerFn = vm.newFunction('removeEventListener', (typeHandle, handlerHandle) => {
      if (vm.getString(typeHandle) !== 'message') return;
      const idx = this.messageListeners.findIndex(h => this.isSameHandle(h, handlerHandle));
      if (idx === -1) return;
      const removed = this.messageListeners[idx];
      if (removed) removed.dispose();
      this.messageListeners.splice(idx, 1);
    });
    vm.setProp(port, 'removeEventListener', removeEventListenerFn);
    removeEventListenerFn.dispose();

    // start() — no-op, exists for API compatibility
    const startFn = vm.newFunction('start', () => vm.undefined);
    vm.setProp(port, 'start', startFn);
    startFn.dispose();

    // close() — frees all stored handles and short-circuits future deliveries.
    const closeFn = vm.newFunction('close', () => {
      this.closed = true;
      this.disposeHandles();
    });
    vm.setProp(port, 'close', closeFn);
    closeFn.dispose();

    // onmessage getter/setter via defineProp.
    // State is accessed through a plain object ref captured in function expressions.
    const { onMessageState } = this;
    vm.defineProp(port, 'onmessage', makeOnMessageDescriptor(onMessageState, vm));

    return port;
  }

  // Returns a WireProvider implementation backed by this port's QuickJS transport
  private makeProvider(): WireProvider {
    return {
      postMessage: message => {
        this.deliver(message);
      },
      subscribe: callback => {
        this.subscribers.add(callback);
        return () => {
          this.subscribers.delete(callback);
        };
      },
      dispose: () => undefined,
    };
  }

  // Delivers raw bytes from the host into the sandbox as a MessageEvent with Uint8Array data
  private deliver(bytes: Uint8Array): void {
    if (this.disposed || this.closed) return;
    const { vm } = this;

    let dataHandle: QuickJSHandle;
    try {
      dataHandle = sendBytesToVm(vm, this.toUint8ArrayFn, bytes);
    } catch (e) {
      this.logger.error('[Sandbox] port: failed to send bytes to VM', e);
      return;
    }

    const event = vm.newObject();
    try {
      vm.setProp(event, 'data', dataHandle);
    } finally {
      dataHandle.dispose();
    }

    const handlers: QuickJSHandle[] = [];
    if (this.onMessageState.handle) handlers.push(this.onMessageState.handle);
    handlers.push(...this.messageListeners);

    try {
      for (const handler of handlers) {
        const result = vm.callFunction(handler, vm.undefined, event);
        if (result.error) {
          this.logger.error('[Sandbox] port.onmessage error:', vm.dump(result.error));
          result.error.dispose();
        } else {
          result.value.dispose();
        }
      }
    } finally {
      event.dispose();
    }

    const jobResult = vm.runtime.executePendingJobs(-1);
    if (jobResult.error) {
      this.logger.error('[Sandbox] job error after port message:', vm.dump(jobResult.error));
      jobResult.error.dispose();
    }
  }

  private disposeHandles(): void {
    this.onMessageState.handle?.dispose();
    this.onMessageState.handle = null;
    for (const h of this.messageListeners) h.dispose();
    this.messageListeners.length = 0;
  }

  dispose(): void {
    this.disposed = true;
    this.disposeHandles();
    this.subscribers.clear();
    this.equalsFn.dispose();
  }
}

class QuickJsSandbox implements Sandbox {
  private readonly logger: Logger;
  private readonly vm: QuickJSAsyncContext;
  private readonly port: SandboxPort;
  private readonly toUint8ArrayFn: QuickJSHandle;
  private readonly disposeTimers: VoidFunction;
  private readonly options: SandboxOptions;
  private disposed = false;

  readonly productId: string;
  readonly provider: WireProvider;

  constructor(productId: string, vm: QuickJSAsyncContext, options: SandboxOptions = {}) {
    this.logger = createLogger(productId);
    this.productId = productId;
    this.vm = vm;
    this.options = options;

    const helperResult = vm.evalCode('(buf) => new Uint8Array(buf)');
    if (helperResult.error) {
      const msg = vm.dump(helperResult.error);
      helperResult.error.dispose();
      throw new Error(`Sandbox setup error: ${JSON.stringify(msg)}`);
    }
    this.toUint8ArrayFn = helperResult.value;
    this.port = new SandboxPort(productId, vm, this.toUint8ArrayFn);
    this.provider = this.port.provider;

    this.disposeTimers = this.injectGlobals();
  }

  private injectGlobals(): VoidFunction {
    const { vm } = this;

    if (this.options.resolveModule) {
      const { moduleLoader, moduleNormalizer } = createModuleLoader({
        resolver: this.options.resolveModule,
        logger: this.logger,
        isDisposed: () => this.disposed,
      });
      vm.runtime.setModuleLoader(moduleLoader, moduleNormalizer);
    }

    const portHandle = this.port.buildHandle();
    vm.setProp(vm.global, '__HOST_WEBVIEW_MARK__', vm.true);
    vm.setProp(vm.global, '__HOST_API_PORT__', portHandle);
    portHandle.dispose();

    // `top` and `window` are aliased to globalThis so product code that performs
    // browser-environment checks (e.g. `window === self`) succeeds. They must be
    // non-writable / non-configurable so sandbox code cannot reassign them or
    // delete them to swap in a malicious global.
    const aliasResult = vm.evalCode(
      `Object.defineProperty(globalThis, 'top', { value: globalThis, writable: false, configurable: false, enumerable: true });
       Object.defineProperty(globalThis, 'window', { value: globalThis, writable: false, configurable: false, enumerable: true });`,
    );
    if (aliasResult.error) {
      const msg = vm.dump(aliasResult.error);
      aliasResult.error.dispose();
      throw new Error(`Failed to alias top/window: ${JSON.stringify(msg)}`);
    }
    aliasResult.value.dispose();

    injectConsole(vm, this.logger);
    injectTextEncoder(vm, this.toUint8ArrayFn);
    injectTextDecoder(vm);
    const disposeCrypto = injectCrypto(vm);
    injectDOMGeneric(vm);
    injectAbortController(vm);
    injectBlob(vm);
    injectFormData(vm);
    const disposeQueueMicrotask = injectQueueMicrotask(vm);
    const disposeIntervals = injectIntervals(vm);
    const disposeTimeouts = injectTimeouts(vm);
    const disposeFetch = this.options.fetchResolver
      ? injectFetch(vm, this.toUint8ArrayFn, this.options.fetchResolver, {
          maxBodyBytes: this.options.fetchMaxBodyBytes,
        })
      : () => undefined;
    const disposeSubtle = this.options.subtleResolver
      ? injectCryptoSubtle(vm, this.toUint8ArrayFn, this.options.subtleResolver)
      : () => undefined;

    // Hide Blob.__getBytes from sandbox code now that fetch (the only intended
    // consumer) has captured it into a closure. Without this, sandbox code
    // could call `Blob.__getBytes(b)` and mutate the underlying byte array,
    // breaking the immutable-Blob invariant.
    const cleanupResult = vm.evalCode('delete Blob.__getBytes;');
    if (cleanupResult.error) cleanupResult.error.dispose();
    else cleanupResult.value.dispose();

    return () => {
      disposeTimeouts();
      disposeIntervals();
      disposeQueueMicrotask();
      disposeFetch();
      disposeSubtle();
      disposeCrypto();
    };
  }

  async run(code: string | Uint8Array, options: { name?: ModuleId } = {}): Promise<void> {
    const { vm } = this;
    const str = typeof code === 'string' ? code : new TextDecoder().decode(code);

    if (this.options.resolveModule && options.name == null) {
      throw new Error('Sandbox.run: { name } is required when resolveModule is configured');
    }
    const filename = options.name ?? `${this.productId ?? 'unknown_product'}/worker.js`;

    // evalCodeAsync (not evalCode) so the async module loader can await the
    // host `resolveModule` hook while QuickJS resolves the import graph.
    const result = await vm.evalCodeAsync(str, filename, {
      type: 'module',
      strict: true,
    });

    if (result.error) {
      const message = vm.dump(result.error);
      result.error.dispose();
      throw new Error(`Sandbox error: ${JSON.stringify(message)}`);
    }

    // If the evaluated code returned a Promise (e.g. an async IIFE), register
    // fulfillment + rejection handlers before flushing microtasks so rejections
    // surface as thrown errors rather than being silently swallowed.

    let response: Promise<void> | undefined = undefined;

    const thenHandle = vm.getProp(result.value, 'then');
    if (vm.typeof(thenHandle) === 'function') {
      response = new Promise<void>((resolve, reject) => {
        const onFulfilled = vm.newFunction('__then', () => {
          resolve();
        });
        const onRejected = vm.newFunction('__catch', errorHandle => {
          reject(new Error(`Sandbox error: ${JSON.stringify(vm.dump(errorHandle))}`));
        });
        const chained = vm.callFunction(thenHandle, result.value, onFulfilled, onRejected);
        onFulfilled.dispose();
        onRejected.dispose();
        if (chained.error) chained.error.dispose();
        else chained.value.dispose();
      });
    }

    thenHandle.dispose();
    result.value.dispose();

    this.flushJobs();

    if (response) {
      await response;
    }
  }

  private flushJobs(): void {
    const result = this.vm.runtime.executePendingJobs(-1);
    if (result.error) {
      this.logger.error('[Sandbox] job error:', this.vm.dump(result.error));
      result.error.dispose();
    }
  }

  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;

    const { vm } = this;

    // Timer cleanup must run before vm.dispose so host intervals don't keep
    // ticking against a freed VM.
    try {
      this.disposeTimers();
    } catch (e) {
      this.logger.warn('[Sandbox] disposeTimers failed', e);
    }

    try {
      this.port.dispose();
    } catch (e) {
      this.logger.warn('[Sandbox] port.dispose failed', e);
    }

    try {
      this.toUint8ArrayFn.dispose();
    } catch (e) {
      this.logger.warn('[Sandbox] toUint8ArrayFn.dispose failed', e);
    }

    // `vm.dispose()` ends in `JS_FreeRuntime`, which asserts
    // `list_empty(&rt->gc_obj_list)`. Real product code (event listeners,
    // in-flight async chains, captured closures) almost always leaves objects
    // in the GC list, so the assertion fires and the QuickJS WASM module
    // aborts. We isolate each sandbox in its own WASM instance (see
    // `createSandbox`), so the abort kills only this sandbox's instance — the
    // host JS GC then reclaims it. Wrap in try/catch so the abort doesn't
    // bubble out of dispose().
    try {
      vm.runtime.executePendingJobs(-1);
      vm.dispose();
    } catch (e) {
      this.logger.warn('[Sandbox] vm.dispose aborted; instance abandoned', e);
    }
  }
}

// Belt-and-suspenders: if a Sandbox wrapper is GC'd without explicit dispose,
// free the underlying WASM context. The held value must close over `vm` ONLY
// (not the wrapper) — otherwise the registry's strong heldValue ref would
// keep the wrapper alive and the finalizer would never fire. Host-side
// QuickJSHandle wrappers are pure JS and reclaimed by ordinary GC.
const __sandboxFinalizer = new FinalizationRegistry<() => void>(free => free());

export async function createSandbox(productId: string, options: SandboxOptions = {}): Promise<Sandbox> {
  // One WASM instance per sandbox to contain abort blast radius — see
  // QuickJsSandbox.dispose() for why JS_FreeRuntime can abort.
  //
  // The Asyncify-built async module is used (rather than the smaller sync one)
  // so the ES module loader can await the host `resolveModule` hook. All other
  // globals operate on the QuickJSContext superclass API and are unaffected.
  const QuickJS = await newQuickJSAsyncWASMModule();
  const vm = QuickJS.newContext();
  const sandbox = new QuickJsSandbox(productId, vm, options);
  __sandboxFinalizer.register(
    sandbox,
    () => {
      try {
        vm.runtime.executePendingJobs(-1);
        vm.dispose();
      } catch {
        /* already disposed or aborted; finalizer must not throw */
      }
    },
    sandbox,
  );
  return sandbox;
}
