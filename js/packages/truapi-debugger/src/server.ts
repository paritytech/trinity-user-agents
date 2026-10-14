/**
 * The runnable debugger app: the WS server a host dials into, plus a minimal
 * trace view.
 *
 * A host's outward WS dial sends one text message per frame -
 * `{ channelId, dir, frame }`, where `frame` is the base64 of the raw SCALE
 * `ProtocolMessage` bytes (JSON can't carry binary; base64 keeps the envelope on
 * one line). Each message is decoded and grouped by {@link createDebugSession}.
 * `GET /traces` returns the grouped traces (payload-blind - raw bytes and
 * decoded values are never serialized); `GET /op` renders one op's drill-down
 * with each frame's decoded value inline; `GET /frame?id=&i=` is the same
 * decode as a programmatic JSON endpoint. Value decode is on by default (a
 * dev-only tool decodes everything); `GET /` serves a page that polls `/op-list`.
 *
 * The exact host↔debugger framing is not yet standardized (envelope spec, track
 * T3); base64-in-JSON is what this server accepts today. Runs under Bun
 * (`bun run src/server.ts`).
 *
 * @module
 */

import { TRUAPI_CODEC_VERSION, TRUAPI_WIRE_SCHEMA_HASH } from "@parity/truapi";
import {
  computeTraceStats,
  createDebugSession,
  decodeTraceFrames,
} from "./session.js";
import {
  normalizeId,
  WIRE_ENVELOPE_VERSION,
  type DebugFrameEnvelope,
} from "./ingest.js";
import { wireTraceToView, type TraceView } from "./trace-view.js";
import { renderOperationRow, renderTraceDetail } from "./trace-render.js";
import { detectRetryStorms } from "./retry-storm.js";
import { INSPECTOR_LAYOUT_CSS, INSPECTOR_SHELL_CSS } from "./inspector-styles.js";
import { TRACE_DETAIL_CSS } from "./trace-styles.js";

/** Default port the debugger listens on; a host points its debug URL here. */
const DEFAULT_PORT = 9231;

/** Ops one `/view` request renders when the caller names no window. */
const VIEW_DEFAULT_LIMIT = 20;

/** Largest `?limit=` `/view` will honour, so one request stays bounded. */
const VIEW_MAX_LIMIT = 100;

/**
 * Cap on one inbound WS message.
 *
 * Deliberately ABOVE every producer's own per-message ceiling. The native
 * `WsDebugSink` budgets its outbound queue at 8 MiB and refuses to enqueue a
 * serialized line that alone exceeds it, so 8 MiB is the largest envelope a
 * conforming host can send - and base64 inflates a frame by 4/3, so a cap set at
 * the engine's 1 MiB per-trace budget would sit *below* the producer and make an
 * ordinary large payload fatal. Bun does not drop an over-cap message: it CLOSES
 * the connection (1006, "Received too big message") without ever invoking
 * `message`, so an under-set cap silently kills the host's stream mid-session.
 * The cap still bounds memory (well under Bun's 16 MiB default) for anything else
 * that reaches the port.
 */
const MAX_INBOUND_MESSAGE_BYTES = 9 * 1024 * 1024;

/** Frame roles that make an op a subscription rather than a request/response. */
/**
 * The text message a host sends per frame: the envelope with a base64 frame,
 * plus the optional identity fields (`v`, `codec`) a versioned host stamps.
 */
interface WireMessage {
  channelId: string;
  dir: "in" | "out";
  frame: string;
  /**
   * When the producer *observed* the frame, as opposed to when this server
   * received it. A host that buffered a backlog replays it in one burst, so
   * without this every op in the flush collapses to a 0 ms span and ops that
   * were seconds apart land inside the retry-storm window.
   */
  observedAt?: number;
  /** `true` when the producer replayed this frame out of its backlog. */
  buffered?: boolean;
  /** Envelope version; see {@link WIRE_ENVELOPE_VERSION}. */
  v?: number;
  /** The host's wire codec version (`TRUAPI_CODEC_VERSION`). */
  codec?: number;
  /**
   * The host's wire-contract fingerprint (`TRUAPI_WIRE_SCHEMA_HASH`): a hash of
   * every frame id and its method leg. Unlike `codec` (the coarse handshake
   * number, bumped ~never), this changes whenever a frame id is reassigned - the
   * case where a frame could otherwise decode to the wrong method and value off
   * this debugger's table.
   */
  schema?: string;
  /** Frames this host dropped (link backlog full) before this one; surfaced in stats. */
  dropped?: number;
}

/** A parsed inbound message: the envelope plus its wire-identity verdict. */
interface ParsedWireMessage {
  envelope: DebugFrameEnvelope;
  /**
   * `true` when the host stamped a `v`/`codec`/`schema` that does not match this
   * debugger's - the API-evolved-underneath case. Blocks the value-decode path.
   */
  identityMismatch: boolean;
  /**
   * `true` only when the host affirmatively stamped a `schema` equal to this
   * debugger's. Decode is allowed only for confirmed channels: an absent schema
   * (a foreign or pre-identity host) is NOT trusted to decode, closing the
   * omit-identity-to-bypass hole. Payload-blind grouping is unaffected.
   */
  identityConfirmed: boolean;
  /** Frames the host reported dropping before this one. */
  dropped: number;
  /**
   * `true` when the host sent a `dropped` that is not a finite non-negative
   * integer (`Infinity`, a float, a string). The frame is still ingested and the
   * bogus count is discarded, but the fact is counted so a host reporting loss in
   * a shape this debugger can't sum is visible rather than read as "no loss".
   */
  droppedFieldInvalid: boolean;
}

/**
 * Whether a WebSocket upgrade may proceed. Non-browser clients (the CLI, curl)
 * send no Origin and are allowed; a browser sends its page Origin, which must be
 * a loopback host - a cross-origin page dialing the debugger to inject frames is
 * refused (CSWSH), which binding to loopback alone does not prevent.
 */
function originAllowed(origin: string | null): boolean {
  if (origin === null) return true;
  try {
    const host = new URL(origin).hostname;
    // `new URL("http://[::1]").hostname` keeps the brackets ("[::1]"), so strip
    // them before classifying (a bare "::1" never occurs, but is handled too).
    return isLoopbackDebugHost(host === "[::1]" ? "::1" : host);
  } catch {
    return false;
  }
}

/**
 * Parse an optional integer query param: `undefined` if absent, `null` if
 * malformed. Requires a CANONICAL decimal integer, so `""`, `" "`, `"1e3"`,
 * `"0x10"`, `"1.5"`, `"+1"`, `"007"`, and `"-0"` all reject rather than silently
 * coercing (`Number("") === 0`, `Number("0x10") === 16`, and `frames[-0]` is
 * `frames[0]`). Every numeric query param on every route goes through this, so
 * one spelling of "not an integer" can't 400 on one route and resolve a real
 * record on another.
 */
function optionalInt(raw: string | null): number | null | undefined {
  if (raw === null) return undefined;
  const t = raw.trim();
  // No leading zeros, no signed zero: exactly one spelling per value.
  if (!/^(0|-?[1-9]\d*)$/.test(t)) return null;
  const n = Number(t);
  return Number.isInteger(n) ? n : null;
}

/**
 * Parse an optional `?channel=` param. An empty (or whitespace-only) value means
 * ABSENT, not "the channel named `''`": a client that builds the query with
 * `?? ""` would otherwise pin itself to a channel that can never exist, and the
 * decode gate would answer with a 409 "codec mismatch" that is simply false.
 */
function optionalChannel(raw: string | null): string | null {
  if (raw === null) return null;
  const t = raw.trim();
  return t === "" ? null : t;
}

/**
 * The three names this server binds and answers for. Note the case: every caller
 * passes a hostname already normalized by the WHATWG URL parser, which lowercases
 * it, so these literals see `LOCALHOST` as `localhost`. The classifier is never
 * handed a raw header.
 */
const LOOPBACK_LITERALS = new Set(["127.0.0.1", "localhost", "::1"]);

