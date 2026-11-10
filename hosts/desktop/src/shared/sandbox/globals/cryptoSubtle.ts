/* eslint-disable @typescript-eslint/consistent-type-assertions -- Ported verbatim from
   @novasamatech/host-worker-sandbox. Every assertion here narrows a value that has just
   crossed the QuickJS boundary, after a runtime check the compiler cannot follow
   (`typeof v === 'object'`, `ArrayBuffer.isView(v)`). Rewriting the marshalling to satisfy
   the rule would mean re-deriving crypto and fetch bridging that arrived with no tests, so
   the port keeps it as-is; tighten it when this file grows its own coverage. */
import { nanoid } from 'nanoid';
import { type QuickJSContext, type QuickJSHandle } from 'quickjs-emscripten';

import { extractBytesFromVm, sendBytesToVm, setPropAndDispose, withProp } from '../buffers';

// Args are typed by reusing the host's WebCrypto SubtleCrypto signatures, so
// resolver implementations get full parameter inference per method without
// having to cast (algorithm shape, CryptoKey vs BufferSource, etc.).
//
// Note: after VM→host marshalling, `BufferSource` arrives as `ArrayBuffer`
// specifically (we don't preserve the original typed-array view), but
// `BufferSource` accepts ArrayBuffer so this is sound.
export type SubtleCall =
  | { method: 'digest'; args: Parameters<SubtleCrypto['digest']> }
  | { method: 'sign'; args: Parameters<SubtleCrypto['sign']> }
  | { method: 'verify'; args: Parameters<SubtleCrypto['verify']> }
  | { method: 'encrypt'; args: Parameters<SubtleCrypto['encrypt']> }
  | { method: 'decrypt'; args: Parameters<SubtleCrypto['decrypt']> }
  | { method: 'generateKey'; args: Parameters<SubtleCrypto['generateKey']> }
  | { method: 'deriveBits'; args: Parameters<SubtleCrypto['deriveBits']> }
  | { method: 'deriveKey'; args: Parameters<SubtleCrypto['deriveKey']> }
  | { method: 'importKey'; args: Parameters<SubtleCrypto['importKey']> }
  | { method: 'exportKey'; args: Parameters<SubtleCrypto['exportKey']> }
  | { method: 'wrapKey'; args: Parameters<SubtleCrypto['wrapKey']> }
  | { method: 'unwrapKey'; args: Parameters<SubtleCrypto['unwrapKey']> };

export type SubtleResolver = (call: SubtleCall) => Promise<unknown>;

const VALID_SUBTLE_METHODS: ReadonlySet<string> = new Set([
  'digest',
  'sign',
  'verify',
  'encrypt',
  'decrypt',
  'generateKey',
  'deriveBits',
  'deriveKey',
  'importKey',
  'exportKey',
  'wrapKey',
  'unwrapKey',
]);

const BRIDGE_NAME = `__SUBTLE_BRIDGE_${nanoid()}__`;

