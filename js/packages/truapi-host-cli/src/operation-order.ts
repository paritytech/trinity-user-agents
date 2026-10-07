// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

// WORKAROUND for a causal-ordering hazard in the chain-head relay, measured
// to SURVIVE the Rust core (its TS-host-era header blamed a package that is
// deleted, yet the identical inversion reproduces).
//
// papi learns a chain-head operation's `operationId` ONLY from the
// `chainHead_v1_storage`/`_body`/`_call` start-response
// (`{result:{result:"started",operationId}}`): substrate-client registers the
// per-operation subscriber inside that response's `onSuccess`, then routes
// events through a manager whose dispatch is
//
//     next(id, data) { subscriptions.get(id)?.next(data) }
//
// so an operation event that arrives BEFORE its start-response is dropped on
// the floor (silently, thanks to the `?.`) and the read it belongs to never
// settles. Measured against the core (tier 8): papi hung 3 runs in 6 with a
// 155-request/12s retry storm, and inversion count correlated with the hang
// 6/6. With this shim wrapping the provider: 6/6 clean.
//
// The shim restores the invariant on the consumer side, where it is cheap and
// transport-agnostic: hold back events whose `operationId` has not been
// announced yet, and release them the moment it is. It belongs on the PRODUCT
// side, wrapping the papi `JsonRpcProvider` that speaks to the core.

/**
 * Structurally compatible with polkadot-api's `JsonRpcProvider` (which passes
 * parsed message objects), without making papi a dependency of this package.
 */
export type JsonRpcProvider = (onMessage: (message: unknown) => void) => {
  send: (message: unknown) => void;
  disconnect: () => void;
};

type RpcMessage = unknown;

/** Operation events after which an `operationId` is retired by the spec. */
const TERMINAL_EVENTS = new Set([
  "operationBodyDone",
  "operationCallDone",
  "operationStorageDone",
  "operationError",
  "operationInaccessible",
]);

/** The `operationId` a start-response announces, if this message is one. */
function announcedOperationId(message: RpcMessage): string | undefined {
  const result = (message as { result?: unknown }).result;
  if (result === null || typeof result !== "object") return undefined;
  const started = result as { result?: unknown; operationId?: unknown };
  return started.result === "started" && typeof started.operationId === "string"
    ? started.operationId
    : undefined;
}

/** The follow-event payload, if this message is a `chainHead_v1_followEvent`. */
function followEvent(
  message: RpcMessage,
): { event?: unknown; operationId?: unknown } | undefined {
  if ((message as { method?: unknown }).method !== "chainHead_v1_followEvent") {
    return undefined;
  }
  return (
    message as {
      params?: { result?: { event?: unknown; operationId?: unknown } };
    }
  ).params?.result;
}

/**
 * Wrap a `JsonRpcProvider` so a chain-head operation's events can never reach
 * the consumer before the start-response that names the operation. Messages
 * without an `operationId` (`initialized`, `newBlock`, `finalized`, plain
 * responses) pass straight through, so ordering is only ever adjusted where
 * it is load-bearing.
 */
export function serializeOperationStarts(
  provider: JsonRpcProvider,
): JsonRpcProvider {
  return (onMessage) => {
    const announced = new Set<string>();
    const queued = new Map<string, RpcMessage[]>();
    // Whether a follow is currently live. After a `stop`, papi's
    // substrate-client has torn the follow down and rejects any operation it
    // is told about (`onSubscribeOperation` errors with DisjointError while
    // followSubscription is null), so a start-response still in flight from
    // the dead follow registers NO subscriber. Announcing its id anyway would
    // un-gate the NEXT operation that reuses that number (substrate hands
    // out small per-follow counters, so the refollow's first storage op is
    // very likely to be "1" again) and papi would silently drop its events.
    // That is precisely the stall this shim exists to prevent. `initialized`
    // is the first event of every follow, so it marks the point from which
    // start-responses can be trusted again. Starts true: the opening follow
    // has not been stopped.
    let followLive = true;

    return provider((message) => {
      const startedId = announcedOperationId(message);
      if (startedId !== undefined) {
        // Forward it regardless (papi decides what to do with it), but only
        // treat it as an announcement while a follow is live.
        onMessage(message);
        if (!followLive) return;
        announced.add(startedId);
        const pending = queued.get(startedId);
        if (pending !== undefined) {
          queued.delete(startedId);
          for (const event of pending) onMessage(event);
        }
        return;
      }

      const event = followEvent(message);
      if (event === undefined) {
        onMessage(message);
        return;
      }

      // A `stop` ends the follow, so every operation under it is dead: drop
      // the bookkeeping (and any still-unannounced events, which can never be
      // delivered) so a refollow starts clean.
      if (event.event === "stop") {
        followLive = false;
        announced.clear();
        queued.clear();
        onMessage(message);
        return;
      }

      const operationId = event.operationId;
      if (typeof operationId !== "string") {
        // Any non-operation follow event (`initialized`, `newBlock`,
        // `bestBlockChanged`, `finalized`) can only come from a LIVE follow,
        // so it ends the post-stop dead zone. Keyed on the whole class rather
        // than `initialized` alone: if a refollow's `initialized` were ever
        // missed, start-responses would stop being honoured and every
        // operation would queue forever, trading one hang for another.
        followLive = true;
        onMessage(message);
        return;
      }

      if (!announced.has(operationId)) {
        const pending = queued.get(operationId);
        if (pending === undefined) queued.set(operationId, [message]);
        else pending.push(message);
        return;
      }

      onMessage(message);
      // Retiring the id on its terminal event keeps both maps bounded over a
      // long-lived client instead of growing once per operation.
      if (typeof event.event === "string" && TERMINAL_EVENTS.has(event.event)) {
        announced.delete(operationId);
      }
    });
  };
}
