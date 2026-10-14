/**
 * A debug session: the trace engine wired to the ingest.
 *
 * A host dials the debugger and streams {@link DebugFrameEnvelope}s over a
 * socket; each is handed to {@link DebugSession.handleEnvelope}, decoded, and
 * grouped into per-`requestId` traces readable via {@link DebugSession.traces}.
 *
 * The socket itself is deliberately not here. The debugger app is a WS server
 * (hosts dial outward to it), but binding the socket is a thin edge: accept a
 * connection, JSON/CBOR-decode each message into a {@link DebugFrameEnvelope},
 * and call `handleEnvelope`. Keeping that edge out of this module lets the
 * session compile and unit-test without a socket transport or Node types.
 *
 * @module
 */

import {
  createWireDebugger,
  createMethodNameMap,
  type WireDebugger,
  type WireMethodInfo,
} from "./wire-debugger.js";
import { createDebugIngest, type DebugFrameEnvelope } from "./ingest.js";
import { createFrameDecoder, type FrameValueDetail } from "./decode.js";
import {
  isLiveSubscription,
  isSubscription,
  operationMethod,
  type TraceView,
} from "./trace-view.js";
import * as W from "@parity/truapi/wire-table";
import { createClient, createTransport } from "@parity/truapi";

/** A provider that sends and receives nothing; used only to enumerate service names. */
const NOOP_PROVIDER = {
  postMessage() {},
  subscribe() {
    return () => {};
  },
  dispose() {},
};

/** Options for {@link createDebugSession}. */
export interface DebugSessionOptions {
  /**
   * Turn on level-2 value decode in the drill-down detail path. On by default
   * (this is a dev-only tool that decodes everything). When on, the session
   * retains raw frame bytes so {@link DebugSession.frameDetail} can decode a
   * frame; `/traces` stays payload-blind regardless (it never reads bytes or
   * decoded values). When off, `frameDetail` reports byte length only.
   */
  decodeValues?: boolean;
  /**
   * Cap on retained operations, LRU-evicted (see
   * {@link WireDebuggerOptions.maxTraces}). Defaults to the engine's own default.
   * A mount that shares a tab with the observed app should lower it: the product
   * pays for whatever the panel retains.
   */
  maxTraces?: number;
  /**
   * Cap on retained frames within one operation (see
   * {@link WireDebuggerOptions.maxFramesPerTrace}). Defaults to the engine's own
   * default.
   */
  maxFramesPerTrace?: number;
  /**
   * Cap on retained payload bytes within one operation (see
   * {@link WireDebuggerOptions.maxBytesPerTrace}); only bites while
   * {@link DebugSessionOptions.decodeValues} retains bytes. Defaults to the
   * engine's own default.
   */
  maxBytesPerTrace?: number;
}

/** How many methods the busiest-methods roll-up reports. */
const TOP_METHOD_LIMIT = 5;

/** What the busiest-methods roll-up calls an op whose ids were all off-table. */
const UNKNOWN_METHOD = "(unknown)";

/**
 * Facts about a session that no single {@link TraceView} can carry, supplied by
 * the mount that owns the link: whole-op eviction, link-level drops, and whether
 * a feeding host's wire contract disagrees with this debugger's.
 */
export interface TraceStatsExtras {
  /** Whole operations LRU-evicted (`traceEngine.evictedTraces()`). */
  evictedTraces?: number;
  /** Frames the feeding host reported dropping before delivery. */
  droppedByHost?: number;
  /** Whether any feeding host declared a wire contract this debugger can't decode against. */
  codecMismatch?: boolean;
}

/**
 * The payload-blind aggregate roll-up behind a mount's summary strip: counts,
 * byte totals, durations, health tallies, the direction split, and the busiest
 * methods. Shape and timing only - never a byte or a decoded value.
 */
export interface TraceStats {
  ops: number;
  frames: number;
  bytes: number;
  subscriptions: number;
  liveSubscriptions: number;
  malformed: number;
  orphaned: number;
  /**
   * Ops carrying a closing frame with no opener in view. Reported separately from
   * `orphaned` and NOT as a warning: the common cause is the debugger attaching
   * mid-op, which is not a host fault. Counted rather than dropped because a real
   * double-answer lands here too and would otherwise appear in no aggregate at
   * all - only in one op's row.
   */
  unpaired: number;
  retryStorms: number;
  truncated: number;
  evictedTraces: number;
  droppedByHost: number;
  codecMismatch: boolean;
  out: number;
  in: number;
  avgDurationMs: number;
  maxDurationMs: number;
  topMethods: { method: string; count: number }[];
}

/**
 * Roll a set of {@link TraceView}s up into the summary strip's numbers.
 *
 * This is THE aggregate computation for every mount. A second implementation is
 * how the two mounts silently disagree about the same stream (one reporting
 * `malformed 1`, the other reporting no malformed at all), so the standalone
 * server's `/stats` and the in-app embed's strip both go through here rather than
 * each summing views their own way.
 *
 * `avgDurationMs` averages over ALL ops, not only completed ones. Note what that
 * does NOT mean: an op's span is `lastAt - startedAt`, i.e. first frame to last
 * frame OBSERVED, with no reference to now. A request that is still hanging has
 * one frame, so its span is 0 and it pulls the average DOWN - a stream full of
 * hung calls reads as a fast session here, even though the operation row renders
 * a live `waiting 9m 59s`. Reporting an open op's true elapsed time would need a
 * clock passed in; the row-level fix was never carried up to this aggregate.
 */