/**
 * Whether `host` is a loopback ORIGIN this server accepts a WS upgrade from: the
 * loopback literals, plus subdomains of `.localhost`.
 *
 * The subdomain case is not a fuzzy match: RFC 6761 reserves `.localhost` as a
 * special-use TLD that always resolves to loopback and cannot be registered
 * publicly, so `host.localhost` is as much loopback as `localhost` is. Real hosts
 * use it - dotli serves its host realm from `host.localhost`, and dials the
 * debugger from that origin - and without this they can never reach the debugger.
 * The dangerous shape this must still reject is the *other* direction, a
 * loopback-looking label under an attacker's domain (`127.0.0.1.evil.com`,
 * `localhost.evil.com`); those do not end in `.localhost` and stay rejected.
 *
 * `Host` headers are NOT classified here - see {@link hostHeaderAllowed}.
 * The input is a WHATWG-normalized (lowercased) hostname, so this is
 * case-sensitive by design.
 */
export function isLoopbackDebugHost(host: string): boolean {
  if (LOOPBACK_LITERALS.has(host)) return true;
  if (!host.endsWith(".localhost")) return false;
  // `*.app.localhost` is a PRODUCT sandbox realm, not a host realm. An embedding
  // host serves untrusted product code from it, so a page there must not be able
  // to dial the debugger and inject frames or drive the decoder. Everything else
  // under `.localhost` is host-owned and stays allowed.
  return !host.endsWith(".app.localhost") && host !== "app.localhost";
}

/**
 * Whether a request's `Host` header targets an address this server is willing to
 * answer for: one of the three names it can actually be reached at.
 *
 * This is the DNS-rebinding guard. Binding to loopback keeps off-box peers out,
 * but a page served from `evil.com` whose DNS has been rebound to `127.0.0.1`
 * can issue same-origin `fetch`es to the debugger and read decoded frames; those
 * requests still carry `Host: evil.com`. Requiring a loopback Host rejects them
 * with a 403. A `Host`-less request (a non-browser client that omits it) is
 * allowed, matching the WS Origin gate's posture.
 *
 * Deliberately NARROWER than the Origin gate: `*.localhost` is a legitimate
 * *origin* for a page that dials in, but never a legitimate *target* - this
 * server binds `127.0.0.1`, and a browser that resolved `x.localhost` to
 * loopback still sends `Host: x.localhost`, an address the debugger does not
 * serve. Accepting it only widens the rebinding surface (a wildcard-DNS
 * `*.localhost` zone under an attacker's control) for no reachable client.
 */
export function hostHeaderAllowed(hostHeader: string | null): boolean {
  if (hostHeader === null || hostHeader === "") return true;
  let hostname: string;
  try {
    hostname = new URL(`http://${hostHeader}`).hostname;
  } catch {
    return false;
  }
  // `new URL("http://[::1]").hostname` keeps the brackets; normalize to bare.
  const normalized = hostname === "[::1]" ? "::1" : hostname;
  return LOOPBACK_LITERALS.has(normalized);
}

/**
 * Why an inbound WS message was not ingested. Every rejection is counted under
 * one of these and surfaced on `/stats`: a silently discarded envelope is
 * indistinguishable from a host that never dialed, which is exactly the state a
 * debugger must never leave its user guessing about.
 */
export type WireRejectReason =
  | "bad-json"
  | "not-object"
  | "bad-channel-id"
  | "empty-channel-id"
  | "bad-dir"
  | "bad-frame"
  | "ingest-threw";

/** Every reject reason, so `/stats` always serializes the same key set. */
const WIRE_REJECT_REASONS: readonly WireRejectReason[] = [
  "bad-json",
  "not-object",
  "bad-channel-id",
  "empty-channel-id",
  "bad-dir",
  "bad-frame",
  "ingest-threw",
];

/** One parse attempt: the message, or the reason it was refused. */
type WireParseResult =
  | { ok: true; value: ParsedWireMessage }
  | { ok: false; reason: WireRejectReason };

/** Parse and validate one inbound WS text message. */
function parseWireMessage(raw: string): WireParseResult {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return { ok: false, reason: "bad-json" };
  }
  if (typeof parsed !== "object" || parsed === null) {
    return { ok: false, reason: "not-object" };
  }
  const m = parsed as Partial<WireMessage>;
  if (typeof m.channelId !== "string") {
    return { ok: false, reason: "bad-channel-id" };
  }
  // `""` is the "all channels" sentinel every filtering endpoint reads as absent,
  // so a host using it as its own id could never be selected, filtered, or
  // decode-scoped. Refuse it at ingest rather than admit an unaddressable host.
  if (m.channelId === "") return { ok: false, reason: "empty-channel-id" };
  if (m.dir !== "in" && m.dir !== "out") return { ok: false, reason: "bad-dir" };
  if (typeof m.frame !== "string") return { ok: false, reason: "bad-frame" };
  const schema = typeof m.schema === "string" ? m.schema : undefined;
  const identityMismatch =
    (typeof m.v === "number" && m.v !== WIRE_ENVELOPE_VERSION) ||
    (typeof m.codec === "number" && m.codec !== TRUAPI_CODEC_VERSION) ||
    (schema !== undefined && schema !== TRUAPI_WIRE_SCHEMA_HASH);
  // `dropped` feeds a summed `droppedByHost: number`, so anything but a finite
  // non-negative integer is not a smaller number - it poisons the whole session's
  // total. `1e999` is `Infinity`, which `JSON.stringify` emits as `null` and the
  // UI renders as "0 dropped" for every channel; a float or a string would break
  // the declared contract just as quietly.
  const droppedRaw = m.dropped;
  const droppedValid =
    droppedRaw === undefined ||
    // `isSafeInteger`, not `isInteger`: 1e308 is an integer, so it passed, and
    // two channels summing to Infinity serialize as JSON `null` - the UI then
    // renders "0 dropped" for the whole session with nothing counted as invalid.
    (typeof droppedRaw === "number" &&
      Number.isSafeInteger(droppedRaw) &&
      droppedRaw >= 0);
  return {
    ok: true,
    value: {
      envelope: {
        channelId: m.channelId,
        dir: m.dir,
        frame: new Uint8Array(Buffer.from(m.frame, "base64")),
        // Provenance travels with the frame: ingest decides whether to trust
        // `observedAt` as the timestamp, and the trace engine suppresses
        // retry-storm detection for a replayed backlog. Dropping these here made
        // the fix invisible through the only mount a host actually dials.
        ...(typeof m.observedAt === "number" ? { observedAt: m.observedAt } : {}),
        ...(m.buffered === true ? { buffered: true as const } : {}),
      },
      identityMismatch,
      identityConfirmed: schema === TRUAPI_WIRE_SCHEMA_HASH,
      dropped:
        droppedValid && typeof droppedRaw === "number" && droppedRaw > 0
          ? droppedRaw
          : 0,
      droppedFieldInvalid: !droppedValid,
    },
  };
}

/** A running debugger server. */
export interface DebugServer {
  /** The port the WS/HTTP server is listening on. */
  readonly port: number;
  /** Whether level-2 value decode is enabled on the drill-down path. */
  readonly decodeValues: boolean;
  /** Stop listening and drop active connections. */
  stop(): void;
}

/**
 * `JSON.stringify` that survives decoded SCALE values: `bigint` becomes a
 * decimal string and `Uint8Array` a `0x…` hex string, both of which
 * `JSON.stringify` otherwise throws on or renders as an index map. Only the
 * drill-down detail path uses this; `/traces` never serializes decoded values.
 */
function safeStringify(value: unknown): string {
  return JSON.stringify(value, (_key, val) => {
    if (typeof val === "bigint") return val.toString();
    if (val instanceof Uint8Array) {
      return `0x${Buffer.from(val).toString("hex")}`;
    }
    return val;
  });
}

/**
 * Start the debugger app: a Bun WS+HTTP server that decodes and groups every
 * frame a host streams to it. `port: 0` binds an ephemeral port, read back from
 * {@link DebugServer.port}.
 *
 * Level-2 value decode is ON unless `decodeValues: false` is passed - this is a
 * dev-only tool that decodes everything (the CLI entry point derives the
 * off-switch from `TRUAPI_DEBUGGER_DECODE_VALUES`). It affects all three
 * drill-down paths, which render or serialize a decoded value: `/op` (the default
 * page's detail pane), `/view` (the standalone fragment), and `/frame` (the JSON
 * endpoint). The list-level endpoints - `/traces`, `/op-list`, `/stats` - are
 * byte- and value-free either way.
 */
