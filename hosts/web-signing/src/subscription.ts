import { ok } from "neverthrow";
import type { GenericError, Result } from "@parity/truapi";

/**
 * A host-side subscription the core reads as an async iterable.
 *
 * `start` runs when the core begins reading. It receives `emit` and returns
 * the teardown that runs when the core cancels, which is the only way these
 * streams end: the core owns cancellation.
 */
export function subscription<T>(
  start: (emit: (value: T) => void) => () => void,
): AsyncIterable<Result<T, GenericError>> {
  return {
    [Symbol.asyncIterator](): AsyncIterator<Result<T, GenericError>> {
      const queued: Result<T, GenericError>[] = [];
      let waiting:
        | ((result: IteratorResult<Result<T, GenericError>>) => void)
        | null = null;
      let closed = false;

      const stop = start((value) => {
        if (closed) return;
        const item = ok<T, GenericError>(value);
        if (waiting) {
          const resolve = waiting;
          waiting = null;
          resolve({ value: item, done: false });
        } else {
          queued.push(item);
        }
      });

      return {
        next() {
          const item = queued.shift();
          if (item) return Promise.resolve({ value: item, done: false });
          if (closed) return Promise.resolve({ value: undefined, done: true });
          return new Promise((resolve) => {
            waiting = resolve;
          });
        },
        return() {
          if (!closed) {
            closed = true;
            stop();
            waiting?.({ value: undefined, done: true });
            waiting = null;
          }
          return Promise.resolve({ value: undefined, done: true });
        },
      };
    },
  };
}

/** A subscription that emits `value` once and then stays open. */
export function constant<T>(value: T): AsyncIterable<Result<T, GenericError>> {
  return subscription((emit) => {
    emit(value);
    return () => {};
  });
}
