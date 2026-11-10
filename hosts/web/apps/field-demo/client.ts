import "./style.css";
import {
  FieldError,
  type FieldErrorReply,
} from "../../bridges/fields/protocol.js";

export { FieldError };

/** Only public responses are recorded, for this dev fixture's privacy checks. */
declare global {
  interface Window {
    __fieldDemoReceived: unknown[];
  }
}

/** Connect the fixture's declared bridge through its normal product frame. */
export async function connectFields(role: "provider" | "market") {
  if (parent === window) throw new Error("Open this URL inside the web host.");
  let nextId = 1;
  const waiting = new Map<
    number,
    { resolve(value: unknown): void; reject(error: Error): void }
  >();
  window.__fieldDemoReceived = [];
  let port: MessagePort | null = null;
  await new Promise<void>((resolve, reject) => {
    const timer = setTimeout(
      () =>
        reject(
          new Error(
            "The host did not attach the web-only demo bridge. Sign in, then reopen this product.",
          ),
        ),
      15_000,
    );
    // The host offers a port once this document has loaded. Registered before
    // the first await, so the offer cannot arrive before the listener.
    const onOffer = (event: MessageEvent<unknown>) => {
      const offer = event.data as { type?: unknown; protocol?: unknown } | null;
      if (
        event.source !== parent ||
        offer?.type !== "web-field-demo-offer" ||
        offer.protocol !== "web-field-demo/1" ||
        event.ports.length !== 1
      )
        return;
      window.removeEventListener("message", onOffer);
      port = event.ports[0]!;
      port.onmessage = (message: MessageEvent<unknown>) => {
        window.__fieldDemoReceived.push(message.data);
        if (window.__fieldDemoReceived.length > 100)
          window.__fieldDemoReceived.shift();
        const data = message.data as {
          protocol?: string;
          role?: string;
          id?: number;
          error?: FieldErrorReply;
          result?: unknown;
        };
        if (data.protocol === "web-field-demo/1") {
          clearTimeout(timer);
          if (data.role === role) resolve();
          else
            reject(
              new FieldError(
                "unsupported",
                "This app is not registered for that role.",
              ),
            );
          return;
        }
        if (data.id === undefined) return;
        const pending = waiting.get(data.id);
        waiting.delete(data.id);
        if (data.error !== undefined)
          pending?.reject(new FieldError(data.error.code, data.error.message));
        else pending?.resolve(data.result);
      };
    };
    window.addEventListener("message", onOffer);
  });
  return (request: Record<string, unknown>): Promise<unknown> =>
    new Promise((resolve, reject) => {
      const id = nextId++;
      const timer = setTimeout(() => {
        waiting.delete(id);
        reject(
          new FieldError(
            "network",
            "Host request timed out. Check the host sheet or test-network allowance.",
          ),
        );
      }, 180_000);
      waiting.set(id, {
        resolve: (value) => {
          clearTimeout(timer);
          resolve(value);
        },
        reject: (error) => {
          clearTimeout(timer);
          reject(error);
        },
      });
      port!.postMessage({ ...request, id });
    });
}

/** A required fixture element. */
export function element<T extends HTMLElement>(id: string): T {
  const found = document.getElementById(id);
  if (found === null) throw new Error("Missing fixture element.");
  return found as T;
}
