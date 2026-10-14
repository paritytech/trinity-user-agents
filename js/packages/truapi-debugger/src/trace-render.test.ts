// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: MIT

import { describe, expect, test } from "bun:test";
import type { FrameValueDetail } from "./decode.js";
import type { FrameRole, ObservedFrame } from "./observed-frame.js";
import type { TraceView } from "./trace-view.js";
import { wireTraceToView } from "./trace-view.js";
import type {
  TraceDropCounts,
  WireMethodInfo,
  WireTrace,
} from "./wire-debugger.js";
import {
  renderFrameValueDetail,
  renderOperationRow,
  renderTraceDetail,
} from "./trace-render.js";

/** Wire ids for one unary method and one subscription, as the wire table has them. */
const WIRE: ReadonlyMap<number, WireMethodInfo> = new Map([
  [22, { method: "account.getAccount", kind: "request" }],
  [23, { method: "account.getAccount", kind: "request" }],
  [40, { method: "account.connectionStatus", kind: "subscription" }],
  [41, { method: "account.connectionStatus", kind: "subscription" }],
  [42, { method: "account.connectionStatus", kind: "subscription" }],
  [43, { method: "account.connectionStatus", kind: "subscription" }],
]);

/**
 * The `messageType` each fixture id below stands in for (see `resolveRole`):
 * 22/23 are `request`-kind's two legs, 40/41/43/42 are `subscription`-kind's
 * four (`start`/`receive`/`interrupt`/`stop` in that order).
 */
const MESSAGE_TYPE: Readonly<Record<number, number>> = {
  22: 0, // request
  23: 1, // response
  40: 0, // start
  41: 1, // receive
  43: 2, // interrupt
  42: 3, // stop
};

/**
 * Build a view the way a mount does - through the wire adapter - so the badges
 * under test are the ones the engine really assigns, not hand-written ones.
 */
function viewOf(
  frames: readonly [number, number][],
  dropped?: TraceDropCounts,
): TraceView {
  const observed: ObservedFrame[] = frames.map(([frameId, timestamp]) => ({
    channelId: "localhost:3000",
    // Real ingest cannot know the lifecycle role without a methodNames map;
    // the adapter resolves it from the frame id's wire-table kind plus the
    // frame's own `messageType` (see resolveRole).
    role: "unknown" as FrameRole,
    direction: "out",
    requestId: "p:1",
    frameId,
    messageType: MESSAGE_TYPE[frameId] ?? 0,
    byteLength: 8,
    timestamp,
  }));
  const trace: WireTrace = {
    channelId: "localhost:3000",
    requestId: "p:1",
    generation: 0,
    frames: observed,
    startedAt: observed[0]?.timestamp ?? 0,
    lastAt: observed[observed.length - 1]?.timestamp ?? 0,
    truncated: dropped !== undefined,
    dropped: dropped ?? {
      framesByCount: 0,
      framesByBytes: 0,
      payloadsShed: 0,
    },
  };
  return wireTraceToView(trace, WIRE);
}

const view: TraceView = {
  requestId: "req-1",
  startedAt: 1000,
  lastAt: 1150,
  durationMs: 150,
  frames: [
    {
      seq: 0,
      direction: "out",
      role: "request",
      method: "account.getAccount",
      frameId: 22,
      byteLength: 8,
      timestamp: 1000,
      latencyFromStartMs: 0,
      badges: [],
      decodable: true,
    },
    {
      seq: 1,
      direction: "in",
      role: "response",
      method: "account.getAccount",
      frameId: 23,
      byteLength: 40,
      timestamp: 1150,
      latencyFromStartMs: 150,
      roundTripMs: 150,
      badges: [],
      decodable: true,
    },
  ],
  badges: [],
};

