import { describe, expect, test } from "bun:test";

import { encodeWireMessage } from "@parity/truapi";
import * as REAL_W from "@parity/truapi/wire-table";

import { createDebugIngest, DEFAULT_MAX_ID_CHARS, normalizeId } from "./ingest.js";
import type { DebugFrameEnvelope } from "./ingest.js";
import type { ObservedFrame } from "./observed-frame.js";
import { detectRetryStorms } from "./retry-storm.js";
import {
  createMethodNameMap,
  createWireDebugger,
  frameIdOf,
} from "./wire-debugger.js";

/**
 * Test-only convenience view over the generated wire table: gives each entry
 * per-leg "flat id" properties by encoding `(frameId, messageType)` as one
 * number (`frameId * 4 + messageType`), so fixtures can address a method's
 * specific leg by name (`W.ACCOUNT_GET_ACCOUNT.request`) instead of a bare
 * pair. `envelope()` below decodes a leg id back into a real `(trait,
 * method)` pair and a `messageType` byte.
 */
function legacyIds(table: Record<string, unknown>): Record<string, Record<string, number>> {
  const legs = {
    request: ["request", "response"],
    subscription: ["start", "receive", "interrupt", "stop"],
  } as const;
  const out: Record<string, Record<string, number>> = {};
  for (const [name, entry] of Object.entries(table)) {
    if (
      entry === null ||
      typeof entry !== "object" ||
      !("trait" in entry) ||
      !("method" in entry) ||
      !("kind" in entry)
    ) {
      continue;
    }
    const { trait, method, kind } = entry as {
      trait: number;
      method: number;
      kind: "request" | "subscription";
    };
    const base = frameIdOf(trait, method) * 4;
    const rec: Record<string, number> = {};
    legs[kind].forEach((leg, i) => {
      rec[leg] = base + i;
    });
    out[name] = rec;
  }
  return out;
}

const W = legacyIds(REAL_W as unknown as Record<string, unknown>);

/** The real generated table, keyed the way `createDebugSession` keys it. */
const METHOD_NAMES = createMethodNameMap(
  REAL_W as unknown as Record<string, unknown>,
  ["account", "signing", "chain", "chat", "resourceAllocation"],
);

/**
 * One host-tap envelope carrying `legId` (a leg id from {@link legacyIds})
 * under correlation id `requestId`. `innerValue` is the leg's own payload
 * bytes, unrelated to which leg `messageType` says this is.
 */
function envelope(
  requestId: string,
  legId: number,
  innerValue = new Uint8Array([0]),
  dir: "in" | "out" = "out",
  channelId = "myapp.dot",
): DebugFrameEnvelope {
  const frameId = Math.floor(legId / 4);
  const messageType = legId % 4;
  const encoded = encodeWireMessage({
    requestId,
    payload: {
      traitId: Math.floor(frameId / 256),
      methodId: frameId % 256,
      messageType,
      value: innerValue,
    },
  });
  if (encoded.isErr()) throw encoded.error;
  return { channelId, dir, frame: encoded.value };
}

/**
 * One envelope as a host tap replays it out of its backlog: `buffered`, with the
 * producer's own `observedAt` rather than the flush instant.
 */
function flushed(
  observedAt: number | undefined,
  requestId: string,
  frameId: number,
  dir: "in" | "out" = "out",
): DebugFrameEnvelope {
  return {
    ...envelope(requestId, frameId, new Uint8Array([0]), dir),
    ...(observedAt === undefined ? {} : { observedAt }),
    buffered: true,
  };
}

/** Collect every frame an ingest emits. */
function collect(options: Parameters<typeof createDebugIngest>[1] = {}) {
  const seen: ObservedFrame[] = [];
  return { seen, ingest: createDebugIngest((f) => seen.push(f), options) };
}

