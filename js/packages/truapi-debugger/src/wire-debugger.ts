// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: MIT
/**
 * Trace engine: group a stream of observed frames into per-op traces.
 *
 * The host tap streams every product↔host frame to the debugger, where
 * {@link createDebugIngest} decodes each into an {@link ObservedFrame} keyed on
 * the wire `requestId`. This module turns that stream into a usable surface:
 *
 *  - {@link createWireDebugger} accumulates frames into per-`requestId` traces
 *    so a single op can be reconstructed across product → wire → host;
 *  - the same `requestId` is the value product-sdk telemetry spans correlate on
 *    (`HostOpEvent.correlationId`), so a frame trace and a product span line up
 *    under one id with no extra plumbing;
 *  - it logs/relays each frame and never touches decoded payloads, so it works
 *    against any host without knowing the application protocol.
 *
 * @module
 */

import type { FrameRole, ObservedFrame, TransportObserver } from "./observed-frame.js";

/**
 * What a trace's retention caps dropped, counted per axis.
 *
 * {@link WireTrace.truncated} collapses all of this to a boolean, which cannot
 * tell "one frame lost" from "seventy-seven lost", nor which cap did it. The
 * counts are the honest signal: `framesByCount` and `framesByBytes` are frames
 * that no longer exist in the trace, `payloadsShed` frames that are still there
 * with their metadata but without their payload bytes.
 */
export interface TraceDropCounts {
  /** Frames evicted to stay under {@link WireDebuggerOptions.maxFramesPerTrace}. */
  framesByCount: number;
  /** Frames evicted to stay under {@link WireDebuggerOptions.maxBytesPerTrace}. */
  framesByBytes: number;
  /**
   * Frames retained but stripped of their bytes because a single payload
   * exceeded the whole byte budget. The frame, its `frameId` and its
   * `byteLength` survive; only the bytes are gone, so no frame is *missing*
   * from the sequence on this axis.
   */
  payloadsShed: number;
}

/**
 * A single op's frames, in arrival order, grouped by their shared
 * `(channelId, requestId)`. `requestId` alone is not unique across channels -
 * each host mints its own `p:1`, `p:2`, … - so the channel is part of a trace's
 * identity.
 */
export interface WireTrace {
  /** Product channel this op belongs to, e.g. `"myapp.dot"`. */
  channelId: string;
  /**
   * Correlation id shared by every frame in this trace. A product may recycle it
   * for a later, unrelated call; {@link WireTrace.generation} disambiguates the
   * successive ops that then share it.
   */
  requestId: string;
  /** Frames observed for this id, in the order they crossed the transport. */
  frames: ObservedFrame[];
  /** Epoch ms of the first frame. */
  startedAt: number;
  /** Epoch ms of the most recent frame. */
  lastAt: number;
  /**
   * Which reuse of `(channelId, requestId)` this op is, from `0`. A fresh opener
   * (`request`/`start`) arriving after the id's current op already opened starts
   * the next generation, so a recycled id never merges two unrelated calls.
   */
  generation: number;
  /**
   * Whether anything was dropped from this trace to stay under the frame or byte
   * cap: the boolean collapse of {@link WireTrace.dropped}. Surfaced as a
   * `truncated` op badge so the operator can tell "older frames dropped" from a
   * genuinely short op.
   */
  truncated: boolean;
  /**
   * Per-axis counts behind {@link WireTrace.truncated}: how many frames each cap
   * dropped, and how many payloads were shed. Lets a mount report "77 frames
   * dropped (byte cap)" instead of a bare "truncated".
   */
  dropped: TraceDropCounts;
}

/** Sink for fully-formatted debug lines (defaults to `console.debug`). */
export type WireDebugSink = (line: string, frame: ObservedFrame) => void;

/**
 * A method's wire shape: request/response, or subscription (which covers both
 * plain and result subscriptions - both share the same four-leg wire shape).
 * This is the one piece of shape carried on the routing table itself; combined
 * with a frame's own `messageType` byte, it is enough to resolve a
 * {@link FrameRole} without decoding the payload.
 */
export type WireMethodKind = "request" | "subscription";

