// Dev-only link that streams the core's wire frames to a loopback debugger.
// It has no module-level side effects, so it loads cleanly outside a worker.

import { TRUAPI_CODEC_VERSION } from "@parity/truapi";
import { isLoopbackWsUrl } from "./worker-protocol.js";

/** Encode raw frame bytes as base64 (JSON can't carry binary over the WS). */
function toBase64(bytes: Uint8Array): string {
  let binary = "";
  for (let i = 0; i < bytes.length; i++)
    binary += String.fromCharCode(bytes[i]);
  return btoa(binary);
}

/**
 * Envelope version stamped on each frame, mirroring the debugger's
 * `WIRE_ENVELOPE_VERSION`. Kept in sync by hand (a value constant, not a shared
 * dep, to avoid truapi-host depending on the debugger package).
 */
const WIRE_ENVELOPE_VERSION = 1;

/**
 * The wire-contract fingerprint of the core that *encodes* the frames, or
 * `undefined` when this build of the core does not report one.
 *
 * The debugger decodes each frame against a `frameId → method` table, so the
 * `schema` an envelope carries has to be the fingerprint of the table the bytes
 * were encoded with. That is the WASM core's, not `@parity/truapi`'s: the client
 * and the core are separate artifacts, and `dist/wasm/web/` is gitignored and
 * built by hand (`make wasm`), so a stale core beside a fresh client is the
 * everyday case rather than an exotic one. Stamping the client's hash there would
 * make the debugger *confirm* identity on frames from a different table and decode
 * them into the wrong methods and values, silently.
 *
 * When the core does not report a hash, the envelope carries none. The debugger
 * treats an unstamped frame as unconfirmed: it still groups the op, but refuses
 * to decode values. Losing decode until `make wasm` is rerun is the honest
 * outcome; a confident wrong decode is not.
 */
export function coreWireSchemaHash(module: {
  wireSchemaHash?: () => string;
}): string | undefined {
  let hash: unknown;
  try {
    hash = module.wireSchemaHash?.();
  } catch {
    hash = undefined;
  }
  if (typeof hash === "string" && hash.length > 0) return hash;
  console.warn(
    "[truapi] wire debugger: this WASM core does not report its wire-schema hash — frames will stream without a `schema` stamp and the debugger will group them but refuse to decode values (rebuild the core with `make wasm`)",
  );
  return undefined;
}

/**
 * The socket surface the debugger link uses. A `WebSocket` satisfies it; tests
 * substitute a fake to drive backpressure and reconnect timing without a network.
 */
export interface DebuggerSocket {
  /** Bytes handed to the socket that it has not yet put on the wire. */
  readonly bufferedAmount: number;
  send(data: string): void;
  close(): void;
  addEventListener(
    type: "open" | "close" | "error",
    listener: () => void,
  ): void;
}

/** Construction options for {@link createDebuggerLink}. */
export interface DebuggerLinkOptions {
  /**
   * The encoding core's wire-schema hash, from {@link coreWireSchemaHash}. When
   * omitted, envelopes carry no `schema` and the debugger refuses value decode
   * rather than trusting a hash the core never vouched for.
   */
  schema?: string;
  /** Socket factory. Defaults to a real `WebSocket`; tests inject a fake. */
  createSocket?: (url: string) => DebuggerSocket;
  /** Deferred scheduler for reconnect backoff. Defaults to `setTimeout`. */
  schedule?: (run: () => void, delayMs: number) => void;
}

/** Initial reconnect delay; doubles per failed dial up to {@link RECONNECT_MAX_MS}. */
const RECONNECT_BASE_MS = 200;

/** Cap on the reconnect backoff. Mirrors the native sink's `MAX_BACKOFF`. */
const RECONNECT_MAX_MS = 5000;

/**
 * Ceiling on the socket's *own* unflushed send buffer before frames are shed.
 *
 * The queue caps below only bound what this module holds while the socket is
 * down. A socket that is open but whose peer has stopped reading keeps
 * `readyState === OPEN` while `bufferedAmount` grows without limit, and that
 * growth is charged to the observed session's worker: handing frames to it
 * unchecked is the same unbounded buffering the queue caps exist to prevent, one
 * layer lower. Over this ceiling, frames are shed into the counted `dropped`
 * instead.
 */
const MAX_SOCKET_BUFFERED_BYTES = 8 * 1024 * 1024;

/**
 * Ceiling on a SINGLE encoded message, enforced on the live path and again when
 * the backlog drains.
 *
 * `MAX_SOCKET_BUFFERED_BYTES` bounds the socket's cumulative backlog, which one
 * oversized frame passes straight through on an otherwise idle socket. The
 * debugger closes the connection on an over-cap message rather than dropping it
 * (Bun: close 1006, "Received too big message"), so an unshed frame costs the
 * whole stream. Base64 inflates 4/3, so this sits below the server's own limit
 * with room for the envelope's other fields.
 */
