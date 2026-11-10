import { type QuickJSContext } from 'quickjs-emscripten';

// Spec: https://www.w3.org/TR/FileAPI/#blob-section
//
// Depends on `TextEncoder` / `TextDecoder` already being injected.
// Exposes a non-enumerable `Blob.__getBytes(blob)` so other in-VM modules
// (fetch's multipart encoder, body extraction) can read the underlying bytes
// synchronously without going through the async `bytes()` method.
const SOURCE = `(() => {
  if (typeof globalThis.Blob === 'function') return;

  const utf8Encoder = new TextEncoder();
  const utf8Decoder = new TextDecoder();
  const _blobBytes = new WeakMap();

  class Blob {
    constructor(parts = [], options = {}) {
      const chunks = [];
      let totalLen = 0;
      const list = parts == null ? [] : parts;
      for (const part of list) {
        let chunk;
        if (typeof part === 'string') chunk = utf8Encoder.encode(part);
        else if (part instanceof Blob) chunk = _blobBytes.get(part).slice();
        else if (part instanceof ArrayBuffer) chunk = new Uint8Array(part).slice();
        else if (ArrayBuffer.isView(part)) chunk = new Uint8Array(part.buffer, part.byteOffset, part.byteLength).slice();
        else chunk = utf8Encoder.encode(String(part));
        chunks.push(chunk);
        totalLen += chunk.byteLength;
      }
      const bytes = new Uint8Array(totalLen);
      let off = 0;
      for (const c of chunks) { bytes.set(c, off); off += c.byteLength; }
      _blobBytes.set(this, bytes);
      Object.defineProperty(this, 'size', { value: totalLen, enumerable: true });
      Object.defineProperty(this, 'type', { value: String(options.type || '').toLowerCase(), enumerable: true });
    }
    arrayBuffer() { return Promise.resolve(_blobBytes.get(this).slice().buffer); }
    text() { return Promise.resolve(utf8Decoder.decode(_blobBytes.get(this))); }
    bytes() { return Promise.resolve(_blobBytes.get(this).slice()); }
    slice(start, end, contentType) {
      const sliced = _blobBytes.get(this).slice(start, end);
      const b = Object.create(Blob.prototype);
      _blobBytes.set(b, sliced);
      Object.defineProperty(b, 'size', { value: sliced.byteLength, enumerable: true });
      Object.defineProperty(b, 'type', { value: String(contentType || '').toLowerCase(), enumerable: true });
      return b;
    }
  }
  Object.defineProperty(Blob.prototype, Symbol.toStringTag, { value: 'Blob', configurable: true });

  // Internal accessor for cross-module use (fetch multipart encoder, body extraction).
  // configurable: true so the host can delete it after fetch captures it,
  // preventing sandbox code from mutating Blob bytes through this back door.
  Object.defineProperty(Blob, '__getBytes', {
    value: (b) => _blobBytes.get(b),
    enumerable: false,
    configurable: true,
  });

  globalThis.Blob = Blob;
})();`;

export function injectBlob(vm: QuickJSContext) {
  const result = vm.evalCode(SOURCE);
  if (result.error) {
    const msg = vm.dump(result.error);
    result.error.dispose();
    throw new Error(`Failed to inject Blob: ${JSON.stringify(msg)}`);
  }
  result.value.dispose();
}
