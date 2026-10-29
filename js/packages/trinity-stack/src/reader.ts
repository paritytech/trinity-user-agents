// Keeps a project's stacks current, as docs/design/trinity-stack-library.md describes: it runs
// from the copies its build bundled, and a fetch only refreshes them. The trinity-stack crate's
// `StackReader` makes every decision; this module fetches, stores and keeps time.
import type {
  Chain,
  CurrentStack,
  Descriptor,
  Dotns,
  DotnsRecord,
  FetchRequest,
  Pointer,
  Service,
} from "../wasm/trinity_stack.js";
import { type DotnsTransport, readDotns } from "./dotns.js";
import { type Fetch, type Store, fetchServed, stackId } from "./index.js";
import { unixSeconds } from "./time.js";
import { wasm } from "./wasm.js";

/** A stack a build bundled, as `trinity-stack update` generates it. */
export type Bundled = {
  pointer: Pointer;
  /** The signed document, as served. */
  document: string;
  /** The stack's dotNS record, null when the build has none. */
  dotns: DotnsRecord | null;
};

/** When the reader fetches a stack it holds a copy of. */
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
   * The copy to use. It waits only for a stack the reader holds no copy of, and under `"once"`
   * for the stack's first fetch. Throws naming what is missing when the stack lacks a chain or
   * service `require` lists.
   */
  get(
    stack: Pointer | Bundled,
    options?: { require?: Requirements },
  ): Promise<CurrentStack>;
  /**
   * The stack's dotNS contracts for the descriptor in use, undefined when its Asset Hub has no
   * gateway. A lookup that fails leaves the record of the same `generation` in use.
   */
  dotns(stack: Pointer | Bundled): Promise<Dotns | undefined>;
  /** Fetches the stack now, whatever the policy, and returns the copy it then uses. */
  refresh(stack: Pointer | Bundled): Promise<CurrentStack>;
  /** Calls `listener` whenever a stack moves to a newer descriptor; returns the unsubscribe. */
  onChange(listener: (change: StackChange) => void): () => void;
  /** Stops background refreshes. */
  close(): void;
};

/**
 * A reader over the copies `bundled` holds, typically the `stacks` of the module
 * `trinity-stack update` generates. It never fetches per request: `refresh` says when it does.
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
    /** How to reach a stack's Asset Hub for dotNS, instead of its RPC and `eth-rpc`. */
    transport?: (descriptor: Descriptor) => DotnsTransport;
    now?: () => Date;
  } = {},
): StackReader {
  const policy = options.refresh ?? "once";
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
  const pointers = new Map<string, Pointer>();
  const loads = new Map<string, Promise<void>>();
  const fetches = new Map<string, Promise<CurrentStack>>();
  const lookups = new Map<string, Promise<DotnsRecord | undefined>>();
  const listeners = new Set<(change: StackChange) => void>();
  const storeKey = (id: string) => `${id}#reader`;

  /** Starts the stack from its build copy and stored record, once. */
  const load = (pointer: Pointer): Promise<void> => {
    const id = stackId(pointer);
    let loading = loads.get(id);
    if (!loading) {
      pointers.set(id, pointer);
      loading = (async () => {
        const build = bundled.get(id);
        reader.load(
          pointer,
          build && new TextEncoder().encode(build.document),
          build?.dotns ?? undefined,
          await options.store?.get(storeKey(id)),
        );
      })();
      loads.set(id, loading);
    }
    return loading;
  };

  /** Keeps `promise` as the stack's pending one in `pending` until it settles. */
  const track = <T>(
    pending: Map<string, Promise<T>>,
    id: string,
    promise: Promise<T>,
  ) => {
    pending.set(id, promise);
    const settle = () => {
      if (pending.get(id) === promise) pending.delete(id);
    };
    promise.then(settle, settle);
    return promise;
  };

  const fetchStack = (pointer: Pointer, request: FetchRequest) => {
    const id = stackId(pointer);
    return track(
      fetches,
      id,
      (async () => {
        let served;
        try {
          served = await fetchServed(request.url, {
            fetch: options.fetch,
            timeoutMs: options.timeoutMs,
            ifNoneMatch: request.ifNoneMatch,
          });
        } catch (error) {
          // A custom fetch's own bug: answer the reader so the stack does not stay pending.
          try {
            reader.fetched(pointer, undefined, undefined, false, now());
          } catch {}
          throw error;
        }
        const { accepted, record } = reader.fetched(
          pointer,
          served.document,
          served.etag,
          served.notModified,
          now(),
        );
        if (record) await options.store?.set(storeKey(id), record);
        if (accepted.status === "updated")
          for (const listener of listeners)
            listener({
              pointer,
              descriptor: accepted.descriptor,
              newGeneration: accepted.newGeneration,
            });
        const { descriptor, signed, stale, expired } = accepted;
        return { descriptor, signed, stale, expired };
      })(),
    );
  };

  /**
   * Runs a lookup for `descriptor`. Resolves to the record now in use, undefined when the reader
   * has moved to another generation meanwhile.
   */
  const lookUp = (pointer: Pointer, descriptor: Descriptor) => {
    const id = stackId(pointer);
    return track(
      lookups,
      id,
      (async () => {
        let record;
        try {
          record = await readDotns(descriptor, {
            transport: options.transport?.(descriptor),
            fetch: options.fetch,
            timeoutMs: options.timeoutMs,
          });
        } catch (error) {
          const kept = reader.lookedUp(pointer, undefined, now()).record;
          if (kept) return kept;
          throw error;
        }
        const looked = reader.lookedUp(pointer, record, now());
        if (looked.stored)
          await options.store?.set(storeKey(id), looked.stored);
        return looked.record;
      })(),
    );
  };

  const api: StackReader = {
    async get(stack, getOptions = {}) {
      const pointer = pointerOf(stack);
      await load(pointer);
      const read = reader.get(pointer, now());
      const fetching = read.fetch
        ? fetchStack(pointer, read.fetch)
        : fetches.get(stackId(pointer));
      const current = read.current ?? (await fetching);
      if (!current) throw new Error(`no copy of ${pointer.url} and no fetch`);
      if (getOptions.require) check(current.descriptor, getOptions.require);
      return current;
    },
    async dotns(stack) {
      const pointer = pointerOf(stack);
      // Again whenever the descriptor moved to another generation while a lookup ran.
      for (;;) {
        const { descriptor } = await api.get(stack);
        const read = reader.dotns(pointer, now());
        const looking = read?.lookUp
          ? lookUp(pointer, descriptor)
          : lookups.get(stackId(pointer));
        const record = read?.record ?? (await looking);
        if (record) return record.dotns;
        if (!looking)
          throw new Error(`no dotNS record of ${pointer.url} and no lookup`);
      }
    },
    async refresh(stack) {
      const pointer = pointerOf(stack);
      await load(pointer);
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
              const pointer = pointers.get(id);
              if (pointer) api.refresh(pointer).catch(() => {});
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