/** Resolution of a bare wire `frameId` to its human-readable method. */
export interface WireMethodInfo {
  /** Dotted method path as it appears on the client, e.g. `"account.getAccount"`. */
  method: string;
  /** The method's wire envelope shape. */
  kind: WireMethodKind;
}

/** `camelCase` → `CONST_CASE`, matching the wire-table's constant naming. */
function constCase(name: string): string {
  return name.replace(/([a-z0-9])([A-Z])/g, "$1_$2").toUpperCase();
}

/** `GET_ACCOUNT` → `getAccount`. */
function camelCase(constName: string): string {
  const [head, ...rest] = constName.toLowerCase().split("_");
  return (
    (head ?? "") +
    rest.map((w) => w.charAt(0).toUpperCase() + w.slice(1)).join("")
  );
}

/** One generated wire-table entry: `{ trait, method, kind }` satisfying `MethodIds`. */
interface WireTableEntry {
  trait: number;
  method: number;
  kind: WireMethodKind;
}

function isWireTableEntry(value: unknown): value is WireTableEntry {
  return (
    value !== null &&
    typeof value === "object" &&
    typeof (value as WireTableEntry).trait === "number" &&
    typeof (value as WireTableEntry).method === "number" &&
    (value as WireTableEntry).kind !== undefined
  );
}

/**
 * The `frameId` a `(trait, method)` pair resolves to across the debugger,
 * matching the generated `WIRE_DECODE_TABLE`'s own keying (`trait * 256 +
 * method`) so a frame's `frameId` and its decode-table entry are always the
 * same lookup.
 */
export function frameIdOf(trait: number, method: number): number {
  return trait * 256 + method;
}

/**
 * Resolve a frame's lifecycle `role` from its method's static wire
 * {@link WireMethodKind} and its own `messageType` byte - the wire's own leg
 * marker (`Payload.messageType`), read directly with no payload decode at all.
 *
 * `messageType` alone cannot name the leg, because its first two values are
 * shared across kinds: 0 is a request's `Request` and a subscription's
 * `Start`, 1 is a `Response` and a `Receive`. `kind` is what resolves that.
 * A request's `Cancel` (4) and a subscription's remaining values are
 * unambiguous (`Interrupt` 2, `Stop` 3). Returns `"unknown"` when `kind` is
 * unset (an off-table id) or `messageType` is out of range for that kind.
 *
 * `cancel` names a frame inside an op rather than one that ends it: the call
 * it names still closes on its own `response`.
 */
export function resolveRole(
  messageType: number,
  kind: WireMethodKind | undefined,
): FrameRole {
  if (kind === undefined) return "unknown";
  if (kind === "request") {
    if (messageType === 0) return "request";
    if (messageType === 1) return "response";
    if (messageType === 4) return "cancel";
    return "unknown";
  }
  switch (messageType) {
    case 0:
      return "start";
    case 1:
      return "receive";
    case 2:
      return "interrupt";
    case 3:
      return "stop";
    default:
      return "unknown";
  }
}

/**
 * Build a reverse map from wire `frameId` (see {@link frameIdOf}) to
 * `"service.method"` name and wire {@link WireMethodKind}, out of the generated
 * wire-table module and the client's service names.
 *
 * Each wire-table export is a flat `{ trait, method, kind }` record (see the
 * generated `MethodIds`); the service list - typically
 * `Object.keys(createClient(transport))` - disambiguates where the service
 * prefix ends (`LOCAL_STORAGE_READ` → `localStorage.read`, not
 * `local.storageRead`). Non-entry exports in `table` are ignored, so the whole
 * `import * as W from "./generated/wire-table.js"` namespace can be passed
 * directly.
 */
export function createMethodNameMap(
  table: Record<string, unknown>,
  services: readonly string[],
): ReadonlyMap<number, WireMethodInfo> {
  // Longest prefix first, so RESOURCE_ALLOCATION_ wins over a hypothetical RESOURCE_.
  const prefixes = services
    .map((service) => ({ service, prefix: `${constCase(service)}_` }))
    .sort((a, b) => b.prefix.length - a.prefix.length);

  const map = new Map<number, WireMethodInfo>();
  for (const [constName, entry] of Object.entries(table)) {
    if (!isWireTableEntry(entry)) continue;
    const match = prefixes.find(({ prefix }) => constName.startsWith(prefix));
    const method = match
      ? `${match.service}.${camelCase(constName.slice(match.prefix.length))}`
      : camelCase(constName);
    map.set(frameIdOf(entry.trait, entry.method), { method, kind: entry.kind });
  }
  return map;
}

