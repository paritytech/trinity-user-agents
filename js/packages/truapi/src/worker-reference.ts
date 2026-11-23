import type { ObservableLike, Subscription } from "./transport.js";

/** A reference on the calling worker. Releasing it twice is a no-op. */
export interface WorkerReference {
  release(): void;
}

/** Resolve once the host has acquired the reference. Abort cancels a pending acquisition. */
export function retainWorker(
  acknowledgements: ObservableLike<undefined, unknown>,
  signal?: AbortSignal,
): Promise<WorkerReference> {
  return new Promise((resolve, reject) => {
    signal?.throwIfAborted();
    let subscription: Subscription | undefined;
    let released = false;

    function release() {
      if (released) return;
      released = true;
      signal?.removeEventListener("abort", abort);
      subscription?.unsubscribe();
    }

    function abort() {
      release();
      reject(signal?.reason);
    }

    signal?.addEventListener("abort", abort, { once: true });
    try {
      subscription = acknowledgements.subscribe({
        next() {
          signal?.removeEventListener("abort", abort);
          resolve({ release });
        },
        error(error) {
          release();
          reject(error);
        },
        complete() {
          release();
          reject(new Error("Worker reference ended before acknowledgement"));
        },
      });
      if (released) subscription.unsubscribe();
    } catch (error) {
      release();
      reject(error);
    }
  });
}
