// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: MIT

import { describe, expect, test } from "bun:test";
import type { ObservedFrame, FrameRole } from "./observed-frame.js";
import type {
  TraceDropCounts,
  WireMethodInfo,
  WireTrace,
} from "./wire-debugger.js";
import { computeTraceStats } from "./session.js";
import {
  isLiveSubscription,
  isSubscription,
  operationMethod,
  wireTraceToView,
} from "./trace-view.js";

function frame(
  role: FrameRole,
  frameId: number,
  timestamp: number,
  extra: Partial<ObservedFrame> = {},
): ObservedFrame {
  return {
    channelId: "host-a.dot",
    direction: role === "response" || role === "receive" ? "in" : "out",
    requestId: "req-1",
    frameId,
    messageType: 0,
    role,
    byteLength: 8,
    timestamp,
    ...extra,
  };
}

function traceOf(
  frames: ObservedFrame[],
  dropped?: TraceDropCounts,
): WireTrace {
  return {
    channelId: "test.dot",
    requestId: "req-1",
    frames,
    startedAt: frames[0]?.timestamp ?? 0,
    lastAt: frames[frames.length - 1]?.timestamp ?? 0,
    generation: 0,
    truncated:
      dropped !== undefined &&
      dropped.framesByCount + dropped.framesByBytes + dropped.payloadsShed > 0,
    dropped: dropped ?? {
      framesByCount: 0,
      framesByBytes: 0,
      payloadsShed: 0,
    },
  };
}

// Fixture frames below carry an explicit `role`, never "unknown", so
// `wireTraceToView` never falls back to resolving it from `kind` here - only
// the method name lookup matters, and `kind` need only type-check.
const methodNames: ReadonlyMap<number, WireMethodInfo> = new Map([
  [22, { method: "account.getAccount", kind: "request" }],
  [23, { method: "account.getAccount", kind: "request" }],
]);

