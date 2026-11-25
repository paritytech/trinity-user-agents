import { type Observable, type Subscription } from 'rxjs';

/**
 * Consume an Observable as an async iterable — the shape a caller needs when handing a
 * live stream to an API that pulls (`for await`, a generator-style host callback).
 *
 * An explicit iterator, not an `async function*`, on purpose. A puller disposes by
 * calling `iterator.return()`, and a generator only unwinds its `finally` when suspended
 * at a `yield`. A pull loop calls `next()` right after each item, so the generator would
 * usually be parked at an `await` instead, where `return()` is queued behind a promise
 * that only settles on the next emission — leaving the subscription live after disposal,
 * indefinitely if the source never emits again. Owning `return()` makes teardown
 * synchronous. Values that arrive before the consumer pulls are buffered, so none drop.
 *
 * The iterable ends when the source completes or errors; map a source error to a value
 * with `catchError` upstream if the consumer needs to see it rather than just stop.
 */
export function observableToAsyncIterable<T>(source$: Observable<T>): AsyncIterable<T> {
  return {
    [Symbol.asyncIterator]: () => {
      // Boxed so `queue.shift()` distinguishes "a buffered value" from "empty" without
      // assuming `T` is truthy — the box is always truthy, `T` may be `0`/`''`/`false`.
      const queue: { value: T }[] = [];
      let deliver: Nullable<(result: IteratorResult<T>) => void> = null;
      // A synchronous source (`of`, an already-settled `combineLatest`) completes during
      // `subscribe`, before `subscription` is assigned, so `close` must tolerate it being
      // unset — RxJS tears a synchronously-completed subscription down on its own.
      let subscription: Nullable<Subscription> = null;
      let closed = false;

      const push = (value: T) => {
        if (closed) return;

        const waiting = deliver;
        if (waiting) {
          deliver = null;
          waiting({ value, done: false });
        } else {
          queue.push({ value });
        }
      };

      const close = () => {
        if (!closed) {
          closed = true;
          subscription?.unsubscribe();
        }

        const waiting = deliver;
        if (waiting) {
          deliver = null;
          waiting({ value: undefined, done: true });
        }
      };

      subscription = source$.subscribe({ next: push, error: close, complete: close });

      return {
        next: () => {
          // Drain buffered values before reporting done: a synchronous source finishes
          // with its last emission still queued, and that value must not be lost.
          const item = queue.shift();
          if (item) return Promise.resolve<IteratorResult<T>>({ value: item.value, done: false });

          if (closed) return Promise.resolve<IteratorResult<T>>({ value: undefined, done: true });

          return new Promise<IteratorResult<T>>(resolve => {
            deliver = resolve;
          });
        },
        return: () => {
          close();

          return Promise.resolve<IteratorResult<T>>({ value: undefined, done: true });
        },
      };
    },
  };
}
