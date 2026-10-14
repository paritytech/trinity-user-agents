import { describe, expect, test } from "bun:test";

import {
  coreWireSchemaHash,
  createDebuggerLink,
  type DebuggerSocket,
} from "./worker-runtime.js";
import { isLoopbackWsUrl } from "./worker-protocol.js";

/**
 * The gate mirrors the native sink's (`native_debug.rs`) three cases — loopback
 * forms, bracket forms, and non-loopback rejection — and then covers the shapes
 * a string-matching check is normally bypassed by.
 *
 * The one deliberate difference from the Rust side: `WsDebugSink::connect`
 * *resolves* the host and requires every resolved address to be loopback, which
 * closes the "validate one string, dial another" gap. A Web Worker has no
 * resolver, so this checks the hostname the WHATWG parser normalized. That is
 * sound here for the reason the Rust gap needed closing at all: the same `url`
 * string is handed to `new WebSocket(url)`, so the browser resolves exactly what
 * was validated — there is no second, unvalidated string.
 */
describe("isLoopbackWsUrl", () => {
  test("accepts ws:// on every genuine loopback form", () => {
    for (const url of [
      "ws://localhost:9231",
      "ws://127.0.0.1:9231",
      // 127.0.0.0/8 in full, not just 127.0.0.1.
      "ws://127.5.6.7:9231",
      "ws://[::1]:9231",
      // A bracket form the URL parser normalizes to a different string than the
      // one written.
      "ws://[0:0:0:0:0:0:0:1]:9231",
      // Scheme and host are case-insensitive; a path or query is irrelevant.
      "WS://127.0.0.1:9231",
      "ws://LOCALHOST:9231",
      "ws://127.0.0.1:9231/path?q=1",
    ]) {
      expect(isLoopbackWsUrl(url)).toBe(true);
    }
  });

  test("rejects wss:// — the tap is ws-only, matching the native sink", () => {
    expect(isLoopbackWsUrl("wss://localhost:9231")).toBe(false);
    expect(isLoopbackWsUrl("wss://127.0.0.1:9231")).toBe(false);
    expect(isLoopbackWsUrl("wss://[::1]:9231")).toBe(false);
  });

  test("rejects every non-ws scheme, including ones that parse", () => {
    for (const url of [
      "http://127.0.0.1:9231",
      "https://127.0.0.1:9231",
      "file://127.0.0.1",
      "javascript:alert(1)",
      "not a url",
      "",
    ]) {
      expect(isLoopbackWsUrl(url)).toBe(false);
    }
  });

  test("rejects non-loopback hosts, including the ones that look local", () => {
    for (const url of [
      "ws://192.0.2.1:9231",
      "ws://example.com:9231",
      // A wildcard bind is not loopback: it is reachable from off-machine.
      "ws://0.0.0.0:9231",
      // Private and link-local ranges are still off-machine.
      "ws://10.0.0.1:9231",
      "ws://169.254.169.254:9231",
      // IPv4-mapped *non*-loopback must not ride the ::ffff: prefix in.
      "ws://[::ffff:192.0.2.1]:9231",
      // A trailing dot is a distinct hostname and is not accepted.
      "ws://localhost.:9231",
      // IPv4-mapped loopback, in both the written and the normalized spelling.
      // The native sink refuses it (`Ipv6Addr::is_loopback` matches only `::1`),
      // so accepting it here would make a dial that works in the web host fail
      // in the CLI - a difference a developer would debug as a broken debugger.
      "ws://[::ffff:127.0.0.1]:9231",
      "ws://[::ffff:7f00:1]:9231",
    ]) {
      expect(isLoopbackWsUrl(url)).toBe(false);
    }
  });

  test("rejects hosts that merely embed a loopback-looking substring", () => {
    for (const url of [
      "ws://localhost.evil.com:9231",
      "ws://127.0.0.1.evil.com:9231",
      // Path and userinfo are not the host: the dial target is `evil.com`.
      "ws://evil.com/127.0.0.1",
      "ws://user:pass@evil.com:9231",
      "ws://127.0.0.1@evil.com:9231",
      // A homoglyph digit does not normalize to an ASCII loopback literal.
      "ws://➀27.0.0.1:9231",
    ]) {
      expect(isLoopbackWsUrl(url)).toBe(false);
    }
  });

  test("accepts alternate integer spellings of 127.0.0.1 — they are loopback", () => {
    // The WHATWG parser normalizes these to `127.0.0.1` before the check, and
    // `new WebSocket(url)` normalizes identically, so accepting them is correct:
    // the socket really does go to loopback. Pinned so a future hand-rolled
    // hostname check cannot quietly start disagreeing with the parser.
    expect(isLoopbackWsUrl("ws://2130706433:9231")).toBe(true);
    expect(isLoopbackWsUrl("ws://0177.0.0.1:9231")).toBe(true);
    expect(isLoopbackWsUrl("ws://0x7f.0.0.1:9231")).toBe(true);
    // The same spellings for a non-loopback address stay rejected.
    expect(isLoopbackWsUrl("ws://3221225985:9231")).toBe(false);
  });

  test("userinfo cannot smuggle a non-loopback dial target", () => {
    // Mirror of the rejection case: here the *host* is loopback and the
    // userinfo is the decoy, so the dial is loopback and the URL is accepted.
    expect(isLoopbackWsUrl("ws://evil.com@127.0.0.1:9231")).toBe(true);
  });
});