export function startDebugServer(
  options: {
    port?: number;
    decodeValues?: boolean;
  } = {},
): DebugServer {
  // Dev-only tool: decode everything by default. A caller can pass
  // `decodeValues: false`.
  const decodeValues = options.decodeValues ?? true;
  const session = createDebugSession({ decodeValues });

  /** Adapt one trace to a view with the shared method map. */
  const toView = (
    trace: ReturnType<typeof session.traceEngine.traces>[number],
    storms: ReturnType<typeof detectRetryStorms>,
  ): TraceView =>
    wireTraceToView(trace, session.methodNames, storms.get(trace) ?? []);

  /**
   * Compute the cross-op retry-storm signal once over a trace set, then adapt
   * every trace. The `traces() → detectRetryStorms → wireTraceToView` pipeline is
   * shared by every list-level endpoint so the same aggregation runs once, not
   * per endpoint.
   */
  const viewsFor = (
    traces: ReturnType<typeof session.traceEngine.traces>,
  ): { trace: (typeof traces)[number]; view: TraceView }[] => {
    const storms = detectRetryStorms(traces);
    return traces.map((trace) => ({ trace, view: toView(trace, storms) }));
  };

  function tracesJson(): string {
    // Payload-blind view: raw `bytes` and decoded values are deliberately never
    // serialized here - values surface only in the `/op` and `/frame` drill-downs.
    // `method`
    // and `role` are public shape metadata derived from the frame id (the same
    // id→name map the op list already exposes), not payload, so they are safe.
    // Rendering each trace through the shared `wireTraceToView` also gives
    // op-level badges (incl. the cross-op retry-storm signal), so the web and
    // terminal frontends read one computed signal rather than each recomputing
    // (or, for the CLI, silently omitting) it.
    const out = viewsFor(session.traceEngine.traces()).map(({ trace: t, view }) => {
      return {
        channelId: t.channelId,
        requestId: t.requestId,
        generation: t.generation,
        startedAt: t.startedAt,
        lastAt: t.lastAt,
        badges: view.badges,
        frames: view.frames.map((f) => ({
          direction: f.direction,
          frameId: f.frameId,
          method: f.method,
          role: f.role,
          byteLength: f.byteLength,
          timestamp: f.timestamp,
        })),
      };
    });
    return JSON.stringify(out);
  }

  /** The `/frame?id=<requestId>&i=<index>[&channel=<channelId>]` drill-down detail response. */
  function frameResponse(url: URL): Response {
    const id = url.searchParams.get("id");
    const channel = optionalChannel(url.searchParams.get("channel")) ?? undefined;
    // Both numeric params go through `optionalInt`: `Number("")`/`Number(" ")` are
    // 0 and pass `Number.isInteger`, `Number("0x0")` is 0, and `frames[-0]` is
    // `frames[0]`, so a bare `Number()` would resolve frame 0 / generation 0 (the
    // oldest recycled op) with a 200 for four different spellings of "not a
    // number".
    const generation = optionalInt(url.searchParams.get("gen"));
    const index = optionalInt(url.searchParams.get("i"));
    if (id === null || index === null || index === undefined || generation === null) {
      return new Response('{"error":"id and integer i required"}', {
        status: 400,
        headers: { "content-type": "application/json" },
      });
    }
    if (!decodeTrusted(channel)) return codecRefusal("application/json");
    const detail = session.frameDetail(id, index, channel, generation);
    if (!detail) {
      return new Response('{"error":"no such frame"}', {
        status: 404,
        headers: { "content-type": "application/json" },
      });
    }
    return new Response(safeStringify(detail), {
      headers: { "content-type": "application/json" },
    });
  }

  /**
   * The `/view?offset=&limit=` fragment: a WINDOW of traces rendered by the
   * shared {@link renderTraceDetail}, the same renderer dotli's panel mounts.
   * Each frame's value is decoded inline for a trusted channel; an untrusted
   * (codec-mismatched) channel groups but shows no value.
   *
   * Bounded per request, and `null` for a malformed window (the caller gets a
   * 400). This is the only endpoint that renders every retained frame's decoded
   * value, so an unbounded response is quadratic in the session's own limits: at
   * the engine's caps a single `/view` would build a multi-hundred-MB string in
   * memory, block the event loop for seconds, and spike RSS by gigabytes. The
   * window keeps one response proportional to what a human reads.
   */
  function viewHtml(url: URL): string | null {
    // `null` (malformed) must not collapse into the default the way `undefined`
    // (absent) does, so the two are separated before either gets a fallback.
    const rawOffset = optionalInt(url.searchParams.get("offset"));
    const rawLimit = optionalInt(url.searchParams.get("limit"));
    if (rawOffset === null || rawLimit === null) return null;
    const offset = rawOffset ?? 0;
    const limit = rawLimit ?? VIEW_DEFAULT_LIMIT;
    if (offset < 0 || limit < 1 || limit > VIEW_MAX_LIMIT) return null;
    const traces = session.traceEngine.traces();
    const entries = viewsFor(traces.slice(offset, offset + limit));
    if (entries.length === 0) {
      return `<div class="td-empty">no frames yet</div>`;
    }
    const shown = offset + entries.length;
    // Say so when the window hides ops, so a truncated read is never mistaken for
    // the whole session (the failure mode the `evicted`/`dropped` tiles exist for).
    const more =
      shown < traces.length
        ? `<div class="td-empty">showing ${offset + 1}-${shown} of ${traces.length} ops — ?offset=${shown} for more</div>`
        : "";
    // Wrap each rendered op in `.td-drilldown` - dotli's verbatim card wrapper -
    // so the standalone list gets the same per-op framing without a bespoke rule.
    return (
      entries
        .map(
          ({ view }) =>
            `<div class="td-drilldown">` +
            renderTraceDetail(view, {
              offerDecode: session.decodeValues,
              // Same codec/schema-drift guard the `/frame` endpoint enforces: an
              // untrusted channel's frames group but never surface a decoded value.
              decoded: decodeTrusted(view.channelId)
                ? decodeTraceFrames(session, view)
                : undefined,
            }) +
            `</div>`,
        )
        .join("") + more
    );
  }

  // Per-channel liveness for the inspector's host dimension. The envelope
  // carries channelId; recording first/last-seen + frame count lets the UI show
  // which hosts have dialed in and whether they are still active. Grouping
  // traces by channel is a separate engine concern; this is only connection
  // state.
  //
  // `connected` is RECENCY-based, not socket-based: a host counts as connected
  // if it emitted a frame within the last CONNECTED_WINDOW_MS. It is NOT "has an
  // open WS socket" - one WS can multiplex frames for several channelIds, so
  // per-host socket liveness is not a clean fact. A host that goes quiet without
  // closing its socket correctly reads as not-connected after the window.
  const CONNECTED_WINDOW_MS = 5000;
  // Cap the registry so a host (or anything able to reach the port) emitting
  // frames under many distinct channelIds can't grow it without bound; when
  // full, evict the least-recently-seen channel.
  const MAX_CHANNELS = 256;
  const channels = new Map<
    string,
    {
      channelId: string;
      firstSeen: number;
      lastSeen: number;
      frameCount: number;
      // `false` once this host has sent a frame whose declared wire identity
      // (`v`/`codec`/`schema`) does not match this debugger's. Sticky: a single
      // mismatch marks the host untrusted for the rest of the session.
      codecOk: boolean;
      // `true` once this host affirmatively stamped a matching `schema`. Decode
      // requires it, so a host that never declares identity is refused, not
      // trusted by omission.
      schemaOk: boolean;
      // Frames the host reported dropping before delivery (its link backlog
      // filled): a gap attributable to the link, surfaced so it is not read as
      // the host "not answering".
      dropped: number;
    }
  >();
  let openSockets = 0;
  // Sticky: any host has sent an unconfirmed (mismatched or unstamped) frame this
  // session. The no-channel decode path keys on this rather than scanning the live
  // registry, because an untrusted host's channel record can be LRU-evicted (see
  // MAX_CHANNELS) while its frames survive in the trace engine.
  let sawUntrusted = false;
  // Envelopes refused at ingest, by reason. Every reason is pre-seeded so the
  // `/stats` key set is fixed and a client can chart a reason that is still zero.
  const rejectCounts = new Map<WireRejectReason, number>(
    WIRE_REJECT_REASONS.map((r) => [r, 0]),
  );
  // Sockets Bun closed abnormally (code 1006), and the subset it closed because an
  // inbound message exceeded MAX_INBOUND_MESSAGE_BYTES. An over-cap message never
  // reaches `message()`, so this close is the ONLY place the loss can be counted:
  // without it an over-cap host's stream simply stops with every counter untouched.
  let abnormalCloses = 0;
  let oversizedMessages = 0;
  // Frames whose `dropped` field was unusable (see `droppedFieldInvalid`).
  let invalidDroppedFields = 0;

  /** Count one refused envelope under its reason. */
  function recordReject(reason: WireRejectReason): void {
    rejectCounts.set(reason, (rejectCounts.get(reason) ?? 0) + 1);
  }

  function recordChannel(channelId: string, parsed: ParsedWireMessage): void {
    // Asymmetry with the in-app mount, recorded deliberately: that one keeps a
    // `distrusted` set of channels evicted while carrying a mismatch, so a
    // mismatching channel cannot buy back a clean record by being forgotten. This
    // mount has no equivalent, so after MAX_CHANNELS distinct ids a mismatched
    // channel re-registers as `codecOk: true` and the drift chip clears.
    //
    // Not ported rather than not noticed. That set is add-only and never pruned -
    // unbounded for the life of the process - which a tab can absorb and a
    // long-lived server should not. Bounding it without reopening the laundering
    // it exists to prevent is a design question, not a copy. Frames retained from
    // before the eviction keep their own `identityConfirmed: false` stamp, so this
    // does not open a decode path for them; it misreports the channel's state.
    //
    // A host is untrusted if it did not confirm the schema OR if any declared
    // identity field mismatched. Keying only on `identityConfirmed` would let the
    // `matching schema + mismatched v/codec` host (confirmed AND mismatched) leave
    // this flag false, and with it the whole channel-less decode path open -
    // discarding the one signal that sees a payload-layout drift the schema hash
    // is blind to.
    if (!parsed.identityConfirmed || parsed.identityMismatch) sawUntrusted = true;
    if (parsed.droppedFieldInvalid) invalidDroppedFields += 1;
    const now = Date.now();
    const key = normalizeId(channelId);
    const existing = channels.get(key);
    if (existing) {
      existing.lastSeen = now;
      existing.frameCount += 1;
      existing.dropped += parsed.dropped;
      if (parsed.identityMismatch) existing.codecOk = false;
      if (parsed.identityConfirmed) existing.schemaOk = true;
      return;
    }
    if (channels.size >= MAX_CHANNELS) {
      let oldestKey: string | undefined;
      let oldestSeen = Infinity;
      for (const [k, c] of channels) {
        if (c.lastSeen < oldestSeen) {
          oldestSeen = c.lastSeen;
          oldestKey = k;
        }
      }
      if (oldestKey !== undefined) channels.delete(oldestKey);
    }
    channels.set(key, {
      channelId: key,
      firstSeen: now,
      lastSeen: now,
      frameCount: 1,
      codecOk: !parsed.identityMismatch,
      schemaOk: parsed.identityConfirmed,
      dropped: parsed.dropped,
    });
  }

  /**
   * Whether a decoded value may be surfaced for a channel's frames. Only bites
   * when decode is on (payload-blind mode never decodes anyway). Decode is
   * allowed only for a channel that affirmatively stamped a matching wire
   * `schema` and never mismatched.
   *
   * This is a COMPATIBILITY guard against honest version drift - a host built
   * against a different frame table, where an id could resolve to the wrong
   * method and value off this debugger's table - not authentication:
   * `TRUAPI_WIRE_SCHEMA_HASH` is a public build constant, so a deliberate local
   * injector could stamp it. The WS Origin gate ({@link originAllowed}) is the
   * boundary against injection; this is defence in depth on top of it.
   */
  function decodeTrusted(channel: string | undefined): boolean {
    if (!decodeValues) return true;
    if (channel !== undefined) {
      const c = channels.get(normalizeId(channel));
      return c !== undefined && c.codecOk && c.schemaOk;
    }
    // No channel disambiguator: refuse once any host has been untrusted this
    // session (sticky, so an evicted untrusted record can't launder its surviving
    // frames). An all-trusted or empty session stays true, so a missing frame
    // 404s rather than being masked by a refusal.
    return !sawUntrusted;
  }

  /** The 409 a decode path returns when the source host's wire codec mismatches. */
  function codecRefusal(contentType: string): Response {
    return new Response('{"error":"decode refused: host wire codec mismatch"}', {
      status: 409,
      headers: { "content-type": contentType },
    });
  }

  function channelsJson(): string {
    const now = Date.now();
    const list = [...channels.values()].sort((a, b) => b.lastSeen - a.lastSeen);
    return JSON.stringify({
      sockets: openSockets,
      // A banner signal: at least one connected host is streaming a wire codec
      // this debugger can't decode against.
      codecMismatch: list.some((c) => !c.codecOk),
      channels: list.map((c) => ({
        ...c,
        connected: now - c.lastSeen < CONNECTED_WINDOW_MS,
      })),
    });
  }

  /**
   * The `/stats?channel=` aggregate roll-up over the ops being listed: counts,
   * byte totals, durations, health-badge tallies, the request/response split,
   * and the busiest methods. Payload-blind - it sums shape and timing only and
   * never serializes a byte or a decoded value. Feeds the inspector's summary
   * strip (the "aggregate-level value").
   */
  function statsJson(channel: string | null): string {
    /** The payload-blind aggregate shape `/stats` serializes. */
    interface StatsPayload {
      ops: number;
      frames: number;
      bytes: number;
      subscriptions: number;
      liveSubscriptions: number;
      malformed: number;
      orphaned: number;
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
      /**
       * Link-level loss and liveness, SESSION-WIDE (never narrowed by
       * `?channel=`): a rejected envelope has no channel to attribute it to, and a
       * closed socket may have carried several. Grouped so a client can tell "the
       * host is quiet" from "the host is talking and this debugger is refusing or
       * losing what it says".
       */
      sockets: number;
      envelopeRejects: number;
      envelopeRejectReasons: Record<string, number>;
      oversizedMessages: number;
      abnormalCloses: number;
      invalidDroppedFields: number;
    }
    const traces =
      channel === null
        ? session.traceEngine.traces()
        : session.traceEngine.tracesForChannel(normalizeId(channel));
    // ONE aggregate for both mounts. A second implementation here is exactly how
    // the two silently disagreed: this block tested `!some(role === "stop")` for
    // liveness, ignoring `interrupt`, so every host-terminated subscription
    // (chain switch, revoked permission) counted as live forever and the tile
    // climbed all session above an op list showing nothing live.
    const stats = computeTraceStats(viewsFor(traces).map(({ view }) => view));
    const evictedTraces = session.traceEngine.evictedTraces();
    const chanList =
      channel === null
        ? [...channels.values()]
        : [...channels.values()].filter(
            (c) => c.channelId === normalizeId(channel),
          );
    const droppedByHost = chanList.reduce((n, c) => n + c.dropped, 0);
    const codecMismatch = chanList.some((c) => !c.codecOk);
    // Typed so a dropped/renamed field is a compile error, not a silent gap in
    // the payload a client parses back.
    const payload: StatsPayload = {
      ...stats,
      evictedTraces,
      droppedByHost,
      codecMismatch,
      sockets: openSockets,
      envelopeRejects: [...rejectCounts.values()].reduce((n, c) => n + c, 0),
      envelopeRejectReasons: Object.fromEntries(rejectCounts),
      oversizedMessages,
      abnormalCloses,
      invalidDroppedFields,
    };
    return JSON.stringify(payload);
  }

  /** The op's method for sorting: the first frame that resolves to one. */
  function traceMethod(
    trace: ReturnType<typeof session.traceEngine.traces>[number],
  ): string {
    for (const f of trace.frames) {
      const method = session.methodNames.get(f.frameId)?.method;
      if (method !== undefined) return method;
    }
    return "";
  }

  /**
   * Order the op list for the `?sort=` control. Default (`""`) keeps arrival
   * order (stable under live updates); the others are one-shot reorders the
   * client's keyed diff mirrors into the DOM.
   */
  function sortTraces(
    traces: ReturnType<typeof session.traceEngine.traces>,
    sort: string | null,
  ): ReturnType<typeof session.traceEngine.traces> {
    if (!sort) return traces;
    const copy = [...traces];
    switch (sort) {
      case "recent":
        return copy.sort((a, b) => b.lastAt - a.lastAt);
      case "duration":
        return copy.sort(
          (a, b) => b.lastAt - b.startedAt - (a.lastAt - a.startedAt),
        );
      case "frames":
        return copy.sort((a, b) => b.frames.length - a.frames.length);
      case "method":
        return copy.sort((a, b) => traceMethod(a).localeCompare(traceMethod(b)));
      default:
        return traces;
    }
  }

  /**
   * The `/op-list?channel=&sort=` primary view: one server-rendered row per op
   * (the shared {@link renderOperationRow}), payload-blind. Retry-storm is a
   * cross-op signal computed here and fed to each view as an extra badge.
   * `channel` filters on the trace's channelId; `sort` reorders the rows.
   */
  function opListHtml(channel: string | null, sort: string | null): string {
    const base =
      channel === null
        ? session.traceEngine.traces()
        : session.traceEngine.tracesForChannel(normalizeId(channel));
    // Retry-storm is per-channel (a burst of like ops from one host), so it is
    // detected over exactly the traces being listed - before any reorder, since
    // the storm map is keyed by the trace object, not its position.
    const storms = detectRetryStorms(base);
    if (base.length === 0) {
      return `<div class="td-op-empty">no operations yet</div>`;
    }
    const rows = sortTraces(base, sort);
    // If any listed op is from a host whose wire contract differs from this
    // debugger's, its method names may be wrong. Warn inline above the rows - not
    // only in the global banner - so the mislabeled rows carry the caveat.
    // "Unreliable" = a mismatched OR merely unconfirmed host: either way its
    // method names come from this debugger's table and may be wrong, so the label
    // matches the decode gate's bar rather than the narrower banner.
    const mismatched = new Set(
      [...channels.values()]
        .filter((c) => !c.codecOk || !c.schemaOk)
        .map((c) => c.channelId),
    );
    const notice =
      mismatched.size > 0 &&
      rows.some((t) => mismatched.has(normalizeId(t.channelId)))
        ? `<div style="padding:4px 10px;color:#fca5a5;font-size:11px;border-bottom:1px solid rgba(255,255,255,.08)">⚠ a connected host's wire contract differs from this debugger's — method names below may be wrong</div>`
        : "";
    return (
      notice +
      rows
        .map((t) => renderOperationRow(toView(t, storms), { now: Date.now() }))
        .join("")
    );
  }

  /**
   * The `/op?id=&channel=` detail fragment: the selected op via
   * {@link renderTraceDetail}. `channel` disambiguates the `requestId` when more
   * than one host is connected (each mints the same `p:N` ids).
   */
  function opDetailHtml(
    requestId: string,
    channel: string | null,
    generation?: number,
  ): string {
    const trace = session.traceEngine.trace(
      requestId,
      channel ?? undefined,
      generation,
    );
    if (!trace) {
      return `<div class="td-detail-empty">operation not found</div>`;
    }
    const storms = detectRetryStorms(
      session.traceEngine.tracesForChannel(trace.channelId),
    );
    const view = toView(trace, storms);
    return renderTraceDetail(view, {
      offerDecode: session.decodeValues,
      // Codec/schema-drift guard, matching `/frame`: refuse to decode a channel
      // whose wire schema did not affirmatively match this debugger's table.
      decoded: decodeTrusted(channel ?? undefined)
        ? decodeTraceFrames(session, view)
        : undefined,
    });
  }

  const htmlHeaders = { "content-type": "text/html; charset=utf-8" };

  /**
   * Route one request. Every throw is contained by the caller, so a malformed
   * request can only ever cost its own response.
   */
  function route(req: Request, srv: Bun.Server<undefined>): Response | undefined {
    const url = new URL(req.url);
    // Reject cross-origin WebSocket upgrades (CSWSH): binding to loopback keeps
    // off-box peers out, but a page open in the dev's own browser could still
    // dial ws://127.0.0.1:<port> to inject frames or drive the decoder over
    // hostile bytes. A same-origin inspector and non-browser clients are
    // allowed; a foreign browser Origin is not.
    if (req.headers.get("upgrade")?.toLowerCase() === "websocket") {
      if (!originAllowed(req.headers.get("origin"))) {
        return new Response("forbidden origin", { status: 403 });
      }
      if (srv.upgrade(req)) return undefined;
    }
    if (url.pathname === "/traces") {
      return new Response(tracesJson(), {
        headers: { "content-type": "application/json" },
      });
    }
    if (url.pathname === "/channels") {
      return new Response(channelsJson(), {
        headers: { "content-type": "application/json" },
      });
    }
    if (url.pathname === "/stats") {
      return new Response(statsJson(optionalChannel(url.searchParams.get("channel"))), {
        headers: { "content-type": "application/json" },
      });
    }
    if (url.pathname === "/op-list") {
      return new Response(
        opListHtml(
          optionalChannel(url.searchParams.get("channel")),
          url.searchParams.get("sort"),
        ),
        { headers: htmlHeaders },
      );
    }
    if (url.pathname === "/op") {
      const id = url.searchParams.get("id");
      const generation = optionalInt(url.searchParams.get("gen"));
      if (generation === null) {
        return new Response(`<div class="td-detail-empty">bad request</div>`, {
          status: 400,
          headers: htmlHeaders,
        });
      }
      return new Response(
        id === null
          ? `<div class="td-detail-empty">select an operation</div>`
          : opDetailHtml(
              id,
              optionalChannel(url.searchParams.get("channel")),
              generation,
            ),
        { headers: htmlHeaders },
      );
    }
    if (url.pathname === "/view") {
      const html = viewHtml(url);
      return html === null
        ? new Response(`<div class="td-empty">bad request</div>`, {
            status: 400,
            headers: htmlHeaders,
          })
        : new Response(html, { headers: htmlHeaders });
    }
    if (url.pathname === "/frame") {
      return frameResponse(url);
    }
    return new Response(VIEW_HTML, { headers: htmlHeaders });
  }

  const server = Bun.serve({
    port: options.port ?? DEFAULT_PORT,
    // Loopback only: the debugger holds every trace (and, with decode on,
    // decoded values), so it must not listen on all interfaces where a LAN peer
    // could read or inject.
    hostname: "127.0.0.1",
    fetch(req, srv) {
      // DNS-rebinding guard: the request's Host must be one this server answers
      // for. This blocks a rebound `evil.com -> 127.0.0.1` page from reading
      // decoded frames over same-origin fetches, which binding to loopback alone
      // does not prevent.
      //
      // FIRST, before `new URL(req.url)`: Bun builds `req.url` from the Host
      // header, so an unparseable authority (`Host: localhost:99999`) throws
      // inside the URL constructor. Gating first turns that into the 403 the
      // header already earns, instead of a 500 plus a stack trace per request.
      if (!hostHeaderAllowed(req.headers.get("host"))) {
        return new Response("forbidden host", { status: 403 });
      }
      // The WS handler is explicitly exception-safe; so is the route dispatcher.
      // One malformed request must cost its own response and nothing else - no
      // 500 with a stack trace, and no unhandled rejection taking the process
      // down mid-session.
      try {
        return route(req, srv);
      } catch {
        return new Response("bad request", { status: 400 });
      }
    },
    websocket: {
      maxPayloadLength: MAX_INBOUND_MESSAGE_BYTES,
      open() {
        openSockets += 1;
      },
      close(_ws, code, reason) {
        openSockets = Math.max(0, openSockets - 1);
        // An over-cap message is not dropped: Bun closes the socket (1006,
        // "Received too big message") without invoking `message`, so this is the
        // only place the loss is observable. Count the specific case, and every
        // abnormal close, so a stream that dies mid-session shows up on /stats
        // instead of looking like a host that simply went quiet.
        if (code === 1006) {
          abnormalCloses += 1;
          if (/too big/i.test(reason ?? "")) oversizedMessages += 1;
        }
      },
      message(_ws, message) {
        // Defensive: a malformed frame must never take down the socket callback.
        // parseWireMessage + the Result-based ingest don't throw today, but keep
        // the invariant local so a future ingest change can't propagate here.
        try {
          const raw = typeof message === "string" ? message : message.toString();
          const parsed = parseWireMessage(raw);
          if (parsed.ok) {
            recordChannel(parsed.value.envelope.channelId, parsed.value);
            // Still grouped (payload-blind is safe and useful); a mismatch only
            // blocks the value-decode path, via decodeTrusted.
            // Stamp the frame with ITS OWN producer's verdict, so decode is gated
            // per frame. See ObservedFrame.identityConfirmed.
            session.handleEnvelope({
              ...parsed.value.envelope,
              // BOTH halves, for symmetry with the in-app mount, which ANDs the
              // same two: `identityConfirmed` is the schema match ALONE, so a
              // host stamping the right hash with a wrong `v`/`codec` is
              // "confirmed AND mismatched".
              //
              // The `!identityMismatch` half is DEFENSIVE and no test pins it.
              // Reaching it needs a channel whose record says trusted while a
              // retained frame says mismatched, which needs a channel record
              // evicted and re-registered clean while its own frames survive.
              // On THIS surface that cannot happen: `startDebugServer` takes no
              // retention options, so `maxTraces` is the engine default 256,
              // MAX_CHANNELS is 256, and every channel costs at least one trace.
              // The in-app mount forwards `maxTraces` from its caller, so the
              // same reasoning does not carry there and this half is load-bearing
              // rather than redundant. `decodeTrusted` is the gate that actually
              // refuses these on both surfaces.
              identityConfirmed:
                parsed.value.identityConfirmed &&
                !parsed.value.identityMismatch,
            });
          } else {
            recordReject(parsed.reason);
          }
        } catch {
          // Drop the frame; the observed session is worth more than one trace.
          recordReject("ingest-threw");
        }
      },
    },
  });

  return {
    // Always a TCP port here; the `?? 0` only satisfies Bun's unix-socket union.
    port: server.port ?? 0,
    decodeValues,
    stop: () => server.stop(true),
  };
}