export function computeTraceStats(
  views: readonly TraceView[],
  extras: TraceStatsExtras = {},
): TraceStats {
  let frames = 0;
  let bytes = 0;
  let subscriptions = 0;
  let liveSubscriptions = 0;
  let malformed = 0;
  let orphaned = 0;
  let unpaired = 0;
  let retryStorms = 0;
  let truncated = 0;
  let out = 0;
  let inbound = 0;
  let durationTotal = 0;
  let durationMax = 0;
  const methodCounts = new Map<string, number>();
  for (const view of views) {
    frames += view.frames.length;
    durationTotal += view.durationMs;
    if (view.durationMs > durationMax) durationMax = view.durationMs;
    if (view.badges.includes("malformed")) malformed += 1;
    if (view.badges.includes("orphaned")) orphaned += 1;
    if (view.badges.includes("unpaired")) unpaired += 1;
    if (view.badges.includes("retry-storm")) retryStorms += 1;
    if (view.badges.includes("truncated")) truncated += 1;
    // Subscription liveness comes from the shared definitions rather than a
    // local role test, so the strip's "subs · N live" can't disagree with the
    // `live` marker the op rows show.
    if (isSubscription(view)) {
      subscriptions += 1;
      if (isLiveSubscription(view)) liveSubscriptions += 1;
    }
    for (const f of view.frames) {
      bytes += f.byteLength ?? 0;
      if (f.direction === "out") out += 1;
      else inbound += 1;
    }
    const method = operationMethod(view) ?? UNKNOWN_METHOD;
    methodCounts.set(method, (methodCounts.get(method) ?? 0) + 1);
  }
  const ops = views.length;
  return {
    ops,
    frames,
    bytes,
    subscriptions,
    liveSubscriptions,
    malformed,
    orphaned,
    unpaired,
    retryStorms,
    truncated,
    evictedTraces: extras.evictedTraces ?? 0,
    droppedByHost: extras.droppedByHost ?? 0,
    codecMismatch: extras.codecMismatch ?? false,
    out,
    in: inbound,
    avgDurationMs: ops === 0 ? 0 : Math.round(durationTotal / ops),
    maxDurationMs: Math.round(durationMax),
    topMethods: [...methodCounts.entries()]
      .sort((a, b) => b[1] - a[1])
      .slice(0, TOP_METHOD_LIMIT)
      .map(([method, count]) => ({ method, count })),
  };
}

/**
 * `512 B` / `1.4 KB` / `2.10 MB`, for a {@link TraceStats} byte total. Shared so
 * the two mounts' summary strips read the same number the same way.
 */
export function formatStatBytes(n: number): string {
  if (n < 1024) return `${String(n)} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(2)} MB`;
}

/** `340ms` / `1.20s`, for a {@link TraceStats} duration. Shared, as above. */
export function formatStatMs(ms: number): string {
  return ms < 1000 ? `${String(Math.round(ms))}ms` : `${(ms / 1000).toFixed(2)}s`;
}

/** Live debug session: feed it envelopes, read back grouped traces. */
export interface DebugSession {
  /** Handle one wire envelope from the host tap. */
  handleEnvelope(envelope: DebugFrameEnvelope): void;
  /** The underlying trace engine (traces, per-id lookup, clear). */
  readonly traceEngine: WireDebugger;
  /** Reverse map from wire `frameId` to method, for labelling frames in a view. */
  readonly methodNames: ReadonlyMap<number, WireMethodInfo>;
  /** Whether level-2 value decode is enabled for this session. */
  readonly decodeValues: boolean;
  /**
   * Drill-down: resolve one frame (by its trace `requestId` and index within
   * that trace) to a {@link FrameValueDetail}. Pass `channelId` to disambiguate
   * when more than one host is connected (each mints the same `p:N` ids).
   * Returns `undefined` if no such frame exists. This is the *only* path that can
   * surface a decoded value, and only when {@link DebugSessionOptions.decodeValues}
   * is on; otherwise it reports byte length only.
   */
  frameDetail(
    requestId: string,
    index: number,
    channelId?: string,
    generation?: number,
  ): FrameValueDetail | undefined;
  /**
   * Decode every frame of one op in a single trace resolution, keyed by frame
   * index (`seq`). This is the batch path the inline drill-down uses, so a mount
   * resolves the op once rather than re-resolving it per frame. Empty when decode
   * is off or the op is not found.
   */
  decodedFrames(
    requestId: string,
    channelId?: string,
    generation?: number,
  ): Map<number, FrameValueDetail>;
}

/**
 * Build a {@link DebugSession}. The `frameId → method` map is derived from the
 * generated wire table and client service names, so traces show
 * `account.getAccount` rather than a bare `id=22`.
 */
