import { Subject } from 'rxjs';

/**
 * Where the code is running. `'runtime'` is the app in any build; `'test'` is a
 * unit test, set once by `vitest.setup.js` before any spec file loads.
 */
export type ExecutionEnvironment = 'runtime' | 'test';

// A plain variable on purpose: an `RxState` here would reset itself on the first
// `resetExecutionEnvironment()` and switch every resource off its mock. Kept
// private with the subject below — `@/shared` exposes functions, not stores.
let environment: ExecutionEnvironment = 'runtime';
const reset$ = new Subject<void>();

export function setExecutionEnvironment(next: ExecutionEnvironment): void {
  environment = next;
}

export function getExecutionEnvironment(): ExecutionEnvironment {
  return environment;
}

export function isTestEnvironment(): boolean {
  return environment === 'test';
}

/**
 * Registers a listener for {@link resetExecutionEnvironment}. A stateful
 * primitive (an `RxState`, a resource) calls this once when it is created and
 * resets itself in the listener; the environment never learns what subscribed.
 * Returns the unsubscribe.
 */
export function onExecutionEnvironmentReset(listener: VoidFunction): VoidFunction {
  const subscription = reset$.subscribe(listener);

  return () => subscription.unsubscribe();
}

/**
 * Tells every subscriber to return to its initial state. `vitest.setup.js` calls
 * this after each test so no test inherits a value the previous one produced.
 */
export function resetExecutionEnvironment(): void {
  reset$.next();
}
