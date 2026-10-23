// The worker's one TrUAPI client and the small waits built on it.
//
// product-sdk-host owns the client: its papi provider routes chain traffic
// through the same client, and an executable may hold only one. The package
// resolves `@parity/truapi` to this repository's build (see `overrides` in
// package.json), which is what carries `fundingProvider`.

import { getTruApi } from "@parity/product-sdk-host";
import type { ObservableLike, TrUApiClient } from "@parity/truapi";

let client: Promise<TrUApiClient> | undefined;

/** The worker's TrUAPI client. */
export function truapi(): Promise<TrUApiClient> {
  client ??= getTruApi().then((found) => {
    if (!found) throw new Error("fundtest worker: no host connection");
    return found;
  });
  return client;
}

/** The value of a host call's `ResultAsync`, or a thrown `HostCallError`
 *  naming `what`. */
export async function unwrap<T, E>(
  result: {
    match<A, B>(ok: (value: T) => A, err: (error: E) => B): PromiseLike<A | B>;
  },
  what: string,
): Promise<T> {
  const settled = await result.match(
    (value) => ({ ok: true as const, value }),
    (error) => ({ ok: false as const, error }),
  );
  if (settled.ok) return settled.value;
  throw new HostCallError(what, settled.error);
}

/** A host call that answered with an error. */
export class HostCallError extends Error {
  constructor(
    readonly what: string,
    readonly reason: unknown,
  ) {
    super(`${what} failed: ${describe(reason)}`);
    this.name = "HostCallError";
  }

  /** The tag of the error's innermost tagged value, such as `OutOfOrder`. */
  get tag(): string | undefined {
    let value: unknown = this.reason;
    let tag: string | undefined;
    while (value && typeof value === "object" && "tag" in value) {
      tag = String((value as { tag: unknown }).tag);
      value = (value as { value?: unknown }).value;
    }
    return tag;
  }
}

/** A readable rendering of any value, bigints included. */
export function describe(value: unknown): string {
  if (value instanceof Error) return value.message;
  try {
    return JSON.stringify(value, (_key, item) =>
      typeof item === "bigint" ? item.toString() : item,
    );
  } catch {
    return String(value);
  }
}

/** Raised by a wait that its signal aborted. */
export class Aborted extends Error {
  constructor() {
    super("aborted");
    this.name = "Aborted";
  }
}

/** Resolves after `ms`, or rejects with `Aborted` when `signal` fires first. */
export function sleep(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) return reject(new Aborted());
    const timer = setTimeout(() => {
      signal?.removeEventListener("abort", onAbort);
      resolve();
    }, ms);
    const onAbort = () => {
      clearTimeout(timer);
      reject(new Aborted());
    };
    signal?.addEventListener("abort", onAbort, { once: true });
  });
}

/** The first item of `stream` that `pick` maps to a value. Rejects when the
 *  stream fails or ends first, or `signal` fires. */
export function firstOf<T, R, E>(
  stream: ObservableLike<T, E>,
  pick: (item: T) => R | undefined,
  signal?: AbortSignal,
): Promise<R> {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) return reject(new Aborted());
    let done = false;
    const finish = (settle: () => void) => {
      if (done) return;
      done = true;
      signal?.removeEventListener("abort", onAbort);
      queueMicrotask(() => subscription.unsubscribe());
      settle();
    };
    const onAbort = () => finish(() => reject(new Aborted()));
    const subscription = stream.subscribe({
      next: (item) => {
        const picked = pick(item);
        if (picked !== undefined) finish(() => resolve(picked));
      },
      error: (error) =>
        finish(() => reject(new HostCallError("subscription", error))),
      complete: () => finish(() => reject(new Error("subscription ended"))),
    });
    signal?.addEventListener("abort", onAbort, { once: true });
  });
}

/** Error tags a host answers when a call can never succeed as made: a
 *  refusal of the request itself, not a failure along the way. */
const REFUSALS = new Set([
  "AlreadyExists",
  "Denied",
  "InsufficientBalance",
  "InvalidAmount",
  "InvalidSource",
  "InvalidUrl",
  "MalformedFrame",
  "NotFound",
  "NotStarted",
  "OutOfOrder",
  "Rejected",
  "TooLarge",
  "Unsupported",
]);

/** Whether `error` may clear by itself, so the call that raised it is worth
 *  making again: a host failure or a chain or network error, but not a host
 *  refusal or a programming error. */
export function isTransient(error: unknown): boolean {
  if (error instanceof HostCallError) return !REFUSALS.has(error.tag ?? "");
  return !(
    error instanceof TypeError ||
    error instanceof RangeError ||
    error instanceof ReferenceError ||
    error instanceof SyntaxError
  );
}
