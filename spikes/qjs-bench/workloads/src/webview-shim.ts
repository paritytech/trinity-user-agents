// E1: gives a hidden WKWebView the same __truapi_frames channel the QuickJS
// prelude provides. Outbound frames go to the `truapiFrame` script message
// handler as arrays of bytes; Swift delivers inbound frames by calling
// `__truapi_deliver_bytes([...])` through evaluateJavaScript. Results and logs
// go to the `truapiLog` handler.
import type { FrameChannel } from "./engine-globals";

interface ScriptMessageHandler {
  postMessage(message: unknown): void;
}
declare global {
  var webkit: { messageHandlers: Record<string, ScriptMessageHandler> };
  var __truapi_deliver_bytes: (bytes: number[]) => void;
}

const handlers = globalThis.webkit.messageHandlers;
const listeners = new Set<(frame: Uint8Array) => void>();

const channel: FrameChannel = {
  send: (frame) => handlers.truapiFrame.postMessage(Array.from(frame)),
  subscribe(listener) {
    listeners.add(listener);
    return () => listeners.delete(listener);
  },
};
globalThis.__truapi_frames = channel;
globalThis.__truapi_deliver_bytes = (bytes) => {
  const frame = Uint8Array.from(bytes);
  for (const listener of [...listeners]) listener(frame);
};

const forward =
  (level: string) =>
  (...args: unknown[]) =>
    handlers.truapiLog.postMessage({ level, message: args.map(String).join(" ") });
console.log = forward("info");
console.warn = forward("warn");
console.error = forward("error");