/** Options for {@link createWireDebugger}. */
export interface WireDebuggerOptions {
  /**
   * Where formatted frame lines go. Defaults to `console.debug`. A host-side
   * panel (e.g. dotli's wire-debug view) passes its own sink here to render the
   * stream live.
   */
  sink?: WireDebugSink;
  /**
   * Optional forward target: a second observer to receive every frame after it
   * is recorded. Lets a host relay frames onward (to a panel, a socket, an OTel
   * exporter) while the debugger keeps its own per-id traces.
   */
  forward?: TransportObserver;
  /** Cap on retained traces (LRU-evicted). Default 256. */
  maxTraces?: number;
  /**
   * Cap on retained frames within a single trace (oldest ring-buffered out).
   * Default 1024. Without this a long-lived subscription - e.g.
   * `account.connectionStatus`, which shares one `requestId` for the whole
   * session - accumulates a frame per `receive` forever, since all its frames
   * share a `requestId` that never LRU-evicts from {@link maxTraces}. A panel
   * showing the last N frames of a subscription is no worse than one showing
   * all of them.
   */
  maxFramesPerTrace?: number;
  /**
   * Cap on retained payload bytes across a trace's *evictable* frames - every
   * frame but the opener. Only bites when the ingest retains bytes (level-2
   * decode); with decode off, frames carry no bytes and this never triggers.
   * Without it, a burst of large payloads sharing one long-lived `requestId`
   * grows memory unbounded even under {@link maxFramesPerTrace} (count-capped,
   * not byte-capped). A single frame whose own payload exceeds the cap has its
   * bytes shed (metadata + byteLength kept); otherwise oldest non-opener frames
   * are evicted until under budget. Default 1 MiB.
   *
   * The opener is never evicted (located by role via `openerIndexOf`, not
   * assumed to be index 0) - pairing (`orphaned`) and
   * retry-storm both key on it - so its bytes are excluded from this budget
   * rather than charged against it. Charging an un-evictable frame's bytes to a
   * budget the eviction loop then tries to reclaim makes the loop evict frames
   * that are not the problem: a 700B request plus a 400B response under a 1000B
   * cap would evict the response of a *completed* op, and an opener whose own
   * payload equals the cap would evict every frame that ever follows it. Bytes
   * held by a trace are therefore bounded by the opener's own payload (itself
   * capped by the shedding rule) plus this cap, not by this cap alone.
   */
  maxBytesPerTrace?: number;
  /**
   * Reverse map from wire `frameId` to method name (build one with
   * {@link createMethodNameMap}). When set, formatted lines carry
   * `account.getAccount` instead of a bare `id=22`.
   */
  methodNames?: ReadonlyMap<number, WireMethodInfo>;
}

/** A live wire debugger: an `observe` hook plus per-`(channelId, requestId)` trace lookup. */
export interface WireDebugger {
  /** The callback that records a frame; drive it from {@link createDebugIngest}. */
  readonly observe: TransportObserver;
  /** All retained traces across all channels, most-recently-active last. */
  traces(): WireTrace[];
  /**
   * The current (latest-generation) trace for a `requestId`. Pass `channelId` to
   * disambiguate when more than one host is connected (each mints the same `p:N`
   * ids); without it, the most-recently-active op matching `requestId` is returned
   * - fine for a single-host session or product-span (`correlationId`) correlation.
   */
  trace(
    requestId: string,
    channelId?: string,
    generation?: number,
  ): WireTrace | undefined;
  /** All retained traces for one channel, most-recently-active last. */
  tracesForChannel(channelId: string): WireTrace[];
  /**
   * Count of whole operations LRU-evicted since the last {@link clear}. Distinct
   * from per-op frame truncation ({@link WireTrace.truncated}): whole-op eviction
   * is otherwise invisible because {@link traces} shows only survivors, so this
   * is how a consumer tells "kept 256 of 10k" from "only 256 ever happened".
   */
  evictedTraces(): number;
  /** Drop all retained traces. */
  clear(): void;
}

