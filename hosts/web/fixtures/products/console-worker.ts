/**
 * A product worker for the browser test of the Workers inspector. It writes
 * to its console from its first statement, logs values that a console must
 * bound, floods the console, leaves a rejection unhandled and throws an
 * uncaught error, and opens one TrUAPI subscription so its frames show in
 * the TrUAPI view. What it saw of its own console calls is on
 * `window.__consoleWorker`.
 */
import { getClientSync } from "@parity/truapi/sandbox";

console.log("worker booting", { step: 1 });
const loop: Record<string, unknown> = { name: "loop" };
loop.self = loop;
console.warn("circular", loop);
console.info("binary", new Uint8Array(4096));
console.error("bad thing", new Error("first failure"));
const returned = console.log("returns");
for (let index = 0; index < 120; index += 1) console.debug("flood", index);

const client = getClientSync();
client?.chat.actionSubscribe().subscribe({ next() {} });

(window as unknown as { __consoleWorker: unknown }).__consoleWorker = {
  returned: returned === undefined,
  hasClient: client !== null,
};

void Promise.reject(new Error("nobody waits"));
setTimeout(() => {
  throw new Error("uncaught later");
}, 0);