const MAX_MESSAGE_BYTES = 6 * 1024 * 1024;

/**
 * Dev-only link to the debugger the host dials. Fire-and-forget by construction:
 * it opens lazily, buffers a bounded backlog until the socket is up, retries a
 * dropped connection with capped backoff, sheds frames (counted) rather than
 * buffering without bound at either layer, and swallows every error - a slow,
 * absent, or crashed debugger only loses the trace, it can never throw into the
 * frame path.
 */
export function createDebuggerLink(
  url: string,
  options: DebuggerLinkOptions = {},
): {
  emit(channelId: string, dir: string, frame: Uint8Array): void;
} {
  // Loopback-only, dev-only: a non-loopback (or non-ws://) debugger URL yields an
  // inert link rather than streaming frames across the network. Warn so a
  // mistyped value reads as "misconfigured", not "the debugger doesn't work".
  if (!isLoopbackWsUrl(url)) {
    console.warn(
      `[truapi] wire debugger URL rejected (must be ws:// on a loopback host): ${url}`,
    );
    return { emit() {} };
  }
  const createSocket =
    options.createSocket ?? ((target: string) => new WebSocket(target));
  const schedule =
    options.schedule ??
    ((run: () => void, delayMs: number) => {
      setTimeout(run, delayMs);
    });
  const schema = options.schema;
  let socket: DebuggerSocket | null = null;
  let open = false;
  const queue: string[] = [];
  // Count *and* byte caps: each queued item is a base64 ProtocolMessage (storage
  // writes, RPC responses - up to MBs each), so a count-only cap would let a slow
  // or absent debugger buffer unbounded RSS on the observed session. Whichever
  // ceiling hits first drops the frame (counted), never blocking the frame path.
  const MAX_QUEUE = 1000;
  const MAX_QUEUE_BYTES = 8 * 1024 * 1024;
  let queuedBytes = 0;
  let droppedSinceSend = 0;
  let reconnectDelayMs = RECONNECT_BASE_MS;
  let reconnectScheduled = false;

  /**
   * Dial again after the current backoff, at most one dial in flight.
   *
   * Without the delay this ran once per frame: a busy session with no debugger
   * listening dialed loopback hundreds of times a second (each refused
   * immediately, each logging a console error), because every emit found
   * `socket === null` and redialled. The native sink has always backed off; this
   * mirrors it.
   */
  function scheduleReconnect(): void {
    if (socket !== null || reconnectScheduled) return;
    reconnectScheduled = true;
    const delayMs = reconnectDelayMs;
    reconnectDelayMs = Math.min(reconnectDelayMs * 2, RECONNECT_MAX_MS);
    try {
      schedule(() => {
        reconnectScheduled = false;
        if (socket === null) connect();
      }, delayMs);
    } catch {
      // No timer available: fall back to redialling on the next emit.
      reconnectScheduled = false;
    }
  }

  /** Drain the backlog onto a freshly opened socket. */
  function flush(): void {
    const pending = queue.splice(0);
    queuedBytes = 0;
    // Deliver drops accumulated while disconnected by stamping the count on the
    // first drained frame - a bare marker without channelId/dir/frame wouldn't
    // parse server-side. Drops only happen once the queue is full, so when the
    // count is nonzero there is always a pending frame to carry it; if not, it
    // rides the next live emit.
    // The count is only cleared once a frame carrying it is actually handed to
    // the socket. Clearing it up front lost the whole gap whenever the drain
    // failed - 1100 shed frames reported as a clean session.
    let carried = 0;
    if (pending.length > 0 && droppedSinceSend > 0) {
      try {
        const first = JSON.parse(pending[0]) as Record<string, unknown>;
        first.dropped = droppedSinceSend;
        pending[0] = JSON.stringify(first);
        carried = droppedSinceSend;
      } catch {
        // Leave the frame as-is; the count rides the next live emit.
      }
    }
    for (const [index, message] of pending.entries()) {
      // The drained path is subject to the same two ceilings as the live one: a
      // wedged socket must not be force-fed the backlog, and an over-cap message
      // closes the debugger's connection and costs every frame after it.
      const open = socket;
      if (
        open === null ||
        open.bufferedAmount > MAX_SOCKET_BUFFERED_BYTES ||
        message.length > MAX_MESSAGE_BYTES
      ) {
        shed();
        continue;
      }
      if (send(message)) {
        if (index === 0) droppedSinceSend -= carried;
      } else {
        shed();
      }
    }
  }

  function connect(): void {
    let dialed: DebuggerSocket;
    try {
      dialed = createSocket(url);
    } catch {
      socket = null;
      scheduleReconnect();
      return;
    }
    socket = dialed;
    dialed.addEventListener("open", () => {
      open = true;
      // A dial that reached the debugger earns the short delay back, so a
      // debugger that restarts is picked up promptly rather than after the cap.
      reconnectDelayMs = RECONNECT_BASE_MS;
      flush();
    });
    dialed.addEventListener("close", () => {
      open = false;
      if (socket === dialed) socket = null;
    });
    dialed.addEventListener("error", () => {
      // A socket that fired `error` is dead: close it explicitly (tidiness), then
      // null it so the next emit schedules a redial. Without the null, a runtime
      // that fires `error` without a following `close` would leave `socket`
      // non-null and frames would buffer then drop.
      open = false;
      if (socket === dialed) socket = null;
      try {
        dialed.close();
      } catch {
        // already closed / closing
      }
    });
  }

  function send(message: string): boolean {
    // A null socket is NOT a success: returning true there would clear the drop
    // count against a frame that went nowhere. Note the residual limit - per
    // WHATWG, `WebSocket.send()` on a CLOSING/CLOSED socket discards silently
    // without throwing, so a `true` here means "handed over", not "delivered".
    const live = socket;
    if (live === null) return false;
    try {
      live.send(message);
      return true;
    } catch {
      // A dead socket must never break the frame path. The caller keeps its
      // pending drop count rather than clearing it against a send that failed -
      // otherwise a gap the host really did cause is reported as no gap at all.
      return false;
    }
  }

  connect();

  let warnedDrop = false;
  /** Shed one frame into the counted backlog gap. */
  function shed(): void {
    droppedSinceSend += 1;
    if (!warnedDrop) {
      // The link buffers a bounded backlog while the debugger is absent/slow, and
      // stops handing frames to a socket that is not draining. Warn once so the
      // gap is attributable to the link, not the host.
      warnedDrop = true;
      console.warn(
        "[truapi] wire debugger link is not keeping up — dropping frames (counted in `dropped`) until it drains",
      );
    }
  }

  return {
    emit(channelId, dir, frame) {
      // A debug tap must never throw into the observed frame path: toBase64 /
      // JSON.stringify can raise on a pathological frame (btoa or V8 string-length
      // limits), and only send() swallows its own errors. Losing a trace is fine;
      // breaking dispatch is not.
      try {
        const live = open ? socket : null;
        // Checked before encoding, so a shed frame costs no base64 either.
        if (live !== null && live.bufferedAmount > MAX_SOCKET_BUFFERED_BYTES) {
          shed();
          return;
        }
        const base = {
          v: WIRE_ENVELOPE_VERSION,
          codec: TRUAPI_CODEC_VERSION,
          // Only when the core vouched for it: see coreWireSchemaHash.
          ...(schema !== undefined ? { schema } : {}),
          channelId,
          dir,
          // The producer is the only party that knows when the frame crossed. The
          // debugger's own clock is the flush instant for anything that waited in
          // the queue below, which collapses every duration in a backlog to 0ms
          // and pulls ops minutes apart into one retry-storm window.
          observedAt: Date.now(),
          frame: toBase64(frame),
        };
        if (live !== null) {
          // Piggyback any frames dropped while the link was down onto the next
          // live frame, so the debugger attributes the gap to the link, not the
          // host.
          const message =
            droppedSinceSend > 0
              ? JSON.stringify({ ...base, dropped: droppedSinceSend })
              : JSON.stringify(base);
          // One over-cap message closes the debugger's socket, taking the whole
          // stream with it. Shedding this frame keeps the rest.
          if (message.length > MAX_MESSAGE_BYTES) {
            shed();
            return;
          }
          if (send(message)) {
            droppedSinceSend = 0;
          } else {
            // The frame just handed over is lost as well, not only the earlier
            // ones: counting the prior gap but not this frame under-reports by
            // exactly the frames whose send failed.
            shed();
          }
          return;
        }
        // Nothing leaves the queue except through flush(), so everything that
        // enters it is by definition replayed rather than live: mark it here and
        // the debugger can tell a backlog gap from a quiet session. Its
        // `observedAt` above is already the real crossing time, so the marker is
        // provenance, not a correction.
        const message = JSON.stringify({ ...base, buffered: true });
        if (
          queue.length < MAX_QUEUE &&
          queuedBytes + message.length <= MAX_QUEUE_BYTES
        ) {
          queue.push(message);
          queuedBytes += message.length;
        } else {
          shed();
        }
        scheduleReconnect();
      } catch {
        // Swallow: never let the tap disturb the frame path.
      }
    },
  };
}