describe("ingest resolves role from the wire table", () => {
  test("role is a pure function of frameId, across every leg of a method", () => {
    const { seen, ingest } = collect({ methodNames: METHOD_NAMES });

    // A request/response pair and a subscription's start/receive legs. Each id
    // carries its own role on the wire table; none of them needs correlation
    // state, and they arrive here out of any lifecycle order on purpose.
    ingest(envelope("p:1", W.ACCOUNT_GET_ACCOUNT.response));
    ingest(envelope("p:1", W.ACCOUNT_GET_ACCOUNT.request));
    ingest(envelope("p:2", W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.receive));
    ingest(envelope("p:2", W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.start));

    expect(seen.map((f) => f.role)).toEqual([
      "response",
      "request",
      "receive",
      "start",
    ]);
  });

  test("an off-table id and a map-less ingest both fall back to unknown", () => {
    const withMap = collect({ methodNames: METHOD_NAMES });
    // (255, 255) is the reserved protocol-error address; no real method lives there.
    withMap.ingest(envelope("p:1", (255 * 256 + 255) * 4));
    expect(withMap.seen[0]?.role).toBe("unknown");

    const withoutMap = collect();
    withoutMap.ingest(envelope("p:1", W.ACCOUNT_GET_ACCOUNT.request));
    expect(withoutMap.seen[0]?.role).toBe("unknown");
  });

  test("an undecodable frame is a malformed sentinel, not a drop", () => {
    const { seen, ingest } = collect({ methodNames: METHOD_NAMES });
    ingest({ channelId: "myapp.dot", dir: "out", frame: new Uint8Array([0xff]) });

    expect(seen).toHaveLength(1);
    expect(seen[0]).toMatchObject({
      role: "malformed",
      requestId: "malformed",
      frameId: -1,
      byteLength: 1,
    });
  });
});

describe("every consumer sees the resolved role, not just the view adapter", () => {
  test("the formatted sink line names the role, not 'unknown'", () => {
    const lines: string[] = [];
    const wireDebugger = createWireDebugger({
      methodNames: METHOD_NAMES,
      sink: (line) => lines.push(line),
    });
    const ingest = createDebugIngest(wireDebugger.observe, {
      methodNames: METHOD_NAMES,
    });

    ingest(envelope("p:1", W.ACCOUNT_GET_ACCOUNT.request));

    // This is the line the default `console.debug` sink prints. It read
    // "-> unknown account.getAccount" while role was resolved only downstream.
    expect(lines[0]).toBe(
      `[wire p:1] → request account.getAccount (id=${String(frameIdOf(REAL_W.ACCOUNT_GET_ACCOUNT.trait, REAL_W.ACCOUNT_GET_ACCOUNT.method))}, 1B)`,
    );
  });

  test("the forward hook receives the resolved role", () => {
    const forwarded: ObservedFrame[] = [];
    const wireDebugger = createWireDebugger({
      methodNames: METHOD_NAMES,
      sink: () => {},
      forward: (frame) => forwarded.push(frame),
    });
    const ingest = createDebugIngest(wireDebugger.observe, {
      methodNames: METHOD_NAMES,
    });

    ingest(envelope("p:1", W.ACCOUNT_GET_ACCOUNT.request));

    expect(forwarded).toHaveLength(1);
    expect(forwarded[0]?.role).toBe("request");
  });
});