export function createDebugSession(
  options: DebugSessionOptions = {},
): DebugSession {
  // Dev-only tool: decode everything by default. The developer is looking at
  // their own session's traffic, so value decode is ON unless a caller explicitly
  // turns it off (tests do).
  const decodeValues = options.decodeValues ?? true;
  const serviceNames = Object.keys(createClient(createTransport(NOOP_PROVIDER)));
  const methodNames = createMethodNameMap(
    W as unknown as Record<string, unknown>,
    serviceNames,
  );
  // No `sink`: a session accumulates traces for the view/`/traces`; it must not
  // spam the server console with a line per frame (the sink default is
  // `console.debug`). Consumers read `traceEngine`, not stdout.
  //
  // The retention caps are the session's memory ceiling
  // (`maxTraces × maxFramesPerTrace`, bounded in bytes by `maxBytesPerTrace`), so
  // they are forwarded rather than left at the engine default: a mount that lives
  // in the observed app's own tab has to be able to lower them.
  const wireDebugger = createWireDebugger({
    methodNames,
    sink: () => {},
    ...(options.maxTraces === undefined ? {} : { maxTraces: options.maxTraces }),
    ...(options.maxFramesPerTrace === undefined
      ? {}
      : { maxFramesPerTrace: options.maxFramesPerTrace }),
    ...(options.maxBytesPerTrace === undefined
      ? {}
      : { maxBytesPerTrace: options.maxBytesPerTrace }),
  });
  // Raw bytes are retained only when decode is on - they exist solely to feed
  // the drill-down decoder, and `/traces` never serializes them. `methodNames`
  // resolves each frame's role at ingest, so the engine and any forward hook see
  // the real role rather than "unknown".
  const handleEnvelope = createDebugIngest(wireDebugger.observe, {
    retainBytes: decodeValues,
    methodNames,
  });
  const decoder = createFrameDecoder({ enabled: decodeValues });

  const frameDetail = (
    requestId: string,
    index: number,
    channelId?: string,
    generation?: number,
  ): FrameValueDetail | undefined => {
    const frame = wireDebugger.trace(requestId, channelId, generation)?.frames[
      index
    ];
    return frame ? decoder.detail(frame) : undefined;
  };

  const decodedFrames = (
    requestId: string,
    channelId?: string,
    generation?: number,
  ): Map<number, FrameValueDetail> => {
    const decoded = new Map<number, FrameValueDetail>();
    if (!decodeValues) return decoded;
    // Resolve the op once, then decode each frame off the resolved trace, rather
    // than re-resolving (a linear scan over every retained trace) per frame.
    const trace = wireDebugger.trace(requestId, channelId, generation);
    if (!trace) return decoded;
    trace.frames.forEach((frame, index) => {
      const detail = decoder.detail(frame);
      if (detail !== undefined) decoded.set(index, detail);
    });
    return decoded;
  };

  return {
    handleEnvelope,
    traceEngine: wireDebugger,
    methodNames,
    decodeValues,
    frameDetail,
    decodedFrames,
  };
}

/**
 * Decode every frame of an op up front, keyed by frame `seq`, ready to hand to
 * {@link renderTraceDetail}'s `decoded` option. A dev-only tool shows values
 * inline rather than behind a per-frame control, so a mount decodes the whole
 * op in one pass. Returns an empty map when the session has decode off.
 */
export function decodeTraceFrames(
  session: DebugSession,
  view: TraceView,
): Map<number, FrameValueDetail> {
  return session.decodedFrames(view.requestId, view.channelId, view.generation);
}

/**
 * Pick which channel record to evict when the registry is full.
 *
 * Least-recently-seen, but never a distrusted one while a trusted one is
 * available. Both mounts share this because §8 of the design doc requires them
 * to give the same answer, and a plain LRU does not: forgetting a channel that
 * declared a foreign wire contract lets it re-register clean on its next frame,
 * so a flood of distinct channel ids launders it and the drift chip clears.
 *
 * Preferring trusted victims closes that without any extra state. A sticky
 * `distrusted` set closes it too, but is add-only and unbounded for the life of
 * the process - affordable in a tab, not in a long-lived server - and that
 * asymmetry is what made the two mounts disagree.
 *
 * Laundering is still reachable when EVERY retained channel is distrusted, at
 * which point the board already reports drift on all of them, so the verdict
 * being rebuilt hides nothing.
 */
export function channelEvictionVictim(
  channels: Map<string, { lastSeen: number; codecOk: boolean }>,
): string | undefined {
  let trustedKey: string | undefined;
  let trustedSeen = Infinity;
  let anyKey: string | undefined;
  let anySeen = Infinity;
  for (const [key, entry] of channels) {
    if (entry.lastSeen < anySeen) {
      anySeen = entry.lastSeen;
      anyKey = key;
    }
    if (entry.codecOk && entry.lastSeen < trustedSeen) {
      trustedSeen = entry.lastSeen;
      trustedKey = key;
    }
  }
  return trustedKey ?? anyKey;
}