/**
 * The wire inspector: a full-screen, host-agnostic dev tool - a Network tab for
 * TrUAPI wire frames. Left is the operation list (one row per op, the primary
 * view); right is the selected op's frame sequence via the shared
 * {@link renderTraceDetail}. A top bar switches between the hosts that have
 * dialed in; a status bar shows counts and liveness.
 *
 * The client is a thin shell over server-rendered fragments: it polls
 * `/op-list` (the shared {@link renderOperationRow}) and `/channels`, and fetches
 * `/op` when an operation is selected. Every injected fragment is produced and
 * escaped server-side, so `innerHTML` is safe. `/op-list` is payload-blind
 * (shape/timing only); `/op` renders each frame's decoded value inline for a
 * trusted channel. `td-*` classes are owned by the shared renderer.
 */
const VIEW_HTML = `<!doctype html>
<meta charset="utf-8">
<title>TrUAPI Wire Inspector</title>
<style>
  :root { color-scheme: dark; }
  * { box-sizing: border-box; }
  html, body { height: 100%; margin: 0; }
  body { font: 12px ui-monospace, SFMono-Regular, Menlo, monospace;
    background: #0a0a0a; color: #e0e0e0; display: grid;
    grid-template-rows: auto auto 1fr auto; height: 100vh; overflow: hidden; }
${INSPECTOR_SHELL_CSS}
${TRACE_DETAIL_CSS}
${INSPECTOR_LAYOUT_CSS}
</style>
<div class="ins-top">
  <span class="ins-title">TrUAPI <span class="accent">Wire Inspector</span></span>
  <input class="ins-filter" id="filter" type="search" placeholder="filter methods…" autocomplete="off" spellcheck="false">
  <select class="ins-sort" id="sort" title="Sort operations">
    <option value="">arrival</option>
    <option value="recent">recent</option>
    <option value="method">method</option>
    <option value="duration">slowest</option>
    <option value="frames">most frames</option>
  </select>
  <span class="ins-channels" id="channels"></span>
</div>
<div class="ins-summary empty" id="summary">waiting for frames…</div>
<div class="ins-body">
  <div class="ins-list" id="list" tabindex="0"><div class="td-op-empty">waiting for frames…</div></div>
  <div class="ins-split" id="split" title="Drag to resize"></div>
  <div class="ins-detail" id="detail" tabindex="0"><div class="td-detail-empty">Select an operation to inspect its frames. ↑/↓ to move, Enter to open.</div></div>
</div>
<div class="ins-status" id="status">connecting…</div>
<script>
  var listEl = document.getElementById("list");
  var detailEl = document.getElementById("detail");
  var chanEl = document.getElementById("channels");
  var statusEl = document.getElementById("status");
  var summaryEl = document.getElementById("summary");
  var filterEl = document.getElementById("filter");
  var sortEl = document.getElementById("sort");

  var selectedId = null;      // requestId of the open op
  var selectedChannel = null; // channelId of the open op (disambiguates requestId across hosts)
  var selectedGen = null;     // generation of the open op (disambiguates a recycled requestId)
  var channel = null;         // channelId filter, null = all
  var lastListHtml = "";   // skip rebuilds when the op list is unchanged
  var lastDetailHtml = ""; // skip detail refresh when the open op is unchanged
  var cursor = -1;         // frame index highlighted in the detail
  var filter = "";         // method substring filter (client-side, over the op list)
  var sortMode = "";       // server-side sort key ("" = arrival order)

  // The op list is a live, keyed-diff DOM; filtering hides rows with a class and
  // must be re-applied after every rebuild. Sort is a server concern (?sort=).
  function applyFilter() {
    rows().forEach(function (r) {
      var m = r.querySelector(".td-op-method");
      var text = m ? m.textContent.toLowerCase() : "";
      var hideText = filter !== "" && text.indexOf(filter) === -1;
      r.classList.toggle("filtered-out", hideText);
    });
  }
  filterEl.addEventListener("input", function () {
    filter = filterEl.value.trim().toLowerCase();
    applyFilter();
  });
  sortEl.addEventListener("change", function () {
    sortMode = sortEl.value;
    lastListHtml = "";  // force a rebuild under the new order
    poll();
  });
  // Click a top-method pill to filter to it.
  summaryEl.addEventListener("click", function (e) {
    var pill = e.target.closest && e.target.closest(".ins-method");
    if (!pill) return;
    filter = (pill.getAttribute("data-method") || "").toLowerCase();
    filterEl.value = pill.getAttribute("data-method") || "";
    applyFilter();
  });

  function get(url) { return fetch(url).then(function (r) { return r.text(); }); }

  // Length-prefixed for the same reason the engine's key is - though this is a
  // DIFFERENT key, (requestId, channelId, generation): a bare separator lets a
  // sender-controlled id forge the separator, so two different ops collide onto
  // one key and the wrong row is selected.
  function part(v) { var s = v || ""; return s.length + ":" + s; }
  function keyOf(el) {
    return part(el.getAttribute("data-request-id")) + "\\0" + part(el.getAttribute("data-channel-id")) + "\\0" + part(el.getAttribute("data-generation") || "0");
  }
  // The selected op's identity is (requestId, channelId), not requestId alone -
  // two hosts on the "all" view mint the same p:N, so selection, the keyed diff,
  // and keyboard nav must all match on the composite key.
  function selKey() {
    return selectedId === null ? null : part(selectedId) + "\\0" + part(selectedChannel) + "\\0" + part(selectedGen || "0");
  }
  function visibleRows() {
    return rows().filter(function (r) { return !r.classList.contains("filtered-out"); });
  }

  // Keyed diff of the op list: only add/remove/patch rows that actually changed,
  // keyed by (requestId, channelId). Unchanged rows keep their DOM identity, so
  // selection, keyboard focus, and an in-flight click survive a live update -
  // critical because a live subscription's row changes every poll.
  function applyList(html) {
    if (html === lastListHtml) return;
    lastListHtml = html;
    var tmp = document.createElement("div");
    tmp.innerHTML = html;
    var incoming = Array.prototype.slice.call(tmp.children);
    // Empty / waiting state (a single non-row div): replace wholesale.
    if (incoming.length === 0 || incoming[0].getAttribute("data-request-id") === null) {
      listEl.innerHTML = html;
      return;
    }
    // Drop any leftover placeholder (e.g. the initial "waiting" state) before
    // the first real rows land.
    Array.prototype.slice.call(listEl.children).forEach(function (c) {
      if (!c.classList || !c.classList.contains("td-op")) c.remove();
    });
    var existing = {};
    Array.prototype.slice.call(listEl.querySelectorAll(".td-op")).forEach(function (r) {
      existing[keyOf(r)] = r;
    });
    var seen = {};
    var prev = null;
    incoming.forEach(function (row) {
      var key = keyOf(row);
      seen[key] = true;
      var cur = existing[key];
      if (cur) {
        if (cur.innerHTML !== row.innerHTML) cur.innerHTML = row.innerHTML;
        if (cur.className !== row.className) cur.className = row.className;
      } else {
        cur = row;
      }
      cur.classList.toggle("selected", selKey() !== null && keyOf(cur) === selKey());
      // Place in incoming order without disturbing untouched neighbours.
      var want = prev ? prev.nextSibling : listEl.firstChild;
      if (cur !== want) listEl.insertBefore(cur, want);
      prev = cur;
    });
    Array.prototype.slice.call(listEl.querySelectorAll(".td-op")).forEach(function (r) {
      if (!seen[keyOf(r)]) r.remove();
    });
    applyFilter();  // rows changed; re-apply the active method filter
  }

  function rows() { return Array.prototype.slice.call(listEl.querySelectorAll(".td-op")); }

  function selectOp(id, chan, gen) {
    selectedId = id;
    selectedChannel = chan || null;
    selectedGen = gen == null ? "0" : String(gen);
    var want = selKey();
    var row = null;
    rows().forEach(function (r) {
      var on = keyOf(r) === want;
      r.classList.toggle("selected", on);
      if (on) row = r;
    });
    cursor = -1;
    get("/op?id=" + encodeURIComponent(id) +
      (selectedChannel ? "&channel=" + encodeURIComponent(selectedChannel) : "") +
      "&gen=" + encodeURIComponent(selectedGen))
      .then(function (frag) {
        lastDetailHtml = frag;
        detailEl.innerHTML = frag;
      });
    if (row) row.scrollIntoView({ block: "nearest" });
  }

  // List keyboard: step selection, open detail.
  listEl.addEventListener("keydown", function (e) {
    // Navigate only the rows the current filter actually shows.
    var rs = visibleRows();
    if (rs.length === 0) return;
    var key = selKey();
    var idx = rs.findIndex(function (r) { return keyOf(r) === key; });
    function pick(r) { selectOp(r.getAttribute("data-request-id"), r.getAttribute("data-channel-id"), r.getAttribute("data-generation")); }
    if (e.key === "ArrowDown") {
      e.preventDefault();
      var n = idx < 0 ? 0 : Math.min(idx + 1, rs.length - 1);
      pick(rs[n]);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      var p = idx < 0 ? rs.length - 1 : Math.max(idx - 1, 0);
      pick(rs[p]);
    } else if (e.key === "Enter" || e.key === "ArrowRight") {
      e.preventDefault();
      detailEl.focus();
      moveCursor(0);
    } else if (e.key === "Escape") {
      selectedId = null;
      selectedChannel = null;
      lastDetailHtml = "";
      rows().forEach(function (r) { r.classList.remove("selected"); });
      detailEl.innerHTML = '<div class="td-detail-empty">Select an operation to inspect its frames.</div>';
    }
  });
  listEl.addEventListener("click", function (e) {
    var row = e.target.closest && e.target.closest(".td-op");
    if (row) { listEl.focus(); selectOp(row.getAttribute("data-request-id"), row.getAttribute("data-channel-id"), row.getAttribute("data-generation")); }
  });

  // Detail keyboard: move the frame cursor (values are already rendered inline).
  function frameEls() { return Array.prototype.slice.call(detailEl.querySelectorAll(".td-frame")); }
  function moveCursor(next) {
    var fs = frameEls();
    if (fs.length === 0) return;
    cursor = Math.max(0, Math.min(next, fs.length - 1));
    fs.forEach(function (f, i) { f.classList.toggle("cursor", i === cursor); });
    fs[cursor].scrollIntoView({ block: "nearest" });
  }
  detailEl.addEventListener("keydown", function (e) {
    if (e.key === "ArrowDown") { e.preventDefault(); moveCursor(cursor + 1); }
    else if (e.key === "ArrowUp") { e.preventDefault(); moveCursor(cursor - 1); }
    else if (e.key === "ArrowLeft" || e.key === "Escape") { e.preventDefault(); listEl.focus(); }
  });

  // Channel switcher + status, from /channels connection state. Liveness uses
  // the server's connected flag (one recency threshold, server-side) rather
  // than recomputing here, so the dot and the server agree.
  function renderChannels(data) {
    var live = 0;
    var html = '<button class="ins-chan' + (channel === null ? " active" : "") +
      '" data-chan="">all</button>';
    (data.channels || []).forEach(function (c) {
      if (c.connected) live++;
      html += '<button class="ins-chan' + (channel === c.channelId ? " active" : "") +
        '" data-chan="' + encodeURIComponent(c.channelId) + '">' +
        '<span class="dot' + (c.connected ? " live" : "") + '"></span>' + escHtml(c.channelId) + "</button>";
    });
    chanEl.innerHTML = html;
    var hosts = (data.channels || []).length;
    // A host streaming a wire codec this debugger can't decode against: value
    // decode is refused for it (payload-blind grouping still works). Banner it.
    var codecWarn = data.codecMismatch
      ? ' · <span class="mismatch" title="A host is streaming a wire codec this debugger cannot decode against; value decode is refused for it.">⚠ codec mismatch</span>'
      : "";
    // Open WS sockets, distinct from "hosts": one socket can multiplex several
    // channelIds, and a host can be recency-idle with its socket still open.
    // Rendered because 0 sockets with 0 ops is "nobody dialed", while 1 socket
    // with 0 ops is "a host is connected and nothing is being ingested" - the
    // exact state the reject counters explain.
    var sockets = data.sockets || 0;
    statusEl.innerHTML = rows().length + " ops · " + hosts + " host" + (hosts === 1 ? "" : "s") +
      " · " + sockets + " socket" + (sockets === 1 ? "" : "s") +
      " · " + (live > 0 ? '<span class="live">' + live + " live</span>" : "idle") + codecWarn;
  }
  function escHtml(s) {
    return String(s).replace(/[&<>"']/g, function (c) {
      return { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c];
    });
  }

  // Aggregate summary strip: the at-a-glance roll-up from /stats (payload-blind -
  // counts, bytes, durations, health, direction split, busiest methods).
  function fmtBytes(n) {
    if (n < 1024) return n + " B";
    if (n < 1024 * 1024) return (n / 1024).toFixed(1) + " KB";
    return (n / (1024 * 1024)).toFixed(2) + " MB";
  }
  function fmtMs(ms) {
    return ms < 1000 ? Math.round(ms) + "ms" : (ms / 1000).toFixed(2) + "s";
  }
  function statTile(n, k, sub) {
    return '<div class="ins-stat"><span class="n">' + escHtml(String(n)) +
      (sub ? ' <span class="sub">' + escHtml(sub) + "</span>" : "") +
      '</span><span class="k">' + escHtml(k) + "</span></div>";
  }
  // Neutral sibling of warnTile: a count worth showing that is not a fault.
  // Both render NOTHING at zero. These are exception counters - "0 truncated,
  // 0 evicted, 0 dropped" is the normal state, and a strip of ten zeros reads as
  // unpopulated scaffolding rather than as reassurance. A tile appearing at all
  // is therefore the signal; its absence means the count is genuinely zero.
  function infoTile(n, k) {
    if (!n) return "";
    return '<div class="ins-stat"><span class="n">' + n +
      '</span><span class="k">' + escHtml(k) + "</span></div>";
  }
  function warnTile(n, k) {
    if (!n) return "";
    return '<div class="ins-stat warn"><span class="n">' + n +
      '</span><span class="k">' + escHtml(k) + "</span></div>";
  }
  // Envelopes the server refused, plus sockets it lost: link-level loss that has
  // no op to hang off, so it is reported even when the op list is empty.
  function linkLoss(s) {
    return (s.envelopeRejects || 0) + (s.oversizedMessages || 0) +
      (s.abnormalCloses || 0) + (s.invalidDroppedFields || 0);
  }
  function renderStats(s) {
    if (!s || (!s.ops && !linkLoss(s))) {
      summaryEl.className = "ins-summary empty";
      summaryEl.textContent = "waiting for frames…";
      return;
    }
    // A host that is talking while nothing is ingested must never read as
    // "waiting for frames…": show the refusal counters even with zero ops.
    if (!s.ops) {
      summaryEl.className = "ins-summary";
      summaryEl.innerHTML = statTile(0, "ops") +
        warnTile(s.envelopeRejects || 0, "rejected") +
        warnTile(s.oversizedMessages || 0, "oversized") +
        warnTile(s.abnormalCloses || 0, "socket drops") +
        warnTile(s.invalidDroppedFields || 0, "bad drop count");
      return;
    }
    summaryEl.className = "ins-summary";
    var html = statTile(s.ops, "ops") +
      statTile(s.frames, "frames", s.out + "▶ " + s["in"] + "◀") +
      statTile(fmtBytes(s.bytes), "data") +
      (s.subscriptions
        ? statTile(s.subscriptions, "subs", s.liveSubscriptions > 0 ? s.liveSubscriptions + " live" : "")
        : "") +
      statTile(fmtMs(s.avgDurationMs), "avg op", "max " + fmtMs(s.maxDurationMs) + ", observed") +
      warnTile(s.malformed, "malformed") +
      warnTile(s.orphaned, "orphaned") +
      infoTile(s.unpaired, "unpaired") +
      warnTile(s.retryStorms, "retry storms") +
      warnTile(s.truncated || 0, "truncated") +
      warnTile(s.evictedTraces || 0, "evicted") +
      warnTile(s.droppedByHost || 0, "dropped") +
      warnTile(s.envelopeRejects || 0, "rejected") +
      warnTile(s.oversizedMessages || 0, "oversized") +
      warnTile(s.abnormalCloses || 0, "socket drops");
    if (s.topMethods && s.topMethods.length) {
      var m = '<div class="ins-methods">';
      s.topMethods.forEach(function (t) {
        m += '<span class="ins-method" data-method="' + escHtml(t.method) + '">' +
          escHtml(t.method) + " <b>" + t.count + "</b></span>";
      });
      html += m + "</div>";
    }
    summaryEl.innerHTML = html;
  }
  chanEl.addEventListener("click", function (e) {
    var btn = e.target.closest && e.target.closest(".ins-chan");
    if (!btn) return;
    var c = btn.getAttribute("data-chan");
    channel = c === "" ? null : decodeURIComponent(c);
    lastListHtml = "";   // force a rebuild under the new filter
    poll();
  });

  // Splitter drag.
  var dragging = false;
  document.getElementById("split").addEventListener("pointerdown", function (e) {
    dragging = true; e.target.setPointerCapture(e.pointerId);
    document.body.style.userSelect = "none";
  });
  window.addEventListener("pointermove", function (e) {
    if (!dragging) return;
    var w = Math.max(220, Math.min(e.clientX, window.innerWidth - 320));
    document.body.style.setProperty("--list-w", w + "px");
  });
  window.addEventListener("pointerup", function () {
    dragging = false; document.body.style.userSelect = "";
  });

  function poll() {
    var base = channel === null ? [] : ["channel=" + encodeURIComponent(channel)];
    var q = base.length ? "?" + base.join("&") : "";
    var listP = sortMode ? base.concat("sort=" + encodeURIComponent(sortMode)) : base;
    var listQ = listP.length ? "?" + listP.join("&") : "";
    get("/op-list" + listQ).then(applyList).catch(function () {});
    fetch("/channels").then(function (r) { return r.json(); }).then(renderChannels).catch(function () {});
    fetch("/stats" + q).then(function (r) { return r.json(); }).then(renderStats).catch(function () {});
    // Keep the open op's detail live (a subscription gains receive frames while
    // it stays selected). Re-render only when the fragment actually changed, so
    // a quiet op is not wiped every second.
    if (selectedId) {
      get("/op?id=" + encodeURIComponent(selectedId) +
        (selectedChannel ? "&channel=" + encodeURIComponent(selectedChannel) : "") +
        "&gen=" + encodeURIComponent(selectedGen || "0"))
        .then(function (frag) {
          if (frag === lastDetailHtml) return;
          lastDetailHtml = frag;
          detailEl.innerHTML = frag;
          cursor = -1;
        }).catch(function () {});
    }
  }
  setInterval(poll, 1000);
  poll();
</script>
`;