describe("renderTraceDetail", () => {
  test("renders the frame sequence with method, bytes, and round-trip", () => {
    const html = renderTraceDetail(view);
    expect(html).toContain("account.getAccount");
    expect(html).toContain("40B");
    expect(html).toContain("150ms");
    expect(html).toContain('data-seq="1"');
  });

  test("is payload-blind by default: no decode control", () => {
    const html = renderTraceDetail(view);
    expect(html).not.toContain("decode payload");
  });

  test("shows byte length for a decodable frame with no resolved value", () => {
    // Decode on but no value supplied for the frame: it falls back to its size,
    // never a click-to-decode control (a dev-only tool decodes up front).
    const html = renderTraceDetail(view, { offerDecode: true });
    expect(html).not.toContain("td-frame-decode-btn");
    expect(html).toContain("payload not shown");
  });

  test("renders a resolved decoded value in place of the control", () => {
    const decoded = new Map<number, FrameValueDetail>([
      [1, { kind: "decoded", value: { free: 42 } }],
    ]);
    const html = renderTraceDetail(view, { offerDecode: true, decoded });
    expect(html).toContain("&quot;free&quot;: 42");
  });

  test("a bytes-only detail shows byte length, never a value", () => {
    const decoded = new Map<number, FrameValueDetail>([
      [0, { kind: "bytes", byteLength: 96 }],
    ]);
    const html = renderTraceDetail(view, { offerDecode: true, decoded });
    expect(html).toContain("96B");
    expect(html).toContain("payload not shown");
    expect(html).not.toContain("free");
  });

  test("escapes wire-sourced strings", () => {
    const evil: TraceView = {
      ...view,
      requestId: '<img src=x onerror="alert(1)">',
      frames: [],
    };
    const html = renderTraceDetail(evil);
    expect(html).not.toContain("<img");
    expect(html).toContain("&lt;img");
  });

  test("op-level badges appear in the header", () => {
    const html = renderTraceDetail({
      ...view,
      badges: ["orphaned", "retry-storm"],
    });
    expect(html).toContain("td-badge-orphaned");
    expect(html).toContain("retry storm");
  });
});

describe("renderFrameValueDetail", () => {
  test("bytes-only with no retained hex shows byte length only", () => {
    const html = renderFrameValueDetail({ kind: "bytes", byteLength: 12 });
    expect(html).toContain("12B");
    expect(html).toContain("payload not shown");
  });

  test("bytes with retained hex shows the raw hex, never 'payload not shown'", () => {
    const html = renderFrameValueDetail({
      kind: "bytes",
      byteLength: 3,
      hex: "0x010203",
    });
    expect(html).toContain("0x010203");
    expect(html).not.toContain("payload not shown");
  });
});