describe("wireTraceToView", () => {
  test("resolves method names and per-frame latency from start", () => {
    const view = wireTraceToView(
      traceOf([frame("request", 22, 1000), frame("response", 23, 1120)]),
      methodNames,
    );
    expect(view.frames.map((f) => f.method)).toEqual([
      "account.getAccount",
      "account.getAccount",
    ]);
    expect(view.frames[0].latencyFromStartMs).toBe(0);
    expect(view.frames[1].latencyFromStartMs).toBe(120);
    expect(view.durationMs).toBe(120);
  });

  test("a matched response carries a round-trip and no orphan badge", () => {
    const view = wireTraceToView(
      traceOf([frame("request", 22, 1000), frame("response", 23, 1150)]),
      methodNames,
    );
    expect(view.frames[1].roundTripMs).toBe(150);
    expect(view.badges).toEqual([]);
  });

  test("a request with no response is orphaned", () => {
    const view = wireTraceToView(traceOf([frame("request", 22, 1000)]));
    expect(view.frames[0].badges).toContain("orphaned");
    expect(view.badges).toContain("orphaned");
  });

  test("a response with no request is unpaired, NOT orphaned", () => {
    // `orphaned` means "went out and nothing came back", which an operator reads
    // as a host fault. A closer with no opener is a different fact and usually
    // not the host's doing - most often the debugger attached mid-op. Asserting
    // the absence matters as much as the presence: conflating the two is what
    // made the badge report the debugger's own retention as a protocol defect.
    const view = wireTraceToView(traceOf([frame("response", 23, 1000)]));
    expect(view.frames[0].badges).toContain("unpaired");
    expect(view.frames[0].badges).not.toContain("orphaned");
    expect(view.badges).toContain("unpaired");
    expect(view.badges).not.toContain("orphaned");
  });

  test("unpaired is counted per OP and never inflates orphaned", () => {
    // The aggregate is the whole reason `unpaired` is surfaced rather than
    // dropped: a genuine second answer is a real host defect, and before it was
    // counted it appeared in no roll-up at all - only in one op's row. Counts ops,
    // not frames, so a trace with three opener-less closers is one.
    const coldStart = wireTraceToView(
      traceOf([
        frame("response", 23, 1000),
        frame("response", 23, 1100),
        frame("response", 23, 1200),
      ]),
    );
    const hung = wireTraceToView(traceOf([frame("request", 22, 1000)]));
    const clean = wireTraceToView(
      traceOf([frame("request", 22, 1000), frame("response", 23, 1100)]),
    );
    const stats = computeTraceStats([coldStart, hung, clean], {});
    expect(stats.unpaired).toBe(1);
    expect(stats.orphaned).toBe(1);
    expect(stats.ops).toBe(3);
  });

  test("an opener-less closer reads the same whatever preceded it", () => {
    // Both traces end in a closer with no opener on the stack. The first genuinely
    // is a second answer to the request above it; the second's opener is simply
    // off this debugger's table (frame 999 resolves to role "unknown", so it is
    // never recognised as an opener). They MUST get the same badge, because
    // positional pairing cannot tell them apart.
    //
    // A previous attempt split these on `matched.size > 0`. That is trace-global,
    // so any earlier answered op in the same trace flipped the second shape to
    // "the host answered twice" - a fabricated protocol defect. This test fails if
    // that discriminator comes back.
    const secondAnswer = wireTraceToView(
      traceOf([
        frame("request", 22, 1000),
        frame("response", 23, 1100),
        frame("response", 23, 1200),
      ]),
    );
    const offTableOpener = wireTraceToView(
      traceOf([
        frame("request", 22, 1000),
        frame("response", 23, 1100),
        frame("unknown", 999, 1150),
        frame("response", 23, 1200),
      ]),
    );
    for (const view of [secondAnswer, offTableOpener]) {
      const last = view.frames[view.frames.length - 1];
      expect(last.badges).toContain("unpaired");
      expect(last.badges).not.toContain("orphaned");
      expect(view.badges).toContain("unpaired");
      expect(view.badges).not.toContain("orphaned");
    }
    // The answered request keeps a clean bill in both: it was answered.
    expect(secondAnswer.frames[0].badges).not.toContain("orphaned");
    expect(offTableOpener.frames[0].badges).not.toContain("orphaned");
  });

  test("subscription: one opener stays open across many receives, no orphans", () => {
    const view = wireTraceToView(
      traceOf([
        frame("start", 40, 1000),
        frame("receive", 41, 1100),
        frame("receive", 41, 1200),
        frame("stop", 42, 1300),
      ]),
    );
    expect(view.badges).toEqual([]);
    // Each receive round-trips against the shared opener.
    expect(view.frames[1].roundTripMs).toBe(100);
    expect(view.frames[2].roundTripMs).toBe(200);
  });

  test("a live subscription (start + receives, no stop) is not orphaned", () => {
    const view = wireTraceToView(
      traceOf([
        frame("start", 40, 1000),
        frame("receive", 41, 1100),
        frame("receive", 41, 1200),
      ]),
    );
    expect(view.badges).not.toContain("orphaned");
    expect(view.frames[0].badges).not.toContain("orphaned");
  });

  test("a subscribe that never delivered is orphaned", () => {
    const view = wireTraceToView(traceOf([frame("start", 40, 1000)]));
    expect(view.frames[0].badges).toContain("orphaned");
    expect(view.badges).toContain("orphaned");
  });

  test("a malformed frame flags both the frame and the op", () => {
    const view = wireTraceToView(
      traceOf([frame("request", 22, 1000), frame("malformed", -1, 1010)]),
    );
    expect(view.frames[1].badges).toContain("malformed");
    expect(view.badges).toContain("malformed");
  });

  test("retain-bytes drives the decodable flag", () => {
    const view = wireTraceToView(
      traceOf([
        frame("request", 22, 1000, { bytes: new Uint8Array([1, 2, 3]) }),
        frame("response", 23, 1100),
      ]),
      methodNames,
    );
    expect(view.frames[0].decodable).toBe(true);
    expect(view.frames[1].decodable).toBe(false);
  });

  test("caller-supplied op badges (retry-storm) are merged", () => {
    const view = wireTraceToView(
      traceOf([frame("request", 22, 1000), frame("response", 23, 1100)]),
      methodNames,
      ["retry-storm"],
    );
    expect(view.badges).toContain("retry-storm");
  });
});