// In-VM `CryptoKey` is a frozen wrapper carrying an opaque host-side id; the
// real CryptoKey lives in a host registry. Args/results crossing the boundary
// are walked recursively to swap binary and key references for markers:
//
//   ArrayBuffer / TypedArray  ↔  { __byteRef: idx }   (bytes ride a side array)
//   CryptoKey                  ↔  { __cryptoKeySpec: { __subtleKeyId, type, ... } }
//   CryptoKeyPair              ↔  { __cryptoKeyPair: { publicKey, privateKey } }
//
// Bridge call signature:  bridge(method, structJson, bytesArray) → {json, bytes}
// Bytes are transferred as real Uint8Arrays via the buffer helpers, sidestepping
// per-byte `Array.from` / JSON serialization, which dominates large encrypt /
// decrypt / digest payloads.
const VM_SOURCE = `(() => {
  class CryptoKey {
    constructor() { throw new TypeError('Illegal constructor'); }
  }
  Object.defineProperty(CryptoKey.prototype, Symbol.toStringTag, { value: 'CryptoKey', configurable: true });

  // When a CryptoKey wrapper becomes unreachable in the sandbox, tell the host
  // to drop the underlying real CryptoKey from its registry. Without this, a
  // long-lived sandbox that imports / generates many keys leaks them all.
  const __keyFinalizer = new FinalizationRegistry((id) => {
    bridge('__release', [id]).catch(() => {});
  });

  function makeCryptoKey(spec) {
    const k = Object.create(CryptoKey.prototype);
    Object.defineProperty(k, 'type', { value: spec.type, enumerable: true });
    Object.defineProperty(k, 'extractable', { value: !!spec.extractable, enumerable: true });
    Object.defineProperty(k, 'algorithm', { value: Object.freeze(Object.assign({}, spec.algorithm)), enumerable: true });
    Object.defineProperty(k, 'usages', { value: Object.freeze([...(spec.usages || [])]), enumerable: true });
    Object.defineProperty(k, '__subtleKeyId', { value: spec['__subtleKeyId'], enumerable: false });
    __keyFinalizer.register(k, spec['__subtleKeyId']);
    return k;
  }

  // WebCrypto algorithm-name canonicalization. The spec mandates case-insensitive
  // names, but a resolver doing its own switch on \`algorithm.name\` may not
  // normalize — we canonicalize at the bridge to block case-confusion bypass.
  const __algNameLookup = new Map([
    'AES-CBC', 'AES-CTR', 'AES-GCM', 'AES-KW',
    'HMAC', 'RSA-PSS', 'RSASSA-PKCS1-v1_5', 'RSA-OAEP',
    'ECDSA', 'ECDH', 'HKDF', 'PBKDF2',
    'SHA-1', 'SHA-256', 'SHA-384', 'SHA-512',
    'Ed25519', 'X25519',
  ].map(n => [n.toLowerCase(), n]));
  const __isPollutionKey = k => k === '__proto__' || k === 'constructor' || k === 'prototype';

  function marshal(v, bytes) {
    if (v == null || typeof v !== 'object') return v;
    if (v instanceof CryptoKey) return { __subtleKeyId: v['__subtleKeyId'] };
    if (v instanceof ArrayBuffer) {
      const idx = bytes.length;
      bytes.push(new Uint8Array(v));
      return { __byteRef: idx };
    }
    if (ArrayBuffer.isView(v)) {
      const idx = bytes.length;
      bytes.push(new Uint8Array(v.buffer, v.byteOffset, v.byteLength));
      return { __byteRef: idx };
    }
    if (Array.isArray(v)) return v.map(x => marshal(x, bytes));
    const out = Object.create(null);
    for (const k of Object.keys(v)) {
      if (__isPollutionKey(k)) continue;
      const raw = v[k];
      // Canonicalize algorithm names; non-algorithm \`name\` fields pass through.
      const canon = k === 'name' && typeof raw === 'string' ? __algNameLookup.get(raw.toLowerCase()) : undefined;
      out[k] = marshal(canon ?? raw, bytes);
    }
    return out;
  }

  function unmarshal(v, bytes) {
    if (v == null || typeof v !== 'object') return v;
    if (typeof v['__byteRef'] === 'number') return bytes[v['__byteRef']].slice().buffer;
    if (v.__cryptoKeySpec) return makeCryptoKey(v.__cryptoKeySpec);
    if (v.__cryptoKeyPair) return {
      publicKey: makeCryptoKey(v.__cryptoKeyPair.publicKey),
      privateKey: makeCryptoKey(v.__cryptoKeyPair.privateKey),
    };
    if (Array.isArray(v)) return v.map(x => unmarshal(x, bytes));
    // Plain {} so sandbox consumers can use Object.prototype methods on the
    // result (e.g. EcKeyAlgorithm). Pollution keys are still skipped to
    // protect the sandbox from a malicious resolver response.
    const out = {};
    for (const k of Object.keys(v)) {
      if (__isPollutionKey(k)) continue;
      out[k] = unmarshal(v[k], bytes);
    }
    return out;
  }

  // Capture the bridge into a closure local, then remove it from globalThis so
  // sandbox code cannot call it directly. (Bracket access because BRIDGE_NAME
  // is randomized via nanoid and may contain '-', which is not a valid identifier.)
  const bridge = globalThis[${JSON.stringify(BRIDGE_NAME)}];
  delete globalThis[${JSON.stringify(BRIDGE_NAME)}];
  function call(method, args) {
    const inBytes = [];
    const struct = args.map(a => marshal(a, inBytes));
    return bridge(method, struct, inBytes).then(result => {
      const parsed = result.structJson ? JSON.parse(result.structJson) : null;
      return unmarshal(parsed, result.bytes || []);
    });
  }

  const subtle = {
    digest(algorithm, data) { return call('digest', [algorithm, data]); },
    sign(algorithm, key, data) { return call('sign', [algorithm, key, data]); },
    verify(algorithm, key, signature, data) { return call('verify', [algorithm, key, signature, data]); },
    encrypt(algorithm, key, data) { return call('encrypt', [algorithm, key, data]); },
    decrypt(algorithm, key, data) { return call('decrypt', [algorithm, key, data]); },
    generateKey(algorithm, extractable, keyUsages) { return call('generateKey', [algorithm, extractable, keyUsages]); },
    deriveBits(algorithm, baseKey, length) { return call('deriveBits', [algorithm, baseKey, length]); },
    deriveKey(algorithm, baseKey, derivedKeyAlgorithm, extractable, keyUsages) {
      return call('deriveKey', [algorithm, baseKey, derivedKeyAlgorithm, extractable, keyUsages]);
    },
    importKey(format, keyData, algorithm, extractable, keyUsages) {
      return call('importKey', [format, keyData, algorithm, extractable, keyUsages]);
    },
    exportKey(format, key) { return call('exportKey', [format, key]); },
    wrapKey(format, key, wrappingKey, wrapAlgorithm) { return call('wrapKey', [format, key, wrappingKey, wrapAlgorithm]); },
    unwrapKey(format, wrappedKey, unwrappingKey, unwrapAlgorithm, unwrappedKeyAlgorithm, extractable, keyUsages) {
      return call('unwrapKey', [format, wrappedKey, unwrappingKey, unwrapAlgorithm, unwrappedKeyAlgorithm, extractable, keyUsages]);
    },
  };
  Object.defineProperty(subtle, Symbol.toStringTag, { value: 'SubtleCrypto', configurable: true });

  if (typeof globalThis.crypto !== 'object' || globalThis.crypto === null) globalThis.crypto = {};
  Object.defineProperty(globalThis.crypto, 'subtle', { value: subtle, enumerable: true, configurable: true });
  globalThis.CryptoKey = CryptoKey;
})();`;

