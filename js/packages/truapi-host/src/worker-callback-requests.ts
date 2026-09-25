import type { CallbackName } from "./generated/worker-callbacks.js";
import type { CallbackArgs, WorkerToMain } from "./worker-protocol.js";

type PostToMain = (msg: WorkerToMain) => void;

/** The outcome the main thread reports for one callback request. */
export type CallbackResult =
  | { ok: true; value: unknown }
  | { ok: false; error: string };

/** The worker's side of host callbacks that run on the main thread. */
export interface CallbackRequests {
  /**
   * Ask the main thread to run `name`. With a `signal` the request is a
   * prompt: aborting the signal withdraws it, and the main thread aborts the
   * signal it handed the host.
   */
  request(
    name: CallbackName,
    args: CallbackArgs,
    signal?: AbortSignal,
  ): Promise<unknown>;
  /** Settle a request with the main thread's answer; a withdrawn one ignores it. */
  settle(requestId: number, result: CallbackResult): void;
}

/** Track the callback requests this worker has in flight on the main thread. */
export function createCallbackRequests(
  postToMain: PostToMain,
): CallbackRequests {
  let nextRequestId = 0;
  const pending = new Map<number, (result: CallbackResult) => void>();
  return {
    request(name, args, signal) {
      return new Promise((resolve, reject) => {
        const requestId = ++nextRequestId;
        const withdraw = () => {
          if (!pending.delete(requestId)) return;
          postToMain({ kind: "callbackAbort", requestId });
          reject(new Error("withdrawn"));
        };
        pending.set(requestId, (r) => {
          signal?.removeEventListener("abort", withdraw);
          if (r.ok) resolve(r.value);
          else reject(new Error(r.error));
        });
        postToMain({
          kind: "callbackRequest",
          requestId,
          name,
          args,
          ...(signal && { withdrawable: true as const }),
        });
        signal?.addEventListener("abort", withdraw, { once: true });
      });
    },
    settle(requestId, result) {
      const settle = pending.get(requestId);
      if (!settle) return;
      pending.delete(requestId);
      settle(result);
    },
  };
}
