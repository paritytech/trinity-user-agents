// Keeps a project's stacks current, as docs/design/trinity-stack-library.md describes: it runs
// from the copies its build bundled, and a fetch only refreshes them. The trinity-stack crate's
// `StackReader` makes every decision; this module fetches, stores and tracks time.
import type {
  Chain,
  CurrentStack,
  Descriptor,
  FetchRequest,
  Pointer,
  Service,
} from "../wasm/trinity_stack.js";
import { DEFAULT_TIMEOUT_MS, type Fetch, fetchServed } from "./fetch.js";
import { type Store, stackId } from "./index.js";
import { unixSeconds } from "./time.js";
import { wasm } from "./wasm.js";

/** A stack a build bundled, as `trinity-stack update` generates it. */
export type Bundled = {
  pointer: Pointer;
  /** The signed document, as served. */
  document: string;
};

/** When the reader fetches a stack it has a copy of. `maxAgeMs` is at least 1000. */
export type RefreshPolicy = "never" | "once" | { maxAgeMs: number };

/** A stack the reader moved to a newer descriptor. */
export type StackChange = {
  pointer: Pointer;
  descriptor: Descriptor;
  /** The chains restarted: reconnect and drop cached chain data. */
  newGeneration: boolean;
};

/** The chain roles and service kinds a project needs a stack to have. */
export type Requirements = { chains?: string[]; services?: string[] };

export type StackReader = {
  /**
   * The copy to use. It waits only for a stack the reader has no copy of, and under `"once"`
   * for the stack's first fetch. Throws naming what is missing when the stack lacks a chain or
   * service `require` lists.
   */
  get(
    stack: Pointer | Bundled,
    options?: { require?: Requirements },
  ): Promise<CurrentStack>;
  /** Fetches the stack now, whatever the policy, and returns the copy it then uses. */
  refresh(stack: Pointer | Bundled): Promise<CurrentStack>;
  /** Calls `listener` whenever a stack moves to a newer descriptor; returns the unsubscribe. */
  onChange(listener: (change: StackChange) => void): () => void;
  /** Stops background refreshes. */
  close(): void;
};

/**
 * A reader over the copies in `bundled`, typically the `stacks` of the module
 * `trinity-stack update` generates, and any `Bundled` a call passes. It never fetches per
 * request: `refresh` decides when it does.
 */