describe("wireTraceToView — truncation", () => {
  test("an op whose frames were evicted is truncated, never orphaned", () => {
    // The engine dropped the frames that would have answered the opener, so
    // "no close observed" no longer means "no close happened". Blaming the op for
    // the engine's own eviction invents a dropped call (and, in the op list, a
    // call still waiting) out of a completed one.
    const view = wireTraceToView(
      traceOf([frame("request", 22, 1000)], {
        framesByCount: 0,
        framesByBytes: 3,
        payloadsShed: 0,
      }),
      methodNames,
    );
    expect(view.badges).toContain("truncated");
    expect(view.badges).not.toContain("orphaned");
    expect(view.frames[0].badges).not.toContain("orphaned");
  });

  test("a shed payload does not suppress the orphan verdict", () => {
    // Shedding drops bytes, not frames: the sequence is complete, so a request
    // with no response really is unanswered.
    const view = wireTraceToView(
      traceOf([frame("request", 22, 1000)], {
        framesByCount: 0,
        framesByBytes: 0,
        payloadsShed: 1,
      }),
      methodNames,
    );
    expect(view.badges).toContain("truncated");
    expect(view.badges).toContain("orphaned");
  });

  test("a closer with no opener stays unpaired under eviction", () => {
    // Eviction suppresses OPENER verdicts, because the frames that answered an
    // opener may be the ones that were dropped. It must not suppress this one:
    // retention spares the opener, so a trace whose only frame is a closer never
    // had an opener in view, and saying so is accurate whether or not frames were
    // dropped. Still `unpaired` rather than `orphaned` - see the test above.
    const view = wireTraceToView(
      traceOf([frame("response", 23, 1000)], {
        framesByCount: 5,
        framesByBytes: 0,
        payloadsShed: 0,
      }),
      methodNames,
    );
    expect(view.frames[0].badges).toContain("unpaired");
    expect(view.frames[0].badges).not.toContain("orphaned");
  });

  test("per-axis drop counts reach the view for a mount to report", () => {
    const view = wireTraceToView(
      traceOf([frame("start", 40, 1000)], {
        framesByCount: 77,
        framesByBytes: 4,
        payloadsShed: 1,
      }),
    );
    expect(view.dropped).toEqual({
      framesByCount: 77,
      framesByBytes: 4,
      payloadsShed: 1,
    });
  });

  test("an un-truncated trace carries neither the badge nor a nonzero count", () => {
    const view = wireTraceToView(
      traceOf([frame("request", 22, 1000), frame("response", 23, 1100)]),
      methodNames,
    );
    expect(view.badges).not.toContain("truncated");
    expect(view.dropped).toEqual({
      framesByCount: 0,
      framesByBytes: 0,
      payloadsShed: 0,
    });
  });
});

describe("operationMethod — the single definition of an op's name", () => {
  test("the opener's method wins over a later frame's", () => {
    const view = wireTraceToView(
      traceOf([frame("request", 22, 1000), frame("response", 23, 1100)]),
      methodNames,
    );
    expect(operationMethod(view)).toBe("account.getAccount");
  });

  test("falls back to the first frame that resolves a method", () => {
    // The opener's id was off this debugger's table; a later frame's was not.
    const view = wireTraceToView(
      traceOf([frame("unknown", 999, 1000), frame("response", 23, 1100)]),
      methodNames,
    );
    expect(operationMethod(view)).toBe("account.getAccount");
  });

  test("undefined when no frame resolves a method, so callers choose the placeholder", () => {
    const view = wireTraceToView(traceOf([frame("unknown", 999, 1000)]));
    expect(operationMethod(view)).toBeUndefined();
  });
});

describe("isLiveSubscription — the single definition of a live sub", () => {
  test("start + receives with no terminator is live", () => {
    const view = wireTraceToView(
      traceOf([frame("start", 40, 1000), frame("receive", 41, 1100)]),
    );
    expect(isSubscription(view)).toBe(true);
    expect(isLiveSubscription(view)).toBe(true);
  });

  test("a product stop ends it", () => {
    const view = wireTraceToView(
      traceOf([
        frame("start", 40, 1000),
        frame("receive", 41, 1100),
        frame("stop", 42, 1200),
      ]),
    );
    expect(isLiveSubscription(view)).toBe(false);
  });

  test("a host interrupt ends it too", () => {
    // A host-terminated subscription that only `stop` closes out reads "live"
    // forever, and every consumer counting live subs climbs monotonically.
    const view = wireTraceToView(
      traceOf([
        frame("start", 40, 1000),
        frame("receive", 41, 1100),
        frame("interrupt", 43, 1200),
      ]),
    );
    expect(isLiveSubscription(view)).toBe(false);
  });

  test("a request/response op is not a subscription and never live", () => {
    const view = wireTraceToView(
      traceOf([frame("request", 22, 1000), frame("response", 23, 1100)]),
      methodNames,
    );
    expect(isSubscription(view)).toBe(false);
    expect(isLiveSubscription(view)).toBe(false);
  });

  test("receives with no observed start still count as a subscription", () => {
    // The debugger attached mid-session: the `start` predates it.
    const view = wireTraceToView(
      traceOf([frame("receive", 41, 1000), frame("receive", 41, 2000)]),
    );
    expect(isSubscription(view)).toBe(true);
    expect(isLiveSubscription(view)).toBe(true);
  });
});
