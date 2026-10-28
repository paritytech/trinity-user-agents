import type { CallMediaEngine } from "../../src/chat-v2/host.js";

/**
 * A call engine that opens no camera or microphone and sends no media. It
 * trades made-up signals, `synthetic-offer:<call>` and
 * `synthetic-answer:<call>`, so the call flow of the Chat Extension v2 draft
 * can be tried between a host and a product's own signaling without devices.
 * The host gives it only to the local chat product demo (`./local-demo.ts`).
 */

const encode = (text: string) => new TextEncoder().encode(text);

export const syntheticCallMedia: CallMediaEngine = {
  name: "synthetic",
  start({ callId, offerer }, events) {
    let state: "connecting" | "connected" = "connecting";
    let stopped = false;
    const connect = () => {
      state = "connected";
      events.changed();
    };
    if (offerer)
      queueMicrotask(() => {
        if (!stopped) events.signal(encode(`synthetic-offer:${callId}`));
      });
    return {
      get state() {
        return state;
      },
      local: null,
      remote: null,
      signal(bytes) {
        if (stopped) return;
        const text = new TextDecoder().decode(bytes);
        if (!offerer && text === `synthetic-offer:${callId}`) {
          events.signal(encode(`synthetic-answer:${callId}`));
          connect();
        } else if (offerer && text === `synthetic-answer:${callId}`) connect();
      },
      setMuted() {},
      setCamera() {},
      stop() {
        stopped = true;
      },
    };
  },
};