/**
 * Whether value decode is on, from `TRUAPI_DEBUGGER_DECODE_VALUES`.
 *
 * On by default (dev-only tool); `0`/`false`/`no`/`off` in any case turns it off.
 * TRIMMED first: this is the switch that stops full payload decode, so it must
 * fail CLOSED on the shapes a shell or a `.env` file actually produces -
 * `DECODE_VALUES="0 "` and `DECODE_VALUES=$'false\n'` are how a human writes
 * "off", and an untrimmed match reads both as "on".
 */
export function decodeValuesFromEnv(raw: string | undefined): boolean {
  return !/^(0|false|no|off)$/i.test((raw ?? "").trim());
}

/**
 * The listen port from `TRUAPI_DEBUGGER_PORT`: the value, `DEFAULT_PORT` when
 * unset/empty, or `null` when it is not a usable port.
 *
 * Rejects rather than coerces. `Number.isFinite(x) && x > 0` accepts `99999`,
 * which the OS truncates to a DIFFERENT port (65535) that the host's debug URL
 * will not be pointing at, and `1.5`, which crashes the process on port 1. A
 * silently-wrong port on a debugger is indistinguishable from a host that never
 * dialed - the single most expensive failure this tool can have.
 */
export function portFromEnv(raw: string | undefined): number | null {
  const t = (raw ?? "").trim();
  if (t === "") return DEFAULT_PORT;
  if (!/^\d+$/.test(t)) return null;
  const port = Number(t);
  // 0 would bind an ephemeral port nobody can predict; 65535 is the TCP ceiling.
  return port >= 1 && port <= 65535 ? port : null;
}

// Entry point: `bun run src/server.ts` (or `npm run serve`) starts the server.
// Port comes from TRUAPI_DEBUGGER_PORT, else the default. This is a DEV-ONLY,
// loopback-only tool: value decode is ON by default (set
// TRUAPI_DEBUGGER_DECODE_VALUES to 0/false/no/off to turn decode off for a demo).
if (import.meta.main) {
  const port = portFromEnv(Bun.env.TRUAPI_DEBUGGER_PORT);
  if (port === null) {
    console.error(
      `[truapi-debugger] TRUAPI_DEBUGGER_PORT must be an integer in 1-65535,` +
        ` got ${JSON.stringify(Bun.env.TRUAPI_DEBUGGER_PORT)}`,
    );
    process.exit(1);
  }
  const server = startDebugServer({
    port,
    decodeValues: decodeValuesFromEnv(Bun.env.TRUAPI_DEBUGGER_DECODE_VALUES),
  });
  console.log(
    `[truapi-debugger] listening on http://127.0.0.1:${server.port}` +
      ` (value decode: ${server.decodeValues ? "on" : "off"})`,
  );
}
