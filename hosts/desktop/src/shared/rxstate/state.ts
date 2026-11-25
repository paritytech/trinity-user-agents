import { type InteropObservable, type Observable, BehaviorSubject, distinctUntilChanged } from 'rxjs';

import { onExecutionEnvironmentReset } from '@/shared/execution-environment';

export type RxState<T> = InteropObservable<T> & {
  value$: Observable<T>;
  set(value: T | ((prev: T) => T)): Observable<T>;
  /** Returns to `initial`, through the same serialized queue `set` uses. */
  reset(): void;
  getInitial(): T;
  pipe: Observable<T>['pipe'];
  get(): T;
};

export function createState<T>(initial: T, comparator?: (a: T, b: T) => boolean): RxState<T> {
  const state$ = new BehaviorSubject(initial);
  const derived$ = state$.pipe(distinctUntilChanged(comparator));

  // Emissions are serialized because a subscriber may write back to the state it is
  // reacting to. A nested `next` would otherwise run to completion inside the outer
  // one: subscribers registered after the writer receive the nested value and then,
  // as the outer emission resumes, the stale one — so their last value is the value
  // the write was meant to replace, while `get()` reports the new one. A flag raised
  // from inside a subscription silently reverts for every later subscriber, which is
  // invisible until subscription order changes.
  //
  // Updates are resolved at emission time, not at `set` time, so a second write
  // queued behind the first still sees the first's result as its `prev`.
  //
  // The one visible consequence: a write made from inside a subscription is not in
  // `get()` on the very next line — it lands once the current emission drains. Read
  // the value a subscriber was handed rather than reaching back for `get()` there.
  let emitting = false;
  const pending: (T | ((prev: T) => T))[] = [];

  const emit = (value: T | ((prev: T) => T)) => {
    pending.push(value);
    if (emitting) return;

    emitting = true;
    try {
      // `for..of` over an array observes entries appended while it iterates, which is
      // exactly what a subscriber writing back during `next` adds.
      for (const [index, update] of pending.entries()) {
        try {
          state$.next(update instanceof Function ? update(state$.value) : update);
        } catch (error) {
          // The first entry is the caller's own write — its updater throwing is the
          // caller's error to see. A queued entry came from a subscriber, and the
          // writer that started the drain is a stranger to it: its throw must neither
          // surface there nor drop the writes queued behind it.
          if (index === 0) throw error;
          console.error('[rxstate] a queued update threw; later queued writes still applied', error);
        }
      }
    } finally {
      pending.length = 0;
      emitting = false;
    }
  };

  function reset(): void {
    emit(initial);
  }

  // Every state returns to its initial value when the environment resets — under
  // test, after each case. In the app nothing ever emits, so this stays inert.
  // The subscription is held for the state's lifetime and every `createState` in
  // the app is a module singleton; a state created per instance would have to
  // unsubscribe when it is dropped, which nothing here does.
  onExecutionEnvironmentReset(reset);

  return {
    value$: derived$,
    pipe: derived$.pipe.bind(derived$),
    get: () => state$.value,
    set(value) {
      emit(value);

      return derived$;
    },
    reset,
    getInitial() {
      return initial;
    },
    [Symbol.observable]() {
      return derived$;
    },
  };
}