describe("renderOperationRow — an unanswered op reports how long it has waited", () => {
  /** A request that went out and got nothing back: the shape of a hung call. */
  const unanswered: TraceView = {
    requestId: "p:4",
    channelId: "localhost:3000",
    startedAt: 1_000,
    lastAt: 1_000,
    // One frame, so last === started and the honest span really is 0.
    durationMs: 0,
    frames: [
      {
        seq: 0,
        direction: "out",
        role: "request",
        method: "account.getAccountAlias",
        frameId: 24,
        byteLength: 97,
        timestamp: 1_000,
        latencyFromStartMs: 0,
        decodable: false,
        badges: ["orphaned"],
      },
    ],
    badges: ["orphaned"],
  };

  test("counts up from the request instead of reporting 0ms", () => {
    // 45s after the request went out, with no reply.
    const html = renderOperationRow(unanswered, { now: 46_000 });
    expect(html).toContain("waiting 45.00s");
    expect(html).not.toContain("· 0ms");
    // Flagged so the row can be styled as a problem, not a fast success.
    expect(html).toContain("td-op-waiting");
  });

  test("the wait grows as the call stays unanswered", () => {
    const early = renderOperationRow(unanswered, { now: 3_000 });
    const later = renderOperationRow(unanswered, { now: 30_000 });
    expect(early).toContain("waiting 2.00s");
    expect(later).toContain("waiting 29.00s");
  });

  test("without a clock it falls back to the recorded span", () => {
    // Callers that cannot supply a clock (or replay a fixed trace) keep the old
    // behaviour rather than inventing a time.
    const html = renderOperationRow(unanswered);
    expect(html).toContain("0ms");
    expect(html).not.toContain("waiting");
    expect(html).not.toContain("td-op-waiting");
  });

  test("an answered op still shows its real round trip, not a wait", () => {
    const answered: TraceView = {
      ...unanswered,
      requestId: "p:2",
      lastAt: 1_150,
      durationMs: 150,
      frames: [
        { ...unanswered.frames[0]!, badges: [] },
        {
          seq: 1,
          direction: "in",
          role: "response",
          method: "account.getAccount",
          frameId: 23,
          byteLength: 35,
          // Distinct from the request it answers: identical timestamps would make
          // this fixture describe an impossible 0ms reply if it is ever fed to
          // renderTraceDetail, which does read these.
          timestamp: 1_120,
          latencyFromStartMs: 120,
          decodable: false,
          badges: [],
        },
      ],
      badges: [],
    };
    const html = renderOperationRow(answered, { now: 999_999 });
    expect(html).toContain("150ms");
    expect(html).not.toContain("waiting");
  });

  test("an unanswered subscribe (orphaned start) also counts up", () => {
    // The true-positive on the `start` leg: a subscribe that never delivered.
    const view = viewOf([[40, 1_000]]);
    expect(view.frames[0].badges).toContain("orphaned");
    const html = renderOperationRow(view, { now: 6_000 });
    expect(html).toContain("waiting 5.00s");
    // It is a subscription with no terminator, so it is live AND waiting: the row
    // carries both classes and the stylesheet's precedence rule decides the
    // colour. The meta text reports the wait, not the span.
    expect(html).toContain("td-op-live");
    expect(html).toContain("td-op-waiting");
  });
});

describe("renderOperationRow — `waiting` needs an unanswered OPENER", () => {
  // `orphaned` is now opener-only by construction: every closer with no opener
  // earns `unpaired`. These cases used to earn `orphaned` too, and reading that as
  // "unanswered" pre-empted the honest duration with a nonsense wait. The badge
  // split removes the ambiguity; these tests pin that the render still never
  // reports a wait for any of them.

  test("a receive that raced past the stop keeps the op's real duration", () => {
    const view = viewOf([
      [40, 1_000], // start
      [41, 1_100], // receive
      [42, 1_200], // stop
      [41, 1_205], // a receive already in flight lands after the stop
    ]);
    // The late receive is a closer with no opener left on the stack: `unpaired`,
    // not an unanswered request.
    expect(view.badges).toContain("unpaired");
    expect(view.badges).not.toContain("orphaned");
    expect(view.durationMs).toBe(205);
    const html = renderOperationRow(view, { now: 1_000 + 3_600_000 });
    expect(html).toContain("205ms");
    expect(html).not.toContain("waiting");
    expect(html).not.toContain("td-op-waiting");
  });

  test("a subscription observed receive-only reports live, not a wait", () => {
    // The debugger attached mid-session, so the `start` was never observed and no
    // receive has an opener. The sub is delivering a frame a second - `unpaired`
    // states that plainly instead of implying the host never answered.
    const view = viewOf([
      [41, 1_000],
      [41, 2_000],
      [41, 3_000],
    ]);
    expect(view.badges).toContain("unpaired");
    expect(view.badges).not.toContain("orphaned");
    const html = renderOperationRow(view, { now: 301_000 });
    expect(html).not.toContain("waiting");
    expect(html).toContain("live");
  });

  test("an off-table opener leaves a completed round trip reading as one", () => {
    // Frame id 999 is not on this debugger's table, so the opener resolves to role
    // "unknown", is not recognised as an opener, and its response has none — but
    // the call did complete, so `orphaned` would be a lie about the host.
    const view = viewOf([
      [999, 1_000],
      [23, 1_120],
    ]);
    expect(view.badges).toContain("unpaired");
    expect(view.badges).not.toContain("orphaned");
    const html = renderOperationRow(view, { now: 1_000 + 3_600_000 });
    expect(html).toContain("120ms");
    expect(html).not.toContain("waiting");
  });
});

