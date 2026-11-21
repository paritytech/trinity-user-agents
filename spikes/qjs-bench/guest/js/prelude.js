// Worker profile subset for M0: timers, console, clock, randomness, UTF-8
// codecs and the frame transport. Captures the raw host bindings in this
// closure and removes them from the global object.
(() => {
  const host = globalThis.__host;
  delete globalThis.__host;

  const LEVEL = { debug: 0, info: 1, warn: 2, error: 3 };
  const format = (args) =>
    args
      .map((arg) => {
        if (typeof arg === "string") return arg;
        if (arg instanceof Error) return `${arg.name}: ${arg.message}\n${arg.stack ?? ""}`;
        try {
          return JSON.stringify(arg);
        } catch {
          return String(arg);
        }
      })
      .join(" ");
  globalThis.console = Object.freeze({
    debug: (...args) => host.log(LEVEL.debug, format(args)),
    log: (...args) => host.log(LEVEL.info, format(args)),
    info: (...args) => host.log(LEVEL.info, format(args)),
    warn: (...args) => host.log(LEVEL.warn, format(args)),
    error: (...args) => host.log(LEVEL.error, format(args)),
  });

  const timers = new Map();
  let nextTimerId = 1;
  const schedule = (callback, delay, args, repeat) => {
    const id = nextTimerId++;
    const ms = Math.max(0, Number(delay) || 0);
    timers.set(id, { callback, args, repeat: repeat ? ms : null });
    host.timerSet(id, ms);
    return id;
  };
  const cancel = (id) => {
    if (timers.delete(id)) host.timerClear(id);
  };
  globalThis.setTimeout = (callback, delay, ...args) => schedule(callback, delay, args, false);
  globalThis.setInterval = (callback, delay, ...args) => schedule(callback, delay, args, true);
  globalThis.clearTimeout = cancel;
  globalThis.clearInterval = cancel;
  globalThis.queueMicrotask = (callback) => Promise.resolve().then(callback);
  globalThis.__truapi_fire_timer = (id) => {
    const timer = timers.get(id);
    if (!timer) return;
    if (timer.repeat === null) timers.delete(id);
    else host.timerSet(id, timer.repeat);
    timer.callback(...timer.args);
  };

  globalThis.performance = Object.freeze({ now: () => host.now() });

  globalThis.crypto = Object.freeze({
    getRandomValues(array) {
      const bytes = host.randomBytes(array.byteLength);
      new Uint8Array(array.buffer, array.byteOffset, array.byteLength).set(bytes);
      return array;
    },
  });

  globalThis.TextEncoder = class TextEncoder {
    get encoding() {
      return "utf-8";
    }
    encode(text = "") {
      return host.utf8Encode(String(text));
    }
  };
  globalThis.TextDecoder = class TextDecoder {
    get encoding() {
      return "utf-8";
    }
    decode(input) {
      if (input === undefined) return "";
      const bytes = input instanceof Uint8Array ? input : new Uint8Array(input.buffer ?? input, input.byteOffset ?? 0, input.byteLength);
      return host.utf8Decode(bytes);
    }
  };

  const frameListeners = new Set();
  globalThis.__truapi_deliver = (frame) => {
    for (const listener of frameListeners) listener(frame);
  };
  globalThis.__truapi_frames = Object.freeze({
    send: (frame) => host.frameOut(frame),
    subscribe(listener) {
      frameListeners.add(listener);
      return () => frameListeners.delete(listener);
    },
  });
})();