/**
 * A socket the tests drive: records what was sent, lets a test stall the peer by
 * holding `bufferedAmount` high, and can fail a send the way a dead socket does.
 */
class FakeSocket implements DebuggerSocket {
  sent: string[] = [];
  bufferedAmount = 0;
  failSends = false;
  closed = false;
  private listeners = new Map<string, (() => void)[]>();

  send(data: string): void {
    if (this.failSends) throw new Error("socket is dead");
    this.sent.push(data);
  }
  close(): void {
    this.closed = true;
  }
  addEventListener(
    type: "open" | "close" | "error",
    listener: () => void,
  ): void {
    const list = this.listeners.get(type) ?? [];
    list.push(listener);
    this.listeners.set(type, list);
  }
  /** Drive the lifecycle the real socket would. */
  fire(type: "open" | "close" | "error"): void {
    for (const l of this.listeners.get(type) ?? []) l();
  }
  /** Every envelope sent so far, parsed. */
  envelopes(): Record<string, unknown>[] {
    return this.sent.map((s) => JSON.parse(s) as Record<string, unknown>);
  }
}

/** A link wired to a fake socket and a manual clock. */
function harness(options: { schema?: string } = {}) {
  const sockets: FakeSocket[] = [];
  const timers: { run: () => void; delayMs: number }[] = [];
  const link = createDebuggerLink("ws://127.0.0.1:9231", {
    ...options,
    createSocket: () => {
      const s = new FakeSocket();
      sockets.push(s);
      return s;
    },
    schedule: (run, delayMs) => timers.push({ run, delayMs }),
  });
  return {
    link,
    sockets,
    timers,
    /** The socket currently in use. */
    live: () => sockets[sockets.length - 1]!,
    /** Run every pending timer once, as the scheduler would. */
    tick: () => {
      const due = timers.splice(0);
      for (const t of due) t.run();
    },
  };
}

const FRAME = new Uint8Array([1, 2, 3]);