describe("renderOperationRow — liveness", () => {
  test("a subscription the host interrupted is not live", () => {
    // `interrupt` is the host's terminator. Testing only for `stop` leaves every
    // host-ended subscription reading live for the rest of the session.
    const view = viewOf([
      [40, 1_000],
      [41, 1_100],
      [43, 1_200], // interrupt
    ]);
    const html = renderOperationRow(view);
    expect(html).toContain("td-op-sub");
    expect(html).not.toContain("td-op-live");
    expect(html).not.toContain("live");
  });

  test("a subscription with no terminator is still live", () => {
    const html = renderOperationRow(
      viewOf([
        [40, 1_000],
        [41, 1_100],
      ]),
    );
    expect(html).toContain("td-op-live");
  });
});

describe("truncation is reported per axis, not as one boolean", () => {
  test("the badge carries the count and names the cap that took the frames", () => {
    const view = viewOf([[40, 1_000]], {
      framesByCount: 77,
      framesByBytes: 0,
      payloadsShed: 0,
    });
    const html = renderOperationRow(view);
    expect(html).toContain("td-badge-truncated");
    expect(html).toContain("truncated 77");
    expect(html).toContain("77 frames dropped (frame cap)");
  });

  test("one frame lost does not render like seventy-seven", () => {
    const one = renderOperationRow(
      viewOf([[40, 1_000]], {
        framesByCount: 1,
        framesByBytes: 0,
        payloadsShed: 0,
      }),
    );
    const many = renderOperationRow(
      viewOf([[40, 1_000]], {
        framesByCount: 77,
        framesByBytes: 0,
        payloadsShed: 0,
      }),
    );
    expect(one).toContain("truncated 1");
    expect(many).toContain("truncated 77");
    expect(one).not.toBe(many);
  });

  test("the byte axis is distinguishable from the frame axis", () => {
    const html = renderTraceDetail(
      viewOf([[40, 1_000]], {
        framesByCount: 0,
        framesByBytes: 4,
        payloadsShed: 2,
      }),
    );
    expect(html).toContain("4 frames dropped (byte cap)");
    expect(html).toContain("2 payloads shed");
    expect(html).not.toContain("frame cap");
  });
});

describe("duration formatting", () => {
  test("a long wait reads in hours, not thousands of seconds", () => {
    const view: TraceView = {
      requestId: "p:9",
      startedAt: 0,
      lastAt: 0,
      durationMs: 0,
      frames: [
        {
          seq: 0,
          direction: "out",
          role: "request",
          method: "account.getAccount",
          frameId: 22,
          byteLength: 8,
          timestamp: 0,
          latencyFromStartMs: 0,
          badges: ["orphaned"],
          decodable: false,
        },
      ],
      badges: ["orphaned"],
    };
    expect(renderOperationRow(view, { now: 10_800_000 })).toContain(
      "waiting 3h 00m",
    );
    expect(renderOperationRow(view, { now: 10_800_000 })).not.toContain(
      "10800.00s",
    );
    expect(renderOperationRow(view, { now: 205_000 })).toContain(
      "waiting 3m 25s",
    );
    // Under a minute still reads in seconds.
    expect(renderOperationRow(view, { now: 45_000 })).toContain(
      "waiting 45.00s",
    );
  });

  test("a multi-minute op's span reads in minutes", () => {
    const html = renderOperationRow(
      viewOf([
        [40, 0],
        [41, 205_000],
      ]),
    );
    expect(html).toContain("3m 25s");
  });
});

describe("method labels survive left-truncation", () => {
  test("the method is emitted inside an explicit LTR isolate", () => {
    // `.td-op-method` uses `direction: rtl` to put the ellipsis on the left, which
    // reorders any label that is not a pure LTR identifier (`account.getAccount:`
    // → `:account.getAccount`). The isolate keeps it one left-to-right run.
    const html = renderOperationRow(
      viewOf([
        [22, 1_000],
        [23, 1_100],
      ]),
    );
    expect(html).toContain('<bdi dir="ltr">account.getAccount</bdi>');
  });
});
