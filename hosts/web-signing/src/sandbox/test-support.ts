import type { Locks } from "./ports.js";

/** Grants each name to one holder at a time, in request order, as Web Locks do. */
export function serialLocks(): Locks {
  const tails = new Map<string, Promise<unknown>>();
  return {
    request: ((name: string, callback: () => unknown) => {
      const run = (tails.get(name) ?? Promise.resolve()).then(callback);
      tails.set(
        name,
        run.catch(() => undefined),
      );
      return run;
    }) as Locks["request"],
  };
}

/**
 * A lock manager that grants everything at once, on the spot: what a tab sees
 * without Web Locks, where another tab's work can land in the middle of its own.
 */
export function noLocks(): Locks {
  return {
    request: ((_name: string, callback: () => unknown) => {
      try {
        return Promise.resolve(callback());
      } catch (error) {
        return Promise.reject(error);
      }
    }) as Locks["request"],
  };
}