describe("debugger link: envelope contents", () => {
  test("stamps the core's schema only when the core vouched for one", () => {
    const attested = harness({ schema: "deadbeefdeadbeef" });
    attested.live().fire("open");
    attested.link.emit("app.dot", "out", FRAME);
    expect(attested.live().envelopes()[0]?.schema).toBe("deadbeefdeadbeef");

    // No schema from the core: the envelope must carry none, so the debugger
    // groups but refuses to decode rather than trusting a hash nobody vouched
    // for. Fabricating one here is the silent mis-decode this exists to prevent.
    const bare = harness();
    bare.live().fire("open");
    bare.link.emit("app.dot", "out", FRAME);
    expect(bare.live().envelopes()[0]).not.toHaveProperty("schema");
  });

  test("every frame carries the producer's own observation time", () => {
    const h = harness();
    h.live().fire("open");
    const before = Date.now();
    h.link.emit("app.dot", "out", FRAME);
    const observedAt = h.live().envelopes()[0]?.observedAt;
    expect(typeof observedAt).toBe("number");
    expect(observedAt as number).toBeGreaterThanOrEqual(before);
  });

  test("frames queued while the socket is down replay marked as buffered", () => {
    const h = harness();
    // Socket not open yet: these go to the queue.
    h.link.emit("app.dot", "out", FRAME);
    h.link.emit("app.dot", "in", FRAME);
    expect(h.live().sent).toHaveLength(0);

    h.live().fire("open");
    const flushed = h.live().envelopes();
    expect(flushed).toHaveLength(2);
    // Without the marker the debugger cannot tell a replayed backlog from a live
    // stream, and every op in the flush lands in one retry-storm window.
    expect(flushed.every((e) => e.buffered === true)).toBe(true);
  });

  test("a live frame is not marked buffered", () => {
    const h = harness();
    h.live().fire("open");
    h.link.emit("app.dot", "out", FRAME);
    expect(h.live().envelopes()[0]).not.toHaveProperty("buffered");
  });
});

describe("debugger link: backpressure and drop accounting", () => {
  test("sheds when the socket's own buffer is over the ceiling", () => {
    const h = harness();
    h.live().fire("open");
    // Peer stopped reading: readyState stays OPEN while bufferedAmount grows, so
    // handing frames over unchecked is unbounded buffering in the observed
    // session's worker.
    h.live().bufferedAmount = 9 * 1024 * 1024;
    h.link.emit("app.dot", "out", FRAME);
    expect(h.live().sent).toHaveLength(0);

    // The shed is counted and reported on the next frame that gets through.
    h.live().bufferedAmount = 0;
    h.link.emit("app.dot", "out", FRAME);
    expect(h.live().envelopes()[0]?.dropped).toBe(1);
  });

  test("sheds a single over-cap message instead of killing the stream", () => {
    const h = harness();
    h.live().fire("open");
    // One oversized frame on an IDLE socket: the cumulative ceiling never trips,
    // but the debugger closes the connection on an over-cap message, so an
    // unshed frame costs every later frame too.
    h.link.emit("app.dot", "out", new Uint8Array(7 * 1024 * 1024));
    expect(h.live().sent).toHaveLength(0);

    h.link.emit("app.dot", "out", FRAME);
    const envelopes = h.live().envelopes();
    expect(envelopes).toHaveLength(1);
    expect(envelopes[0]?.dropped).toBe(1);
  });

  test("a failed send keeps the drop count instead of clearing it", () => {
    const h = harness();
    h.live().fire("open");
    h.live().bufferedAmount = 9 * 1024 * 1024;
    h.link.emit("app.dot", "out", FRAME); // shed, dropped = 1
    h.live().bufferedAmount = 0;

    h.live().failSends = true;
    h.link.emit("app.dot", "out", FRAME); // send throws
    h.live().failSends = false;

    h.link.emit("app.dot", "out", FRAME);
    // EXACT, not >=1: the shed frame AND the frame whose send failed are both
    // losses. `>=1` was satisfied by the seeded shed alone, so it could not see
    // the frame that vanished on the failed send.
    expect(h.live().envelopes()[0]?.dropped).toBe(2);
  });

  test("a failed backlog drain reports the gap instead of swallowing it", () => {
    const h = harness();
    // Queue a backlog and shed past the cap, then fail every send on drain. The
    // count was cleared before the loop, so a fully-failed flush reported a
    // clean session while every frame in it was lost.
    for (let i = 0; i < 1200; i++) h.link.emit("app.dot", "out", FRAME);
    h.live().failSends = true;
    h.live().fire("open");
    h.live().failSends = false;

    h.link.emit("app.dot", "out", FRAME);
    const reported = h.live().envelopes().at(-1)?.dropped;
    expect(typeof reported).toBe("number");
    expect(reported as number).toBeGreaterThan(1000);
  });

  test("the backlog drain respects the per-message cap", () => {
    const h = harness();
    // Queued while the socket is down, drained after it opens. The cap is
    // documented as applying to both paths; only the live one enforced it, so an
    // over-cap message reached the debugger on reconnect and closed the stream.
    h.link.emit("app.dot", "out", new Uint8Array(5 * 1024 * 1024));
    h.live().fire("open");
    for (const raw of h.live().sent) {
      expect(raw.length).toBeLessThanOrEqual(6 * 1024 * 1024);
    }
  });

  test("the backlog drain does not force-feed a wedged socket", () => {
    const h = harness();
    for (let i = 0; i < 50; i++) h.link.emit("app.dot", "out", FRAME);
    // Peer stopped reading before the drain begins.
    h.live().bufferedAmount = 9 * 1024 * 1024;
    h.live().fire("open");
    expect(h.live().sent).toHaveLength(0);
  });
});

