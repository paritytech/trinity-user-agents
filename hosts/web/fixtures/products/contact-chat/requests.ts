import {
  PRODUCT_HELLO,
  PRODUCT_PROTOCOL,
  type CallResult,
  type HostMessage,
  type ProductCall,
  type ProductHello,
} from "../../../src/invitations/contract.js";

/**
 * DEV ONLY. A product's end of the proposed host request service
 * (`src/invitations/contract.ts`): its calls, each answered with one result,
 * and what the host tells it unasked.
 */

/** A call without its `id`, which this end adds. */
export type RequestCall = ProductCall extends infer Call
  ? Call extends { id: number }
    ? Omit<Call, "id">
    : never
  : never;

export interface HostRequests {
  call(call: RequestCall): Promise<CallResult>;
  /** Called with every host message that is not a result, in order. */
  onMessage(listener: (message: HostMessage) => void): void;
}

/** The service over `port`. Messages that arrive before a listener are kept for it. */
export function hostRequestsOver(port: MessagePort): HostRequests {
  let nextId = 1;
  const calls = new Map<number, (result: CallResult) => void>();
  const early: HostMessage[] = [];
  let listener: ((message: HostMessage) => void) | null = null;
  port.onmessage = (event: MessageEvent<HostMessage>) => {
    const message = event.data;
    if (message?.kind === "result") {
      calls.get(message.id)?.(message.result);
      calls.delete(message.id);
    } else if (listener === null) early.push(message);
    else listener(message);
  };
  port.start();
  return {
    call(call) {
      const id = nextId++;
      return new Promise((resolve) => {
        calls.set(id, resolve);
        port.postMessage({ ...call, id });
      });
    },
    onMessage(next) {
      listener = next;
      for (const message of early.splice(0)) next(message);
    },
  };
}

/** Hand the host this frame's port for the request service, and use it. */
export function connectHostRequests(
  parent: Window = window.parent,
  hostOrigin: string = location.origin,
): HostRequests {
  const channel = new MessageChannel();
  const requests = hostRequestsOver(channel.port1);
  parent.postMessage(
    { type: PRODUCT_HELLO, protocol: PRODUCT_PROTOCOL } satisfies ProductHello,
    hostOrigin,
    [channel.port2],
  );
  return requests;
}