export function injectCryptoSubtle(vm: QuickJSContext, toUint8ArrayFn: QuickJSHandle, resolver: SubtleResolver): VoidFunction {
  let disposed = false;
  const registry = new Map<string, CryptoKey>();
  let nextId = 1;

  // Walk dumped args: replace markers with real bytes / CryptoKeys. `bytes` is
  // the host-side array of Uint8Arrays already extracted from the VM-side
  // `bytesArray` argument.
  const inflate = (v: unknown, bytes: Uint8Array[]): unknown => {
    if (v == null || typeof v !== 'object') return v;
    if (Array.isArray(v)) return v.map(x => inflate(x, bytes));
    const o = v as Record<string, unknown>;
    if (typeof o['__byteRef'] === 'number') {
      const u8 = bytes[o['__byteRef']];
      if (!u8) throw new Error(`subtle bridge: missing byteRef ${o['__byteRef']}`);
      // Return ArrayBuffer (BufferSource shape WebCrypto methods expect).
      return u8.slice().buffer;
    }
    if (typeof o['__subtleKeyId'] === 'string') {
      const key = registry.get(o['__subtleKeyId']);
      if (!key) throw new Error(`Unknown CryptoKey id: ${o['__subtleKeyId']}`);
      return key;
    }
    const out: Record<string, unknown> = {};
    for (const k of Object.keys(o)) out[k] = inflate(o[k], bytes);
    return out;
  };

  const storeKey = (key: CryptoKey): Record<string, unknown> => {
    const id = String(nextId++);
    registry.set(id, key);
    return {
      __subtleKeyId: id,
      type: key.type,
      extractable: key.extractable,
      algorithm: key.algorithm,
      usages: [...key.usages],
    };
  };

  // Walk a host result: replace ArrayBuffer / CryptoKey / CryptoKeyPair with
  // markers. Bytes are pushed onto `out` and referenced by index.
  const deflate = (v: unknown, out: Uint8Array[]): unknown => {
    if (v == null || typeof v !== 'object') return v;
    if (v instanceof ArrayBuffer) {
      const idx = out.length;
      out.push(new Uint8Array(v));
      return { __byteRef: idx };
    }
    if (ArrayBuffer.isView(v)) {
      const view = v as ArrayBufferView;
      const idx = out.length;
      out.push(new Uint8Array(view.buffer, view.byteOffset, view.byteLength));
      return { __byteRef: idx };
    }
    if (v instanceof CryptoKey) return { __cryptoKeySpec: storeKey(v) };
    const pair = v as { publicKey?: unknown; privateKey?: unknown };
    if (pair.publicKey instanceof CryptoKey && pair.privateKey instanceof CryptoKey) {
      return {
        __cryptoKeyPair: {
          publicKey: storeKey(pair.publicKey),
          privateKey: storeKey(pair.privateKey),
        },
      };
    }
    if (Array.isArray(v)) return v.map(x => deflate(x, out));
    const wrapped: Record<string, unknown> = {};
    for (const k of Object.keys(v as object)) wrapped[k] = deflate((v as Record<string, unknown>)[k], out);
    return wrapped;
  };

  // Pull the bytes-side array (Uint8Array[]) out of the VM at bridge entry.
  const extractBytesArray = (bytesH: QuickJSHandle): Uint8Array[] => {
    const len = withProp(vm, bytesH, 'length', h => vm.getNumber(h));
    const out: Uint8Array[] = [];
    for (let i = 0; i < len; i++) {
      out.push(withProp(vm, bytesH, i, h => extractBytesFromVm(vm, h)));
    }
    return out;
  };

  // Build the VM-side `{ structJson, bytes }` response object the wrapper expects.
  // `structJson` is small (just the marker tree) — JSON-string transport is fine.
  // `bytes` rides as real Uint8Array[] via sendBytesToVm.
  const buildResponseHandle = (deflated: unknown, outBytes: Uint8Array[]): QuickJSHandle => {
    const obj = vm.newObject();
    try {
      setPropAndDispose(vm, obj, 'structJson', vm.newString(JSON.stringify(deflated ?? null)));

      const bytesArr = vm.newArray();
      try {
        for (const [i, u8] of outBytes.entries()) {
          setPropAndDispose(vm, bytesArr, i, sendBytesToVm(vm, toUint8ArrayFn, u8));
        }
        vm.setProp(obj, 'bytes', bytesArr);
      } finally {
        bytesArr.dispose();
      }
      return obj;
    } catch (e) {
      obj.dispose();
      throw e;
    }
  };

  const bridge = vm.newFunction(BRIDGE_NAME, (methodH, argsH, bytesH) => {
    const method = vm.getString(methodH);

    // Internal: VM-side FinalizationRegistry calls this when a CryptoKey
    // wrapper is GC'd, so we can drop the real CryptoKey from our registry.
    if (method === '__release') {
      const ids = (vm.dump(argsH) ?? []) as unknown[];
      for (const id of ids) {
        if (typeof id === 'string') registry.delete(id);
      }
      const deferred = vm.newPromise();
      const obj = vm.newObject();
      deferred.resolve(obj);
      obj.dispose();
      return deferred.handle;
    }

    // `method` is fully sandbox-controlled. Reject anything outside the
    // SubtleCall union before we marshal args or hit the resolver.
    if (!VALID_SUBTLE_METHODS.has(method)) {
      const deferred = vm.newPromise();
      const errHandle = vm.newError(`Unknown SubtleCrypto method: ${method}`);
      deferred.reject(errHandle);
      errHandle.dispose();
      if (!disposed) vm.runtime.executePendingJobs(-1);
      return deferred.handle;
    }

    const rawArgs = (vm.dump(argsH) ?? []) as unknown[];
    const inBytes = extractBytesArray(bytesH);

    const deferred = vm.newPromise();

    const call = { method, args: inflate(rawArgs, inBytes) } as SubtleCall;

    Promise.resolve()
      .then(() => resolver(call))
      .then(result => {
        if (disposed) return;
        const outBytes: Uint8Array[] = [];
        const deflated = deflate(result, outBytes);
        const respHandle = buildResponseHandle(deflated, outBytes);
        deferred.resolve(respHandle);
        respHandle.dispose();
      })
      .catch((err: unknown) => {
        if (disposed) return;
        const errHandle = vm.newError(err instanceof Error ? err.message : String(err));
        // Preserve resolver-supplied error name (`OperationError`, `DataError`,
        // `InvalidAccessError`, etc. — WebCrypto rejects with specific names).
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

  const result = vm.evalCode(VM_SOURCE);
  if (result.error) {
    const msg = vm.dump(result.error);
    result.error.dispose();
    throw new Error(`Failed to inject crypto.subtle: ${JSON.stringify(msg)}`);
  }
  result.value.dispose();

  return () => {
    disposed = true;
    registry.clear();
  };
}