describe("debugger link: reconnect", () => {
  test("a dead link redials on a timer, not once per frame", () => {
    const h = harness();
    h.live().fire("close");
    const dialsBefore = h.sockets.length;

    // Reconnect is scheduled lazily by the next emit. The property that matters
    // is that N further frames do NOT produce N dials: before the backoff, a busy
    // session with no debugger listening dialed loopback hundreds of times a
    // second, each refused, each logging a console error.
    for (let i = 0; i < 10; i++) h.link.emit("app.dot", "out", FRAME);
    expect(h.sockets.length).toBe(dialsBefore);
    expect(h.timers.length).toBe(1);
    expect(h.timers[0]?.delayMs ?? 0).toBeGreaterThan(0);

    h.tick();
    expect(h.sockets.length).toBe(dialsBefore + 1);
  });

  test("the backoff grows across repeated failed dials", () => {
    const h = harness();
    const delays: number[] = [];
    for (let i = 0; i < 3; i++) {
      h.live().fire("close");
      h.link.emit("app.dot", "out", FRAME);
      delays.push(h.timers[0]?.delayMs ?? 0);
      h.tick();
    }
    expect(delays[0]).toBeGreaterThan(0);
    expect(delays[1]).toBeGreaterThan(delays[0]!);
  });

  test("a dial that reaches the debugger earns the short delay back", () => {
    const h = harness();
    // Fail twice so the backoff has grown.
    for (let i = 0; i < 2; i++) {
      h.live().fire("close");
      h.link.emit("app.dot", "out", FRAME);
      h.tick();
    }
    // Now a dial succeeds, then dies again: the next wait is the base delay, so a
    // debugger that restarts is picked up promptly rather than after the cap.
    h.live().fire("open");
    h.live().fire("close");
    h.link.emit("app.dot", "out", FRAME);
    expect(h.timers[0]?.delayMs).toBe(200);
  });
});

describe("coreWireSchemaHash: attest only what the core vouched for", () => {
  test("returns the core's hash when it reports one", () => {
    expect(
      coreWireSchemaHash({ wireSchemaHash: () => "abc123abc123abc1" }),
    ).toBe("abc123abc123abc1");
  });

  test("returns undefined for a core that does not report one", () => {
    // `dist/wasm/web/` is gitignored and hand-built, so a stale bundle predating
    // the export is a normal state to find at runtime. Inventing a hash here
    // would attest to a table this core did not encode with — the debugger would
    // then decode a foreign contract confidently, which is precisely the silent
    // mis-decode the fingerprint exists to stop. Grouping without decode is the
    // correct degradation.
    expect(coreWireSchemaHash({})).toBeUndefined();
  });

  test("returns undefined when the core's accessor throws or lies", () => {
    expect(
      coreWireSchemaHash({
        wireSchemaHash: () => {
          throw new Error("stale bundle");
        },
      }),
    ).toBeUndefined();
    // A non-string or empty answer is not an attestation either.
    expect(
      coreWireSchemaHash({ wireSchemaHash: () => "" as unknown as string }),
    ).toBeUndefined();
  });
});