describe("ingest bounds ids and gates raw bytes", () => {
  test("channelId and requestId over the bound are digested, not sliced", () => {
    const long = "x".repeat(DEFAULT_MAX_ID_CHARS + 100);
    const { seen, ingest } = collect({ methodNames: METHOD_NAMES });

    ingest(
      envelope(long, W.ACCOUNT_GET_ACCOUNT.request, new Uint8Array([0]), "out", long),
    );

    // A slice of the id would be a prefix of it and would keep the whole 356-char
    // parent string alive (JSC/V8 both back `slice` with a view of the parent, so
    // a 250k-char id retains 250k chars while accounting for 256). The digest
    // references nothing.
    for (const id of [seen[0]?.channelId, seen[0]?.requestId]) {
      expect(id).toBe(normalizeId(long));
      expect(long.startsWith(id ?? "")).toBe(false);
      expect((id ?? "").length).toBeLessThan(40);
      // The length the host actually sent stays visible to the operator.
      expect(id).toContain(`:${String(long.length)}`);
    }
  });

  test("two ids sharing the bound-length prefix stay two ops", () => {
    // The consequence of truncating: these differ only past the cap, so they
    // clamped to the same key, merged into one trace, and manufactured a
    // roundTripMs between two unrelated ops (while clearing the `orphaned` badge
    // each of them had earned).
    const shared = "x".repeat(DEFAULT_MAX_ID_CHARS);
    const wireDebugger = createWireDebugger({
      methodNames: METHOD_NAMES,
      sink: () => {},
    });
    const ingest = createDebugIngest(wireDebugger.observe, {
      methodNames: METHOD_NAMES,
    });

    ingest(envelope(`${shared}a`, W.ACCOUNT_GET_ACCOUNT.request));
    ingest(envelope(`${shared}b`, W.ACCOUNT_GET_ACCOUNT.request));

    const traces = wireDebugger.traces();
    expect(traces).toHaveLength(2);
    expect(new Set(traces.map((t) => t.requestId)).size).toBe(2);
  });

  test("ids within the bound are passed through untouched", () => {
    const { seen, ingest } = collect({ methodNames: METHOD_NAMES });
    ingest(envelope("p:1", W.ACCOUNT_GET_ACCOUNT.request));
    expect(seen[0]?.requestId).toBe("p:1");
    expect(seen[0]?.channelId).toBe("myapp.dot");
    expect(normalizeId("x".repeat(DEFAULT_MAX_ID_CHARS))).toHaveLength(
      DEFAULT_MAX_ID_CHARS,
    );
  });

  test("maxIdChars overrides the default bound", () => {
    const { seen, ingest } = collect({ maxIdChars: 4 });
    ingest(envelope("p:1234567890", W.ACCOUNT_GET_ACCOUNT.request));
    expect(seen[0]?.requestId).toBe(normalizeId("p:1234567890", 4));
    expect(seen[0]?.requestId).not.toBe("p:12");
  });

  test("raw bytes are attached only under retainBytes", () => {
    const off = collect({ methodNames: METHOD_NAMES });
    off.ingest(envelope("p:1", W.ACCOUNT_GET_ACCOUNT.request, new Uint8Array([7])));
    expect(off.seen[0]?.bytes).toBeUndefined();
    // Byte length is recorded either way: the 1 inner byte.
    expect(off.seen[0]?.byteLength).toBe(1);

    const on = collect({ methodNames: METHOD_NAMES, retainBytes: true });
    on.ingest(envelope("p:1", W.ACCOUNT_GET_ACCOUNT.request, new Uint8Array([7])));
    expect(Array.from(on.seen[0]?.bytes ?? [])).toEqual([7]);
  });

  test("the product-vantage direction is carried through untouched", () => {
    const { seen, ingest } = collect({ methodNames: METHOD_NAMES });
    ingest(envelope("p:1", W.ACCOUNT_GET_ACCOUNT.request, new Uint8Array([0]), "out"));
    ingest(envelope("p:1", W.ACCOUNT_GET_ACCOUNT.response, new Uint8Array([0]), "in"));
    expect(seen.map((f) => f.direction)).toEqual(["out", "in"]);
  });
});

/**
 * A host tap buffers a backlog while the debugger is absent and flushes it in one
 * loop on connect. If the ingest clock is the only clock, that loop stamps every
 * frame of the whole session with the same instant: durations collapse to 0ms and
 * ops minutes apart fall inside the retry-storm window. These cover both halves
 * against the real trace engine and the real storm detector.
 */