function formatFrame(
  frame: ObservedFrame,
  methodNames?: ReadonlyMap<number, WireMethodInfo>,
): string {
  const arrow = frame.direction === "out" ? "→" : "←";
  const method = methodNames?.get(frame.frameId)?.method;
  const label = method ? `${frame.role} ${method}` : frame.role;
  return `[wire ${frame.requestId}] ${arrow} ${label} (id=${frame.frameId}, ${frame.byteLength}B)`;
}

/**
 * Build a {@link WireDebugger}. Feed its {@link WireDebugger.observe} from
 * {@link createDebugIngest} to start recording. Frames are logged through
 * `sink`, forwarded through `forward` (if set), and grouped into
 * per-`requestId` {@link WireTrace}s for correlation with product-sdk spans.
 */
export function createWireDebugger(
  options: WireDebuggerOptions = {},
): WireDebugger {
  const sink: WireDebugSink = options.sink ?? ((line) => console.debug(line));
  const forward = options.forward;
  // Floor every cap at 1. A cap of 0 (or negative) evicts nothing - `splice(1, n)`
  // has no index 1 to remove - while still counting a drop per frame, so the
  // badge climbs forever against a trace that never lost anything. The embed
  // forwards caller-supplied caps straight through, so this is reachable.
  const atLeastOne = (value: number | undefined, fallback: number): number =>
    value === undefined || !Number.isFinite(value) || value < 1
      ? fallback
      : Math.floor(value);
  const maxTraces = atLeastOne(options.maxTraces, 256);
  const maxFramesPerTrace = atLeastOne(options.maxFramesPerTrace, 1024);
  const maxBytesPerTrace = atLeastOne(options.maxBytesPerTrace, 1024 * 1024);
  const methodNames = options.methodNames;
  // Insertion-ordered; re-inserting on activity keeps the map LRU-ordered.
  // Keyed by `(channelId, requestId)` since requestId is per-channel only.
  const traces = new Map<string, WireTrace>();
  /**
   * The one frame per trace that retention must never drop: the opener if one was
   * observed, else index 0 as a stand-in so a trace can always keep an anchor.
   *
   * Uses `isOpener`, the SAME resolution `rotate` uses, deliberately: the ingest
   * leaves `role` as "unknown" (lifecycle is not on the wire) and resolves it from
   * the frameId's wire-table kind. Reading the raw `role` instead would make this
   * disagree with rotate about which frame opened the op, and for a caller driving
   * `observe` without a `methodNames` table it would find no opener at all and
   * protect index 0 - the exact bug this function exists to fix.
   */
  function protectedIndexOf(frames: readonly ObservedFrame[]): number {
    const opener = frames.findIndex((f) => isOpener(f));
    return opener === -1 ? 0 : opener;
  }

  /**
   * Drop the oldest evictable frame in place, never the protected one, and return
   * it so the caller can account for its bytes. `undefined` when nothing but the
   * protected frame remains.
   *
   * Removes exactly one: both callers evict one at a time in their own loop, so
   * there is no multi-removal index bookkeeping to get wrong. Not a bulk
   * `splice(1, n)` either - the protected frame may sit anywhere, so this walks
   * from the oldest and steps over it rather than assuming it is at index 0.
   */
  function evictOldestSparing(
    frames: ObservedFrame[],
  ): ObservedFrame | undefined {
    if (frames.length <= 1) return undefined;
    const spared = protectedIndexOf(frames);
    for (let i = 0; i < frames.length; i++) {
      if (i === spared) continue;
      return frames.splice(i, 1)[0];
    }
    return undefined;
  }

  // Length-prefixed, so the separator cannot be forged from inside a component.
  // `channelId` and `requestId` are sender-controlled and only length-clamped,
  // never character-filtered, so a bare separator put the delimiter inside the
  // alphabet of the fields it separates: channel `a` + request `b<NUL>0` and
  // channel `a<NUL>b` + request `0` collided onto one key, merging a foreign
  // channel's frame into another channel's operation.
  const keyOf = (channelId: string, requestId: string): string =>
    `${String(channelId.length)}:${channelId}\u0000${String(requestId.length)}:${requestId}`;

  // `(channelId, requestId)` -> the gen-key of that id's current (latest) op.
  const current = new Map<string, string>();
  // Whole operations LRU-evicted since the last clear(). Surfaced so a session
  // that overflowed maxTraces doesn't silently under-report its op count.
  let evictedCount = 0;
  // A frame's lifecycle role. Ingest resolves it against the frame's own
  // `messageType` whenever it has a methodNames map (see resolveRole); a frame
  // can still arrive "unknown" (no map there, or an off-table id) — fall back
  // the same way wireTraceToView does, the same resolution — otherwise no
  // real frame ever reads as an opener.
  const roleOf = (f: ObservedFrame): string | undefined =>
    f.role !== "unknown"
      ? f.role
      : resolveRole(f.messageType, methodNames?.get(f.frameId)?.kind);
  // A frame that begins an operation: a unary request or a subscription start.
  const isOpener = (f: ObservedFrame): boolean => {
    const r = roleOf(f);
    return r === "request" || r === "start";
  };

  const observe: TransportObserver = (frame) => {
    const baseKey = keyOf(frame.channelId, frame.requestId);
    const curKey = current.get(baseKey);
    const cur = curKey !== undefined ? traces.get(curKey) : undefined;

    // A fresh opener for an id whose current trace already holds ANY frame means
    // the product recycled the requestId: rotate to a new generation so the two
    // never merge.
    //
    // Rotating only when the current trace already OPENED merged two unrelated
    // ops. A tap attaching mid-session sees the tail of an op that predates it, so
    // the current trace often holds a closer and no opener - and a closer with no
    // opener PROVES that op is over. Appending the next op's request to it reported
    // one operation that never existed, with a duration spanning both (4.1s for a
    // 100ms call) folded into `avgDurationMs` for the whole session, and put the
    // `unpaired` badge - which means "the debugger attached late" - on an op that
    // was fully observed.
    //
    // That is the failure this engine separates `orphaned` from `unpaired` to
    // avoid: attributing the debugger's own blind spot to the host, here by
    // fabricating a number rather than merely mislabelling one. Rotating instead
    // yields a gen-0 trace holding the unpairable tail and a gen-1 trace that is
    // honest about itself - two claims about the debugger's view, both visible.
    //
    // The inverse risk - a closer arriving before its OWN opener, splitting one op
    // in two - needs a producer that reorders frames within a channel. The host's
    // replay queue is FIFO (`queue.push` / `queue.splice(0)` in
    // `@parity/truapi-host`'s worker runtime), so no shipped path can do it.
    const rotate = cur !== undefined && isOpener(frame) && cur.frames.length > 0;

    let trace: WireTrace;
    let key: string;
    if (curKey !== undefined && cur !== undefined && !rotate) {
      traces.delete(curKey); // re-insert below to keep the map LRU-ordered
      trace = cur;
      key = curKey;
    } else {
      const generation = cur === undefined ? 0 : cur.generation + 1;
      key = `${baseKey}\0${String(generation)}`;
      trace = {
        channelId: frame.channelId,
        requestId: frame.requestId,
        generation,
        frames: [],
        startedAt: frame.timestamp,
        lastAt: frame.timestamp,
        truncated: false,
        dropped: {
          framesByCount: 0,
          framesByBytes: 0,
          payloadsShed: 0,
        },
      };
    }
    trace.frames.push(frame);
    if (trace.frames.length > maxFramesPerTrace) {
      // Evict oldest to keep an exact hard cap, but NEVER the opener: it is the
      // request/start the pairing (`orphaned`) and retry-storm signals key on, so
      // dropping it would falsely orphan a long-lived subscription (e.g.
      // account.connectionStatus).
      //
      // Keyed on the opener's ACTUAL index, not `frames[0]`. Both mounts attach
      // mid-session, so the first frame observed for an id is often a closer for a
      // request that predates the tap; the opener then sits at index >= 1, which is
      // exactly where the previous `splice(1, excess)` evicted from - so protecting
      // index 0 protected a stray response and dropped the opener it meant to save.
      while (trace.frames.length > maxFramesPerTrace) {
        if (evictOldestSparing(trace.frames) === undefined) break;
        trace.dropped.framesByCount += 1;
      }
    }
    // Byte cap: only bites when bytes are retained (level-2 decode).
    if (frame.bytes !== undefined && maxBytesPerTrace !== Infinity) {
      // A single frame whose own payload exceeds the whole budget can never fit;
      // shed its bytes (keeping its metadata + byteLength) rather than evict every
      // other frame around it. The opener is not exempt: pairing/retry-storm key
      // on the opener's frameId, so the frame stays — only its bytes are dropped.
      for (const f of trace.frames) {
        if ((f.bytes?.length ?? 0) > maxBytesPerTrace) {
          f.bytes = undefined;
          trace.dropped.payloadsShed += 1;
        }
      }
      // Budget the evictable frames only (everything but the opener). The opener
      // can never be evicted, so charging its bytes to a budget this loop reclaims
      // from would evict frames that are not the cause — up to and including the
      // response of an already-completed op, or every frame after an opener that is
      // itself at the cap. Evict oldest-first until the evictable frames are under
      // budget. Same opener-index rule as the count cap above.
      const spared = protectedIndexOf(trace.frames);
      let retained = 0;
      for (let i = 0; i < trace.frames.length; i++) {
        if (i === spared) continue;
        retained += trace.frames[i].bytes?.length ?? 0;
      }
      while (retained > maxBytesPerTrace && trace.frames.length > 1) {
        const evicted = evictOldestSparing(trace.frames);
        if (evicted === undefined) break;
        retained -= evicted.bytes?.length ?? 0;
        trace.dropped.framesByBytes += 1;
      }
    }
    // `truncated` is the boolean collapse of the per-axis counts, so the two can
    // never disagree.
    trace.truncated =
      trace.dropped.framesByCount +
        trace.dropped.framesByBytes +
        trace.dropped.payloadsShed >
      0;
    trace.lastAt = frame.timestamp;
    traces.set(key, trace);
    current.set(baseKey, key);

    while (traces.size > maxTraces) {
      const oldest = traces.keys().next().value;
      if (oldest === undefined) break;
      const evicted = traces.get(oldest);
      traces.delete(oldest);
      evictedCount += 1;
      // If the evicted op was an id's current, forget it so reuse starts clean.
      if (evicted !== undefined) {
        const bk = keyOf(evicted.channelId, evicted.requestId);
        if (current.get(bk) === oldest) current.delete(bk);
      }
    }

    try {
      sink(formatFrame(frame, methodNames), frame);
    } catch {
      // A debug sink must never break the observed transport.
    }
    if (forward) {
      try {
        forward(frame);
      } catch {
        // A forward target must never break the observed transport.
      }
    }
  };

  return {
    observe,
    traces: () => [...traces.values()],
    trace: (requestId, channelId, generation) => {
      // A specific generation (drill-down into one op of a recycled id).
      if (generation !== undefined) {
        for (const t of traces.values()) {
          if (
            t.requestId === requestId &&
            t.generation === generation &&
            (channelId === undefined || t.channelId === channelId)
          ) {
            return t;
          }
        }
        return undefined;
      }
      // The current (latest) generation for this id.
      if (channelId !== undefined) {
        const key = current.get(keyOf(channelId, requestId));
        return key !== undefined ? traces.get(key) : undefined;
      }
      // No channel given: the most recent op matching this requestId. Iterate in
      // LRU order and keep the last match, so a reused id resolves to its latest op.
      let match: WireTrace | undefined;
      for (const t of traces.values()) {
        if (t.requestId === requestId) match = t;
      }
      return match;
    },
    tracesForChannel: (channelId) =>
      [...traces.values()].filter((t) => t.channelId === channelId),
    evictedTraces: () => evictedCount,
    clear: () => {
      traces.clear();
      current.clear();
      evictedCount = 0;
    },
  };
}
