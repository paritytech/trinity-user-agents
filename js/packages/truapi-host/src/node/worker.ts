// Entry point of the worker thread `createNodePairingHostRuntime` starts. It
// gives the thread the Web Worker globals `worker-runtime` uses, loads the core
// from disk, then runs the same `worker-runtime` a browser host runs.

import { readFileSync } from "node:fs";
import { parentPort } from "node:worker_threads";

import { initSync } from "../wasm/web/truapi_server.js";

if (!parentPort)
  throw new Error(
    "@parity/truapi-host/node: worker.js must run in a worker thread",
  );
const port = parentPort;

const events = new EventTarget();
port.on("message", (data) =>
  events.dispatchEvent(new MessageEvent("message", { data })),
);
port.on("messageerror", () =>
  events.dispatchEvent(new MessageEvent("messageerror")),
);

// A browser worker only logs an unhandled rejection; a Node thread would die.
process.on("unhandledRejection", (reason) => {
  console.error("[truapi worker] unhandled rejection:", reason);
});

// `self` is the thread's own global, so the core still finds `crypto` and the
// rest of what it reads there.
Object.assign(globalThis, {
  self: globalThis,
  postMessage: (message: unknown, transfer?: Transferable[]) =>
    port.postMessage(message, transfer as never),
  addEventListener: events.addEventListener.bind(events),
  removeEventListener: events.removeEventListener.bind(events),
});

// `worker-runtime` initialises the glue with no argument, which would fetch the
// `.wasm` by URL. Initialising it from disk first turns that call into a no-op.
initSync({
  module: readFileSync(
    new URL("../wasm/web/truapi_server_bg.wasm", import.meta.url),
  ),
});

await import("../worker-runtime.js");