describe("a flushed backlog keeps the producer's clock, not the flush instant", () => {
  /** Feed envelopes through a real ingest into a real trace engine. */
  function traceEngine() {
    const wireDebugger = createWireDebugger({
      methodNames: METHOD_NAMES,
      sink: () => {},
    });
    return {
      traces: () => wireDebugger.traces(),
      ingest: createDebugIngest(wireDebugger.observe, {
        methodNames: METHOD_NAMES,
      }),
    };
  }

  test("a 500ms round trip stays 500ms after the flush", () => {
    const engine = traceEngine();
    // One op whose two frames genuinely crossed 500ms apart, both replayed out of
    // the backlog in the same loop long afterwards.
    engine.ingest(flushed(1_000_000, "p:1", W.ACCOUNT_GET_ACCOUNT.request, "out"));
    engine.ingest(flushed(1_000_500, "p:1", W.ACCOUNT_GET_ACCOUNT.response, "in"));

    const [trace] = engine.traces();
    expect(trace?.lastAt - trace?.startedAt).toBe(500);
    expect(trace?.frames.map((f) => f.timestamp)).toEqual([1_000_000, 1_000_500]);
    // The frames say where their clock came from, and that they were replayed.
    expect(trace?.frames.every((f) => f.timestampFromProducer === true)).toBe(true);
    expect(trace?.frames.every((f) => f.buffered === true)).toBe(true);
  });

  test("six ops ten seconds apart are not a retry storm", () => {
    const engine = traceEngine();
    // Six `account.getAccount` calls, one every 10s: a calm session by any
    // reading. Flushed together, an ingest-stamped clock puts all six inside the
    // detector's 1000ms window and badges every row "retry storm".
    for (let i = 0; i < 6; i++) {
      engine.ingest(
        flushed(1_000_000 + i * 10_000, `p:${String(i)}`, W.ACCOUNT_GET_ACCOUNT.request),
      );
    }

    const traces = engine.traces();
    expect(traces).toHaveLength(6);
    expect(detectRetryStorms(traces).size).toBe(0);
  });

  test("a genuine burst is still detected through a flush", () => {
    const engine = traceEngine();
    // The same six ops 100ms apart really are a storm: preserving the producer's
    // clock must not blunt the signal, only stop fabricating it.
    for (let i = 0; i < 6; i++) {
      engine.ingest(
        flushed(1_000_000 + i * 100, `p:${String(i)}`, W.ACCOUNT_GET_ACCOUNT.request),
      );
    }

    expect(detectRetryStorms(engine.traces()).size).toBe(6);
  });

  test("a tap that stamps no time falls back to the ingest clock and marks the frame", () => {
    const { seen, ingest } = collect({ methodNames: METHOD_NAMES });
    const before = Date.now();
    ingest(flushed(undefined, "p:1", W.ACCOUNT_GET_ACCOUNT.request));

    // Nothing better exists for such a frame, so `timestamp` is the flush instant
    // - but it is flagged `buffered` with no `timestampFromProducer`, which is the
    // pair a consumer keys on to suppress its duration and its storm
    // participation.
    expect(seen[0]?.timestamp).toBeGreaterThanOrEqual(before);
    expect(seen[0]?.timestampFromProducer).toBeUndefined();
    expect(seen[0]?.buffered).toBe(true);
  });

  test("a live frame is neither buffered nor producer-stamped", () => {
    const { seen, ingest } = collect({ methodNames: METHOD_NAMES });
    ingest(envelope("p:1", W.ACCOUNT_GET_ACCOUNT.request));
    expect(seen[0]?.buffered).toBeUndefined();
    expect(seen[0]?.timestampFromProducer).toBeUndefined();
  });

  test("an unusable observedAt is refused, not trusted into the trace list", () => {
    // Anything reaching the tap can put anything here, and it feeds ordering and
    // every duration.
    for (const observedAt of [0, -1, Number.NaN, Infinity, -Infinity]) {
      const { seen, ingest } = collect({ methodNames: METHOD_NAMES });
      const before = Date.now();
      ingest({
        ...envelope("p:1", W.ACCOUNT_GET_ACCOUNT.request),
        observedAt,
      });
      expect(seen[0]?.timestampFromProducer).toBeUndefined();
      expect(seen[0]?.timestamp).toBeGreaterThanOrEqual(before);
    }
  });

  test("a malformed frame carries the same provenance as a decodable one", () => {
    const { seen, ingest } = collect({ methodNames: METHOD_NAMES });
    ingest({
      channelId: "myapp.dot",
      dir: "out",
      frame: new Uint8Array([0xff]),
      observedAt: 1_000_000,
      buffered: true,
    });
    expect(seen[0]).toMatchObject({
      role: "malformed",
      timestamp: 1_000_000,
      timestampFromProducer: true,
      buffered: true,
    });
  });
});
