// Globals the macOS `jsc` shell lacks, so the workload wrappers run there:
//   jsc [--useJIT=false] src/jsc-shim.js dist/w-codec.js
// TextEncoder/TextDecoder are plain JS here; QuickJS gets native ones, so this
// slightly disadvantages JSC on the codec workload.
(() => {
  const g = globalThis;
  g.console = { log: (...a) => print(a.join(" ")), info: (...a) => print(a.join(" ")), warn: (...a) => print(a.join(" ")), error: (...a) => print(a.join(" ")), debug() {} };
  g.performance = { now: () => preciseTime() * 1000 };
  g.queueMicrotask = (callback) => Promise.resolve().then(callback);
  const timers = new Map();
  let nextId = 1;
  g.setTimeout = (callback, delay, ...args) => {
    const id = nextId++;
    timers.set(id, true);
    Promise.resolve().then(() => timers.delete(id) && callback(...args));
    return id;
  };
  g.clearTimeout = (id) => timers.delete(id);
  g.setInterval = () => 0;
  g.clearInterval = () => {};
  g.TextEncoder = class {
    encode(text = "") {
      const out = [];
      for (const char of String(text)) {
        let code = char.codePointAt(0);
        if (code < 0x80) out.push(code);
        else if (code < 0x800) out.push(0xc0 | (code >> 6), 0x80 | (code & 63));
        else if (code < 0x10000) out.push(0xe0 | (code >> 12), 0x80 | ((code >> 6) & 63), 0x80 | (code & 63));
        else out.push(0xf0 | (code >> 18), 0x80 | ((code >> 12) & 63), 0x80 | ((code >> 6) & 63), 0x80 | (code & 63));
      }
      return new Uint8Array(out);
    }
  };
  g.TextDecoder = class {
    decode(input) {
      if (!input) return "";
      const bytes = ArrayBuffer.isView(input)
        ? new Uint8Array(input.buffer, input.byteOffset, input.byteLength)
        : new Uint8Array(input);
      let text = "";
      for (let index = 0; index < bytes.length; ) {
        const byte = bytes[index++];
        let code = byte;
        if (byte >= 0xf0) code = ((byte & 7) << 18) | ((bytes[index++] & 63) << 12) | ((bytes[index++] & 63) << 6) | (bytes[index++] & 63);
        else if (byte >= 0xe0) code = ((byte & 15) << 12) | ((bytes[index++] & 63) << 6) | (bytes[index++] & 63);
        else if (byte >= 0xc0) code = ((byte & 31) << 6) | (bytes[index++] & 63);
        text += String.fromCodePoint(code);
      }
      return text;
    }
  };
  g.crypto = { getRandomValues: (array) => { for (let i = 0; i < array.length; i++) array[i] = (Math.random() * 256) | 0; return array; } };
  g.__truapi_frames = { send() {}, subscribe: () => () => {} };
})();