export function createStackReader(
  options: {
    bundled?: Record<string, Bundled> | Bundled[];
    /** `"once"` by default. */
    refresh?: RefreshPolicy;
    /** This project's own store, so a restart starts from the last copy it fetched. */
    store?: Store;
    fetch?: Fetch;
    timeoutMs?: number;
    now?: () => Date;
  } = {},
): StackReader {
  const policy = options.refresh ?? "once";
  if (
    typeof policy === "object" &&
    !(Number.isFinite(policy.maxAgeMs) && policy.maxAgeMs >= 1000)
  )
    throw new TypeError("maxAgeMs must be a finite number of at least 1000");
  const reader =
    typeof policy === "string"
      ? new wasm.StackReader(policy)
      : new wasm.StackReader("maxAge", policy.maxAgeMs / 1000);
  const now = () => unixSeconds(options.now?.() ?? new Date());
  const bundled = new Map(
    Object.values(options.bundled ?? {}).map((stack) => [
      stackId(stack.pointer),
      stack,
    ]),
  );
  /** Each stack the reader has started, with the pointer it was started from. */
  const loads = new Map<string, { pointer: Pointer; loading: Promise<void> }>();
  const fetches = new Map<string, Promise<CurrentStack>>();
  const listeners = new Set<(change: StackChange) => void>();
  const storeKey = (id: string) => `${id}#reader`;

  /** Starts the stack from its build copy and stored record, once; a start that failed runs again. */
  const load = (stack: Pointer | Bundled): Promise<void> => {
    const pointer = pointerOf(stack);
    const id = stackId(pointer);
    let started = loads.get(id);
    if (!started) {
      const build =
        bundled.get(id) ?? ("document" in stack ? stack : undefined);
      const loading = (async () => {
        reader.load(
          pointer,
          build && new TextEncoder().encode(build.document),
          await options.store?.get(storeKey(id)),
        );
      })();
      started = { pointer, loading };
      loads.set(id, started);
      loading.catch(() => loads.delete(id));
    }
    return started.loading;
  };

  const fetchStack = (pointer: Pointer, request: FetchRequest) => {
    const id = stackId(pointer);
    const fetching = (async () => {
      let served;
      try {
        const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
        const deadline = Date.now() + timeoutMs;
        served = await fetchServed(request.url, {
          fetch: options.fetch,
          timeoutMs,
          ifNoneMatch: request.ifNoneMatch,
        });
        // A browser cannot send `If-None-Match` to a server whose CORS does not allow it, so a
        // conditional fetch that failed is retried at once without it, within the same timeout.
        const remainingMs = deadline - Date.now();
        if (
          request.ifNoneMatch &&
          !served.document &&
          !served.notModified &&
          remainingMs > 0
        )
          served = await fetchServed(request.url, {
            fetch: options.fetch,
            timeoutMs: remainingMs,
          });
      } catch (error) {
        // A custom fetch's own bug: answer the reader so the stack does not stay pending.
        try {
          reader.fetched(pointer, undefined, undefined, false, now());
        } catch {}
        throw error;
      }
      const { accepted, stored } = reader.fetched(
        pointer,
        served.document,
        served.etag,
        served.notModified,
        now(),
      );
      if (accepted.status === "updated")
        for (const listener of listeners) {
          try {
            listener({
              pointer,
              descriptor: accepted.descriptor,
              newGeneration: accepted.newGeneration,
            });
          } catch (error) {
            report(error);
          }
        }
      // The reader already uses the new copy, so a store that fails costs only the next start.
      if (stored)
        try {
          await options.store?.set(storeKey(id), stored);
        } catch (error) {
          report(error);
        }
      const { descriptor, signed, stale, expired } = accepted;
      return { descriptor, signed, stale, expired };
    })();
    fetches.set(id, fetching);
    const forget = () => {
      if (fetches.get(id) === fetching) fetches.delete(id);
    };
    fetching.then(forget, forget);
    return fetching;
  };

  const api: StackReader = {
    async get(stack, getOptions = {}) {
      const pointer = pointerOf(stack);
      await load(stack);
      const read = reader.get(pointer, now());
      const fetching = read.fetch
        ? fetchStack(pointer, read.fetch)
        : fetches.get(stackId(pointer));
      const current = read.current ?? (await fetching);
      if (!current) throw new Error(`no copy of ${pointer.url} and no fetch`);
      if (getOptions.require) check(current.descriptor, getOptions.require);
      return current;
    },
    async refresh(stack) {
      const pointer = pointerOf(stack);
      await load(stack);
      const request = reader.refresh(pointer, now());
      return request
        ? fetchStack(pointer, request)
        : (fetches.get(stackId(pointer)) ?? api.get(stack));
    },
    onChange(listener) {
      listeners.add(listener);
      return () => void listeners.delete(listener);
    },
    close() {
      clearInterval(timer);
    },
  };

  const timer =
    typeof policy === "object"
      ? setInterval(
          () => {
            for (const id of reader.due(now())) {
              const started = loads.get(id);
              if (started) api.refresh(started.pointer).catch(report);
            }
          },
          Math.min(policy.maxAgeMs, 60_000),
        )
      : undefined;
  // A timer alone must not keep a Node process running.
  (timer as { unref?: () => void } | undefined)?.unref?.();
  return api;
}

/** The chain with `role`. Throws when the stack has none. */
export function chain(descriptor: Descriptor, role: string): Chain {
  const found = descriptor.chains.find((chain) => chain.role === role);
  if (!found) throw new Error(`${descriptor.id} has no chain ${role}`);
  return found;
}

/** The service of `kind`. Throws when the stack has none. */
export function service(descriptor: Descriptor, kind: string): Service {
  const found = descriptor.services.find((service) => service.kind === kind);
  if (!found) throw new Error(`${descriptor.id} has no service ${kind}`);
  return found;
}

function check(descriptor: Descriptor, require: Requirements) {
  const missing = [
    ...(require.chains ?? [])
      .filter((role) => !descriptor.chains.some((chain) => chain.role === role))
      .map((role) => `chain ${role}`),
    ...(require.services ?? [])
      .filter(
        (kind) => !descriptor.services.some((service) => service.kind === kind),
      )
      .map((kind) => `service ${kind}`),
  ];
  if (missing.length > 0)
    throw new Error(`${descriptor.id} has no ${missing.join(", ")}`);
}

function pointerOf(stack: Pointer | Bundled): Pointer {
  return "document" in stack ? stack.pointer : stack;
}

/** Reports an error nothing awaits, as a browser reports an event listener's error. */
function report(error: unknown) {
  if (typeof reportError === "function") reportError(error);
  else console.error(error);
}
