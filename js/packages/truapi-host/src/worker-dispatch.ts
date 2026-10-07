import { errorMessage } from "./error.js";
import type { WorkerToMain } from "./worker-protocol.js";

type PostToMain = (msg: WorkerToMain) => void;

export interface SubscriptionListeners {
  sendItem: (value: unknown) => void;
  sendError: (error: string) => void;
  /** Private backend data and failures must never reach diagnostic logs. */
  privateMedia?: boolean;
}

function reportDispatchFailure(
  postToMain: PostToMain,
  label: string,
  err: unknown,
): void {
  postToMain({
    kind: "disposeError",
    error: `${label} callback failed: ${errorMessage(err)}`,
  });
}

export function dispatchSubscriptionItem(
  subId: number,
  value: unknown,
  listeners: Map<number, SubscriptionListeners>,
  postToMain: PostToMain,
): void {
  const listener = listeners.get(subId);
  if (!listener) return;
  try {
    listener.sendItem(value);
  } catch (err) {
    listeners.delete(subId);
    postToMain({ kind: "subscriptionStop", subId });
    reportDispatchFailure(
      postToMain,
      `subscription ${subId}`,
      listener.privateMedia ? new Error("media backend failure") : err,
    );
  }
  if (listener.privateMedia) {
    postToMain({ kind: "mediaSubscriptionAck", subId });
  }
}

export function dispatchSubscriptionError(
  subId: number,
  error: string,
  listeners: Map<number, SubscriptionListeners>,
  postToMain: PostToMain,
): void {
  const listener = listeners.get(subId);
  if (!listener) return;
  if (listener.privateMedia) {
    listeners.delete(subId);
    postToMain({ kind: "subscriptionStop", subId });
  }
  try {
    listener.sendError(error);
  } catch (err) {
    listeners.delete(subId);
    postToMain({ kind: "subscriptionStop", subId });
    reportDispatchFailure(
      postToMain,
      `subscription ${subId} error`,
      listener.privateMedia ? new Error("media backend failure") : err,
    );
  }
}

export function dispatchChainResponse(
  connId: number,
  json: string,
  listeners: Map<number, (json: string) => void>,
  postToMain: PostToMain,
): void {
  const listener = listeners.get(connId);
  if (!listener) return;
  try {
    listener(json);
  } catch {
    listeners.delete(connId);
    postToMain({ kind: "chainClose", connId });
    // Response-handler errors may contain private HOP data.
    postToMain({
      kind: "disposeError",
      error: `JSON-RPC connection ${connId} callback failed`,
    });
  }
}
