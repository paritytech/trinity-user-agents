import { expect, test } from "bun:test";

import {
  encodeWireMessage,
  MESSAGE_TYPE_REQUEST,
  VersionedHostSignRawRequest,
  TRUAPI_CODEC_VERSION,
  TRUAPI_WIRE_SCHEMA_HASH,
} from "@parity/truapi";
import * as REAL_W from "@parity/truapi/wire-table";

import { WIRE_ENVELOPE_VERSION } from "./ingest.js";
import {
  decodeValuesFromEnv,
  hostHeaderAllowed,
  isLoopbackDebugHost,
  portFromEnv,
  startDebugServer,
} from "./server.js";
import { frameIdOf } from "./wire-debugger.js";

interface TraceFrameView {
  direction: string;
  frameId: number;
  method?: string;
  byteLength: number;
}
interface TraceView {
  requestId: string;
  frames: TraceFrameView[];
}

/**
 * Test-only convenience view over the generated wire table: gives each entry
 * per-leg "flat id" properties by encoding `(frameId, messageType)` as one
 * number (`frameId * 4 + messageType`), so fixtures can address a method's
 * specific leg by name. `encodeFrame()` below decodes a leg id back into a
 * real `(trait, method)` pair and a `messageType` byte.
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

/** base64 of a wire message for leg id `legId` carrying `value` as its own payload bytes. */
function encodeFrame(requestId: string, legId: number, value: Uint8Array): string {
  const frameId = Math.floor(legId / 4);
  const messageType = legId % 4;
  const encoded = encodeWireMessage({
    requestId,
    payload: {
      traitId: Math.floor(frameId / 256),
      methodId: frameId % 256,
      messageType,
      value,
    },
  });
  if (encoded.isErr()) throw encoded.error;
  return Buffer.from(encoded.value).toString("base64");
}

/**
 * base64 of a real, decodable sign-raw request wire message. Carries a
 * recognizable `dotNsIdentifier` ("alice.dot") in its decoded value so a test
 * can prove the value surfaced — this debugger decodes it like any other frame.
 */
function signFrame(requestId: string): string {
  const value = VersionedHostSignRawRequest.enc({
    tag: "V1",
    value: {
      account: {
        dotNsIdentifier: "alice.dot",
        derivationIndex: { tag: "Index", value: 0 },
      },
      payload: { tag: "Bytes", value: { bytes: "0xdeadbeef" } },
    },
  });
  const encoded = encodeWireMessage({
    requestId,
    payload: {
      traitId: REAL_W.SIGNING_SIGN_RAW.trait,
      methodId: REAL_W.SIGNING_SIGN_RAW.method,
      messageType: MESSAGE_TYPE_REQUEST,
      value,
    },
  });
  if (encoded.isErr()) throw encoded.error;
  return Buffer.from(encoded.value).toString("base64");
}

/** Open a WS to the server, send one envelope, wait until `/traces` is non-empty. */
async function streamFrame(
  base: string,
  port: number,
  frame: string,
  dir: "in" | "out" = "out",
): Promise<TraceView[]> {
  const ws = new WebSocket(`ws://localhost:${port}`);
  await new Promise<void>((resolve, reject) => {
    ws.onopen = () => resolve();
    ws.onerror = () => reject(new Error("ws failed to open"));
  });
  ws.send(
    JSON.stringify({
      channelId: "myapp.dot",
      dir,
      frame,
      schema: TRUAPI_WIRE_SCHEMA_HASH,
    }),
  );
  let traces: TraceView[] = [];
  for (let i = 0; i < 50 && traces.length === 0; i++) {
    traces = (await (await fetch(`${base}/traces`)).json()) as TraceView[];
    if (traces.length === 0) await new Promise((r) => setTimeout(r, 20));
  }
  ws.close();
  return traces;
}

test("decodes and groups a frame a host streams over the WS", async () => {
  const server = startDebugServer({ port: 0 });
  const base = `http://localhost:${server.port}`;
  try {
    const encoded = encodeWireMessage({
      requestId: "p:1",
      payload: {
        traitId: REAL_W.SYSTEM_HANDSHAKE.trait,
        methodId: REAL_W.SYSTEM_HANDSHAKE.method,
        messageType: MESSAGE_TYPE_REQUEST,
        value: new Uint8Array([0, 1, 2, 3]),
      },
    });
    if (encoded.isErr()) throw encoded.error;
    const frame = Buffer.from(encoded.value).toString("base64");

    const ws = new WebSocket(`ws://localhost:${server.port}`);
    await new Promise<void>((resolve, reject) => {
      ws.onopen = () => resolve();
      ws.onerror = () => reject(new Error("ws failed to open"));
    });
    ws.send(
      JSON.stringify({
        channelId: "myapp.dot",
        dir: "out",
        frame,
        schema: TRUAPI_WIRE_SCHEMA_HASH,
      }),
    );

    let traces: TraceView[] = [];
    for (let i = 0; i < 50 && traces.length === 0; i++) {
      traces = (await (await fetch(`${base}/traces`)).json()) as TraceView[];
      if (traces.length === 0) await new Promise((r) => setTimeout(r, 20));
    }
    ws.close();

    expect(traces).toHaveLength(1);
    expect(traces[0].requestId).toBe("p:1");
    expect(traces[0].frames[0].direction).toBe("out");
    expect(traces[0].frames[0].frameId).toBe(
      frameIdOf(REAL_W.SYSTEM_HANDSHAKE.trait, REAL_W.SYSTEM_HANDSHAKE.method),
    );
    // The method map resolves the wire id to a dotted name for the view.
    expect(typeof traces[0].frames[0].method).toBe("string");
  } finally {
    server.stop();
  }
});

test("the inspector page is served at /", async () => {
  const server = startDebugServer({ port: 0 });
  try {
    const res = await fetch(`http://localhost:${server.port}/`);
    expect(res.status).toBe(200);
    expect(res.headers.get("content-type")).toContain("text/html");
    const html = await res.text();
    expect(html).toContain("TrUAPI Wire Inspector");
    // The shell fetches the shared fragments, not a bespoke renderer.
    expect(html).toContain("/op-list");
    expect(html).toContain("/op?id=");
  } finally {
    server.stop();
  }
});

test("/op-list renders one shared row per op, payload-blind", async () => {
  const server = startDebugServer({ port: 0 });
  const base = `http://localhost:${server.port}`;
  try {
    const frame = encodeFrame(
      "p:1",
      W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.start,
      new Uint8Array([0]),
    );
    await streamFrame(base, server.port, frame);
    const html = await (await fetch(`${base}/op-list`)).text();
    expect(html).toContain("td-op");
    expect(html).toContain('data-request-id="p:1"');
    // Subscription start, no stop yet: marked live. And never a value.
    expect(html).toContain("td-op-sub");
    expect(html).not.toContain("V1");
  } finally {
    server.stop();
  }
});

test("/op renders the drill-down for one op; unknown id degrades", async () => {
  const server = startDebugServer({ port: 0 });
  const base = `http://localhost:${server.port}`;
  try {
    const frame = encodeFrame("p:1", W.SYSTEM_HANDSHAKE.request, new Uint8Array([1]));
    await streamFrame(base, server.port, frame);
    const ok = await (await fetch(`${base}/op?id=p:1`)).text();
    expect(ok).toContain("td-trace");
    expect(ok).toContain('data-request-id="p:1"');
    const missing = await (await fetch(`${base}/op?id=nope`)).text();
    expect(missing).toContain("not found");
  } finally {
    server.stop();
  }
});

test("/channels reports the hosts that have dialed in", async () => {
  const server = startDebugServer({ port: 0 });
  const base = `http://localhost:${server.port}`;
  try {
    const frame = encodeFrame("p:1", W.SYSTEM_HANDSHAKE.request, new Uint8Array([1]));
    await streamFrame(base, server.port, frame);
    const data = (await (await fetch(`${base}/channels`)).json()) as {
      sockets: number;
      channels: {
        channelId: string;
        firstSeen: number;
        lastSeen: number;
        frameCount: number;
        connected: boolean;
      }[];
    };
    const ch = data.channels.find((c) => c.channelId === "myapp.dot");
    expect(ch).toBeDefined();
    expect(ch?.frameCount).toBeGreaterThanOrEqual(1);
    expect(ch?.connected).toBe(true);
    expect(ch?.firstSeen).toBeLessThanOrEqual(ch?.lastSeen ?? 0);
  } finally {
    server.stop();
  }
});

test("/traces is byte- and value-free even with value decode on", async () => {
  const server = startDebugServer({ port: 0, decodeValues: true });
  const base = `http://localhost:${server.port}`;
  try {
    // A decodable, non-sensitive frame: `connection-status.subscribe` start is
    // `V1(void)` = a single 0x00 byte, which the generated table decodes to a
    // `{ tag: "V1" }` value - a value that must never appear in `/traces`.
    const frame = encodeFrame(
      "p:1",
      W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.start,
      new Uint8Array([0]),
    );
    const traces = await streamFrame(base, server.port, frame);
    expect(traces).toHaveLength(1);

    const raw = await (await fetch(`${base}/traces`)).text();
    // No payload-bearing keys and no decoded content leak into the trace list.
    for (const banned of ['"bytes"', '"value"', '"decoded"', '"tag"', "V1"]) {
      expect(raw).not.toContain(banned);
    }
  } finally {
    server.stop();
  }
});

test("/stats is byte- and value-free even with value decode on", async () => {
  const server = startDebugServer({ port: 0, decodeValues: true });
  const base = `http://localhost:${server.port}`;
  try {
    // The same decodable, non-sensitive frame as the /traces test: it has no
    // request wrapper, so its decoded value is bare `undefined`. The aggregate
    // must report only counts - its `bytes` field is a summed byte *length*,
    // never a raw or decoded payload.
    const frame = encodeFrame(
      "p:1",
      W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.start,
      new Uint8Array([0]),
    );
    await streamFrame(base, server.port, frame);

    const raw = await (await fetch(`${base}/stats`)).text();
    // No decoded content and no raw-payload hex leaks into the aggregate.
    for (const banned of ['"value"', '"decoded"', '"tag"', "V1", "0x"]) {
      expect(raw).not.toContain(banned);
    }
    // The aggregate is present, and `bytes` is a summed length (here 1B: the
    // 1 inner byte), a count.
    const stats = JSON.parse(raw) as {
      ops: number;
      frames: number;
      bytes: number;
    };
    expect(stats.ops).toBe(1);
    expect(stats.frames).toBe(1);
    expect(stats.bytes).toBe(1);
  } finally {
    server.stop();
  }
});

test("/frame decodes a non-sensitive frame by default; decodeValues:false reports bytes", async () => {
  const frame = encodeFrame(
    "p:1",
    W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.start,
    new Uint8Array([0]),
  );

  // Default (dev-only tool): decode is on, so the drill-down surfaces the value.
  const on = startDebugServer({ port: 0 });
  try {
    expect(on.decodeValues).toBe(true);
    const baseOn = `http://localhost:${on.port}`;
    await streamFrame(baseOn, on.port, frame);
    const detail = await (await fetch(`${baseOn}/frame?id=p:1&i=0`)).json();
    expect(detail.kind).toBe("decoded");
    // The request wrapper carries no payload, so the tag is all there is to report.
    expect(detail.value).toEqual({ tag: "V1" });
  } finally {
    on.stop();
  }

  // `decodeValues: false` (still supported, for demos/tests): byte length only.
  const off = startDebugServer({ port: 0, decodeValues: false });
  try {
    expect(off.decodeValues).toBe(false);
    const baseOff = `http://localhost:${off.port}`;
    await streamFrame(baseOff, off.port, frame);
    const detail = await (await fetch(`${baseOff}/frame?id=p:1&i=0`)).json();
    expect(detail.kind).toBe("bytes");
    // The 1 inner byte.
    expect(detail.byteLength).toBe(1);
  } finally {
    off.stop();
  }
});

test("a signing frame decodes like any other; /traces never carries its bytes", async () => {
  const server = startDebugServer({ port: 0, decodeValues: true });
  const base = `http://localhost:${server.port}`;
  try {
    await streamFrame(base, server.port, signFrame("p:sign"));

    // Dev-only tool: no denylist, so the frame decodes and its value surfaces.
    const detail = await (await fetch(`${base}/frame?id=p:sign&i=0`)).json();
    expect(detail.kind).toBe("decoded");
    expect(JSON.stringify(detail.value)).toContain("alice.dot");
    // The decoded result never carries a "sensitive"/"redacted" marker any more.
    expect(detail.sensitive).toBeUndefined();

    // The payload-blind grouping invariant still holds: /traces never serializes
    // the raw or decoded bytes, only the /frame drill-down does.
    const raw = await (await fetch(`${base}/traces`)).text();
    expect(raw).not.toContain("deadbeef");
    expect(raw).not.toContain("alice.dot");
  } finally {
    server.stop();
  }
});

test("/view renders the shared drill-down with decoded values by default", async () => {
  // Default (dev-only tool): decode is on, so the drill-down renders each
  // frame's value inline — no click-to-decode control.
  const server = startDebugServer({ port: 0 });
  try {
    const base = `http://localhost:${server.port}`;
    const frame = encodeFrame(
      "p:1",
      W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.start,
      new Uint8Array([0]),
    );
    await streamFrame(base, server.port, frame);
    const html = await (await fetch(`${base}/view`)).text();
    // Shared-renderer markup, not the old table.
    expect(html).toContain("td-trace");
    expect(html).toContain("td-frame");
    expect(html).toContain('data-request-id="p:1"');
    // Values render inline; the click-to-decode control is gone.
    expect(html).toContain("td-frame-payload");
    expect(html).not.toContain("td-frame-decode-btn");
    expect(html).not.toContain("decode payload");
  } finally {
    server.stop();
  }
});

test("/view is payload-blind when decode is off", async () => {
  const off = startDebugServer({ port: 0, decodeValues: false });
  try {
    const base = `http://localhost:${off.port}`;
    const frame = encodeFrame(
      "p:1",
      W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.start,
      new Uint8Array([0]),
    );
    await streamFrame(base, off.port, frame);
    const html = await (await fetch(`${base}/view`)).text();
    expect(html).toContain('data-request-id="p:1"');
    // No payload column at all, and no decode control.
    expect(html).not.toContain("td-frame-payload");
    expect(html).not.toContain("td-frame-decode-btn");
  } finally {
    off.stop();
  }
});

test("/op decodes every frame inline via the real decodeTraceFrames path", async () => {
  const server = startDebugServer({ port: 0 });
  const base = `http://localhost:${server.port}`;
  try {
    // A real sign-raw request whose decoded value carries "alice.dot".
    await streamFrame(base, server.port, signFrame("p:sign"));

    // The op drill-down renders the decoded value inline — proving the
    // session → decodeTraceFrames → renderer wiring, not just structural markup.
    const html = await (
      await fetch(`${base}/op?id=p:sign&channel=myapp.dot&gen=0`)
    ).text();
    expect(html).toContain("td-frame-decoded");
    expect(html).toContain("alice.dot");
    // Inline, not behind a control, and nothing withheld.
    expect(html).not.toContain("td-frame-decode-btn");
    expect(html).not.toContain("redacted");
  } finally {
    server.stop();
  }
});

test("/op refuses to decode a codec-mismatched (untrusted) channel", async () => {
  const server = startDebugServer({ port: 0 });
  const base = `http://localhost:${server.port}`;
  try {
    // Stream a frame with a wrong wire schema hash: the channel is untrusted.
    const ws = new WebSocket(`ws://localhost:${server.port}`);
    await new Promise<void>((resolve, reject) => {
      ws.onopen = () => resolve();
      ws.onerror = () => reject(new Error("ws failed"));
    });
    ws.send(
      JSON.stringify({
        channelId: "drift.dot",
        dir: "out",
        frame: signFrame("p:sign"),
        schema: "0000000000000000",
      }),
    );
    for (let i = 0; i < 50; i++) {
      const t = (await (await fetch(`${base}/traces`)).json()) as TraceView[];
      if (t.length > 0) break;
      await new Promise((r) => setTimeout(r, 20));
    }
    ws.close();

    const html = await (
      await fetch(`${base}/op?id=p:sign&channel=drift.dot&gen=0`)
    ).text();
    // Grouped and shown, but no decoded value for the untrusted channel.
    expect(html).toContain('data-request-id="p:sign"');
    expect(html).not.toContain("alice.dot");
    expect(html).toContain("payload not shown");
  } finally {
    server.stop();
  }
});

test("/frame validates its params and 404s an unknown frame", async () => {
  const server = startDebugServer({ port: 0, decodeValues: true });
  const base = `http://localhost:${server.port}`;
  try {
    expect((await fetch(`${base}/frame`)).status).toBe(400);
    expect((await fetch(`${base}/frame?id=x&i=notint`)).status).toBe(400);
    // Empty `?i=` must 400, not resolve frame 0 (Number("") === 0).
    expect((await fetch(`${base}/frame?id=x&i=`)).status).toBe(400);
    expect((await fetch(`${base}/frame?id=x&i=%20`)).status).toBe(400);
    // Same coercion on `?gen=`: empty/whitespace/non-int must 400, not resolve
    // generation 0 (the oldest recycled op) with a 200.
    expect((await fetch(`${base}/frame?id=x&i=0&gen=`)).status).toBe(400);
    expect((await fetch(`${base}/frame?id=x&i=0&gen=%20`)).status).toBe(400);
    expect((await fetch(`${base}/frame?id=x&i=0&gen=notint`)).status).toBe(400);
    expect((await fetch(`${base}/frame?id=missing&i=0`)).status).toBe(404);
  } finally {
    server.stop();
  }
});

test("a codec-mismatched host is banner-flagged and its frames refuse to decode", async () => {
  const server = startDebugServer({ port: 0, decodeValues: true });
  const base = `http://localhost:${server.port}`;
  try {
    const frame = encodeFrame(
      "p:1",
      W.ACCOUNT_GET_ACCOUNT.request,
      new Uint8Array([0]),
    );
    // Stream one frame declaring a codec this debugger can't decode against.
    const ws = new WebSocket(`ws://localhost:${server.port}`);
    await new Promise<void>((resolve, reject) => {
      ws.onopen = () => resolve();
      ws.onerror = () => reject(new Error("ws failed to open"));
    });
    ws.send(
      JSON.stringify({ v: 1, codec: 999, channelId: "old.dot", dir: "out", frame }),
    );
    // Wait until the frame is grouped (payload-blind grouping still happens).
    for (let i = 0; i < 50; i++) {
      const traces = (await (await fetch(`${base}/traces`)).json()) as unknown[];
      if (traces.length > 0) break;
      await new Promise((r) => setTimeout(r, 20));
    }
    ws.close();

    // /channels banners the mismatch.
    const channels = await (await fetch(`${base}/channels`)).json();
    expect(channels.codecMismatch).toBe(true);
    // Decode is refused (409) for that host's frames — never resolved against the
    // wrong contract.
    const refused = await fetch(`${base}/frame?id=p:1&i=0&channel=old.dot`);
    expect(refused.status).toBe(409);
  } finally {
    server.stop();
  }
});

test("a wrong-schema or unstamped host refuses to decode, but still groups", async () => {
  const server = startDebugServer({ port: 0, decodeValues: true });
  const base = `http://localhost:${server.port}`;
  try {
    const frame = encodeFrame(
      "p:1",
      W.ACCOUNT_GET_ACCOUNT.request,
      new Uint8Array([0]),
    );
    const stream = async (envelope: Record<string, unknown>): Promise<void> => {
      const ws = new WebSocket(`ws://localhost:${server.port}`);
      await new Promise<void>((resolve, reject) => {
        ws.onopen = () => resolve();
        ws.onerror = () => reject(new Error("ws failed to open"));
      });
      const want = ((await (await fetch(`${base}/traces`)).json()) as unknown[])
        .length;
      ws.send(JSON.stringify(envelope));
      for (let i = 0; i < 50; i++) {
        const traces = (await (await fetch(`${base}/traces`)).json()) as unknown[];
        if (traces.length > want) break;
        await new Promise((r) => setTimeout(r, 20));
      }
      ws.close();
    };
    // A frame stamping a wire schema this debugger can't decode against (the
    // codec number alone is unchanged) must be refused, never resolved against
    // the wrong contract - the case a coarse codec check misses.
    await stream({
      channelId: "stale.dot",
      dir: "out",
      frame,
      codec: 1,
      schema: "deadbeefdeadbeef",
    });
    expect(
      (await fetch(`${base}/frame?id=p:1&i=0&channel=stale.dot`)).status,
    ).toBe(409);
    // A host that stamps no identity at all is refused too: absent is not trusted.
    await stream({ channelId: "bare.dot", dir: "out", frame });
    expect(
      (await fetch(`${base}/frame?id=p:1&i=0&channel=bare.dot`)).status,
    ).toBe(409);
    // Payload-blind grouping is unaffected: both ops are recorded regardless.
    const traces = (await (await fetch(`${base}/traces`)).json()) as unknown[];
    expect(traces.length).toBe(2);
  } finally {
    server.stop();
  }
});

test("isLoopbackDebugHost accepts loopback literals and .localhost subdomains", () => {
  expect(isLoopbackDebugHost("127.0.0.1")).toBe(true);
  expect(isLoopbackDebugHost("localhost")).toBe(true);
  expect(isLoopbackDebugHost("::1")).toBe(true);
  // RFC 6761 reserves `.localhost`: it always resolves to loopback and cannot be
  // registered, so a sub-hostname under it is loopback too. Real hosts use this -
  // dotli serves its host realm from `host.localhost`, and dials the debugger
  // from that origin - so rejecting it locks the shipped host out entirely.
  expect(isLoopbackDebugHost("host.localhost")).toBe(true);
  expect(isLoopbackDebugHost("app.host.localhost")).toBe(true);
});

test("isLoopbackDebugHost rejects loopback-looking names under other domains", () => {
  // The dangerous direction: a loopback-shaped label under an attacker's domain.
  // Reading any of these as loopback would let a rebound page past the
  // DNS-rebinding Host guard and the WS Origin gate.
  for (const host of [
    "0.0.0.0",
    "127.0.0.1.evil.com",
    "localhost.evil.com",
    // `.localhost` as a *label*, not the TLD - still an attacker domain.
    "localhost.com",
    "notlocalhost",
    "127.0.0.2",
    "[::1]",
    "example.com",
  ]) {
    expect(isLoopbackDebugHost(host)).toBe(false);
  }
});

test("the Host guard classifies RAW header strings, case included", async () => {
  // `isLoopbackDebugHost` only ever sees a WHATWG-normalized (lowercased)
  // hostname, so asserting `isLoopbackDebugHost("LOCALHOST") === false` encodes a
  // belief the system does NOT have: the gate lowercases first, and `Host:
  // LOCALHOST` is accepted live. Assert through the gate, with raw headers.
  expect(hostHeaderAllowed("LOCALHOST")).toBe(true);
  expect(hostHeaderAllowed("LocalHost:9231")).toBe(true);
  expect(hostHeaderAllowed("127.0.0.1:9231")).toBe(true);
  expect(hostHeaderAllowed("[::1]:9231")).toBe(true);
  // Absent/empty Host: a non-browser client, allowed like a missing Origin.
  expect(hostHeaderAllowed(null)).toBe(true);
  expect(hostHeaderAllowed("")).toBe(true);
  // Case does not launder an attacker domain either.
  expect(hostHeaderAllowed("EVIL.COM")).toBe(false);
  expect(hostHeaderAllowed("LOCALHOST.EVIL.COM")).toBe(false);

  // And live, through the real server, with the raw header on the wire.
  const server = startDebugServer({ port: 0 });
  try {
    const base = `http://localhost:${server.port}`;
    const status = async (host: string): Promise<number> =>
      (await fetch(`${base}/traces`, { headers: { host } })).status;
    expect(await status(`LOCALHOST:${server.port}`)).toBe(200);
    expect(await status(`EVIL.LOCALHOST:${server.port}`)).toBe(403);
  } finally {
    server.stop();
  }
});

test("the Host guard is narrower than the Origin gate: *.localhost is not a target", async () => {
  // `.localhost` is a legitimate *origin* for a page that dials in (dotli serves
  // its host realm from host.localhost), but never a legitimate *target*: this
  // server binds 127.0.0.1 and answers for three names only. Accepting
  // `Host: x.localhost` would only widen the rebinding surface to a
  // wildcard-`*.localhost` zone, for a client that cannot exist.
  expect(isLoopbackDebugHost("host.localhost")).toBe(true);
  expect(hostHeaderAllowed("host.localhost")).toBe(false);
  expect(hostHeaderAllowed("host.localhost:9231")).toBe(false);
  const server = startDebugServer({ port: 0 });
  try {
    const res = await fetch(`http://localhost:${server.port}/traces`, {
      headers: { host: `host.localhost:${server.port}` },
    });
    expect(res.status).toBe(403);
  } finally {
    server.stop();
  }
});

test("an unparseable Host is a 403, not a 500 out of the route dispatcher", async () => {
  const server = startDebugServer({ port: 0 });
  try {
    // Bun builds `req.url` from the Host header, so an out-of-range port makes
    // `new URL(req.url)` throw. The rebinding gate runs first, so the header gets
    // the 403 it already earns instead of a 500 plus a stack trace per request.
    for (const host of ["localhost:99999", "localhost:notaport", "["]) {
      const res = await fetch(`http://localhost:${server.port}/traces`, {
        headers: { host },
      });
      expect(res.status).toBe(403);
    }
  } finally {
    server.stop();
  }
});

test("/frame rejects out-of-range indices (negative and huge) with 404", async () => {
  const server = startDebugServer({ port: 0, decodeValues: true });
  const base = `http://localhost:${server.port}`;
  try {
    const frame = encodeFrame(
      "p:1",
      W.ACCOUNT_GET_ACCOUNT.request,
      new Uint8Array([0]),
    );
    await streamFrame(base, server.port, frame);
    // Integer but out of range ⇒ 404 (no such frame); non-integer ⇒ 400.
    expect((await fetch(`${base}/frame?id=p:1&i=-1`)).status).toBe(404);
    expect((await fetch(`${base}/frame?id=p:1&i=99999`)).status).toBe(404);
    expect((await fetch(`${base}/frame?id=p:1&i=1.5`)).status).toBe(400);
  } finally {
    server.stop();
  }
});

test("a default server decodes every frame, including formerly-sensitive ones", async () => {
  // Dev-only tool: decode is on by default, so a signing frame decodes.
  const server = startDebugServer({ port: 0 });
  const base = `http://localhost:${server.port}`;
  try {
    expect(server.decodeValues).toBe(true);
    await streamFrame(base, server.port, signFrame("p:sign"));
    const detail = await (await fetch(`${base}/frame?id=p:sign&i=0`)).json();
    expect(detail.kind).toBe("decoded");
    expect(JSON.stringify(detail.value)).toContain("alice.dot");
    // No sensitive/redacted machinery: `?reveal=0` is just an unknown param,
    // ignored, and the frame still decodes.
    const still = await (
      await fetch(`${base}/frame?id=p:sign&i=0&reveal=0`)
    ).json();
    expect(still.kind).toBe("decoded");
  } finally {
    server.stop();
  }
});

test("a page with a non-loopback Host header is refused (DNS-rebinding guard)", async () => {
  const server = startDebugServer({ port: 0 });
  const base = `http://localhost:${server.port}`;
  try {
    // A rebound evil.com -> 127.0.0.1 page's same-origin fetch still carries its
    // own Host; a non-loopback (non-bind) Host must be refused with a 403.
    const res = await fetch(`${base}/traces`, {
      headers: { host: "evil.com" },
    });
    expect(res.status).toBe(403);
    // A loopback Host is fine.
    const ok = await fetch(`${base}/traces`, {
      headers: { host: `127.0.0.1:${server.port}` },
    });
    expect(ok.status).toBe(200);
  } finally {
    server.stop();
  }
});

test("groups by (channel, requestId) — two hosts minting the same id do not merge", async () => {
  const server = startDebugServer({ port: 0, decodeValues: true });
  const base = `http://localhost:${server.port}`;
  try {
    // Per-transport counters mean both hosts mint requestId "p:1" for different
    // ops. They must NOT collapse into one trace.
    // Distinct byte lengths so the per-channel drill-down is distinguishable.
    const a = encodeFrame("p:1", W.ACCOUNT_GET_ACCOUNT.request, new Uint8Array([1]));
    const b = encodeFrame(
      "p:1",
      W.CHAIN_GET_HEAD_HEADER.request,
      new Uint8Array([2, 2, 2]),
    );
    const send = async (frame: string, channelId: string) => {
      const ws = new WebSocket(`ws://localhost:${server.port}`);
      await new Promise<void>((resolve, reject) => {
        ws.onopen = () => resolve();
        ws.onerror = () => reject(new Error("ws failed to open"));
      });
      ws.send(
        JSON.stringify({
          channelId,
          dir: "out",
          frame,
          schema: TRUAPI_WIRE_SCHEMA_HASH,
        }),
      );
      await new Promise((r) => setTimeout(r, 40));
      ws.close();
    };
    await send(a, "hostA.dot");
    await send(b, "hostB.dot");

    interface Ch {
      channelId: string;
      requestId: string;
      frames: TraceFrameView[];
    }
    let traces: Ch[] = [];
    for (let i = 0; i < 50; i++) {
      traces = (await (await fetch(`${base}/traces`)).json()) as Ch[];
      if (traces.length >= 2) break;
      await new Promise((r) => setTimeout(r, 20));
    }
    // Two separate traces: same requestId, distinct channels, distinct frames.
    expect(traces).toHaveLength(2);
    const byChannel = new Map(traces.map((t) => [t.channelId, t]));
    expect(byChannel.get("hostA.dot")?.requestId).toBe("p:1");
    expect(byChannel.get("hostB.dot")?.requestId).toBe("p:1");
    expect(byChannel.get("hostA.dot")?.frames[0].frameId).toBe(
      frameIdOf(REAL_W.ACCOUNT_GET_ACCOUNT.trait, REAL_W.ACCOUNT_GET_ACCOUNT.method),
    );
    expect(byChannel.get("hostB.dot")?.frames[0].frameId).toBe(
      frameIdOf(REAL_W.CHAIN_GET_HEAD_HEADER.trait, REAL_W.CHAIN_GET_HEAD_HEADER.method),
    );

    // /frame disambiguates by channel: same id "p:1" resolves to the right
    // host's frame (distinct byte lengths prove it's not the other channel's).
    const detailA = await (
      await fetch(`${base}/frame?id=p:1&i=0&channel=hostA.dot`)
    ).json();
    const detailB = await (
      await fetch(`${base}/frame?id=p:1&i=0&channel=hostB.dot`)
    ).json();
    expect(detailA.byteLength).toBe(1);
    expect(detailB.byteLength).toBe(3);
    expect(detailA).not.toEqual(detailB);
  } finally {
    server.stop();
  }
});

/** Open a WS, run `body`, then close it. */
async function withSocket(
  port: number,
  body: (ws: WebSocket) => Promise<void>,
): Promise<void> {
  const ws = new WebSocket(`ws://localhost:${port}`);
  await new Promise<void>((resolve, reject) => {
    ws.onopen = () => resolve();
    ws.onerror = () => reject(new Error("ws failed to open"));
  });
  try {
    await body(ws);
  } finally {
    ws.close();
  }
}

/** The `/stats` fields these tests assert on. */
interface StatsShape {
  ops: number;
  frames: number;
  droppedByHost: number;
  envelopeRejects: number;
  envelopeRejectReasons: Record<string, number>;
  oversizedMessages: number;
  abnormalCloses: number;
  invalidDroppedFields: number;
}

/** Poll `/stats` until `done` or the budget runs out; returns the last payload. */
async function statsUntil(
  base: string,
  done: (s: StatsShape) => boolean,
  query = "",
): Promise<StatsShape> {
  let stats = {} as StatsShape;
  for (let i = 0; i < 100; i++) {
    stats = (await (await fetch(`${base}/stats${query}`)).json()) as StatsShape;
    if (done(stats)) return stats;
    await new Promise((r) => setTimeout(r, 20));
  }
  return stats;
}

test("a matching schema with a mismatched envelope version blocks the CHANNEL-LESS decode path", async () => {
  const server = startDebugServer({ port: 0, decodeValues: true });
  const base = `http://localhost:${server.port}`;
  try {
    // The gap a `!identityConfirmed`-only flag leaves open: this host stamps the
    // matching schema hash (so `identityConfirmed`) AND a wrong envelope version
    // (so `identityMismatch`). The scoped path always refused it; the unscoped one
    // must too, because `codec` is the only signal for a payload-layout drift the
    // schema hash is blind to — and the shipped UI itself omits `&channel=` when
    // it has no channel.
    await withSocket(server.port, async (ws) => {
      ws.send(
        JSON.stringify({
          v: 2,
          codec: 1,
          schema: TRUAPI_WIRE_SCHEMA_HASH,
          channelId: "drift.dot",
          dir: "out",
          frame: signFrame("p:sign"),
        }),
      );
      for (let i = 0; i < 50; i++) {
        const t = (await (await fetch(`${base}/traces`)).json()) as unknown[];
        if (t.length > 0) break;
        await new Promise((r) => setTimeout(r, 20));
      }
    });

    // Scoped by channel: refused (this already held).
    expect(
      (await fetch(`${base}/frame?id=p:sign&i=0&channel=drift.dot`)).status,
    ).toBe(409);
    // UNSCOPED: must be refused too — the hole.
    expect((await fetch(`${base}/frame?id=p:sign&i=0`)).status).toBe(409);
    // And the HTML drill-down the default page actually renders must not carry the
    // decoded payload either.
    const html = await (await fetch(`${base}/op?id=p:sign`)).text();
    expect(html).not.toContain("alice.dot");
    expect(html).toContain("payload not shown");
    // Payload-blind grouping is unaffected.
    const traces = (await (await fetch(`${base}/traces`)).json()) as unknown[];
    expect(traces.length).toBe(1);
  } finally {
    server.stop();
  }
});

test("a frame larger than the engine's per-trace budget is ingested, not killed by the WS cap", async () => {
  const server = startDebugServer({ port: 0, decodeValues: true });
  const base = `http://localhost:${server.port}`;
  try {
    // 1.5 MiB raw payload: base64 inflates it by 4/3, so a 1 MiB message cap would
    // sit BELOW what the producers can legitimately send. Bun does not drop an
    // over-cap message, it closes the socket (1006) without ever calling
    // `message()`, so the whole stream would die mid-session with every counter
    // untouched.
    const big = encodeFrame(
      "p:big",
      W.ACCOUNT_GET_ACCOUNT.request,
      new Uint8Array(1536 * 1024).fill(7),
    );
    expect(Buffer.from(big, "base64").length).toBeGreaterThan(1024 * 1024);
    const traces = await streamFrame(base, server.port, big);
    expect(traces).toHaveLength(1);
    expect(traces[0].requestId).toBe("p:big");
    const stats = (await (await fetch(`${base}/stats`)).json()) as {
      oversizedMessages: number;
      abnormalCloses: number;
    };
    expect(stats.oversizedMessages).toBe(0);
    expect(stats.abnormalCloses).toBe(0);
  } finally {
    server.stop();
  }
});

test("a socket closed for an over-cap message is counted and surfaced on /stats", async () => {
  const server = startDebugServer({ port: 0 });
  const base = `http://localhost:${server.port}`;
  try {
    // Above MAX_INBOUND_MESSAGE_BYTES (9 MiB): Bun closes with 1006 "Received too
    // big message" and never calls `message()`. Without a counter here the loss is
    // literally unobservable — /stats byte-for-byte identical before and after.
    await withSocket(server.port, async (ws) => {
      ws.send("x".repeat(10 * 1024 * 1024));
      await new Promise((r) => setTimeout(r, 200));
    });
    const stats = await statsUntil(base, (s) => s.oversizedMessages === 1);
    expect(stats.oversizedMessages).toBe(1);
    expect(stats.abnormalCloses).toBe(1);
    // Nothing was ingested, and no envelope reject is claimed: the message never
    // reached the parser.
    expect(stats.envelopeRejects).toBe(0);
    expect(stats.frames).toBe(0);
  } finally {
    server.stop();
  }
});

test("envelope-level rejects are counted by reason and surfaced on /stats", async () => {
  const server = startDebugServer({ port: 0 });
  const base = `http://localhost:${server.port}`;
  try {
    const frame = encodeFrame("p:1", W.SYSTEM_HANDSHAKE.request, new Uint8Array([1]));
    await withSocket(server.port, async (ws) => {
      ws.send("{not json");
      ws.send("42");
      ws.send(JSON.stringify({ channelId: "a.dot", dir: "sideways", frame }));
      // A renamed field — the shape a wire-envelope drift actually takes.
      ws.send(JSON.stringify({ channel_id: "a.dot", dir: "out", frame }));
      ws.send(JSON.stringify({ channelId: "a.dot", dir: "out" }));
      // `""` collides with the "all channels" sentinel: that host could never be
      // selected or decode-scoped, so it is refused at ingest.
      ws.send(JSON.stringify({ channelId: "", dir: "out", frame }));
      await new Promise((r) => setTimeout(r, 100));
    });
    const stats = await statsUntil(base, (s) => s.envelopeRejects === 6);
    expect(stats.envelopeRejects).toBe(6);
    expect(stats.envelopeRejectReasons).toEqual({
      "bad-json": 1,
      "not-object": 1,
      "bad-channel-id": 1,
      "empty-channel-id": 1,
      "bad-dir": 1,
      "bad-frame": 1,
      "ingest-threw": 0,
    });
    // Six refusals and nothing ingested: /traces and /channels stay empty, which
    // without the counters is indistinguishable from "the host never dialed".
    expect(((await (await fetch(`${base}/traces`)).json()) as unknown[]).length).toBe(0);
    const channels = (await (await fetch(`${base}/channels`)).json()) as {
      channels: unknown[];
    };
    expect(channels.channels.length).toBe(0);
  } finally {
    server.stop();
  }
});

test("the inspector page renders the socket count from /channels", async () => {
  const server = startDebugServer({ port: 0 });
  try {
    const html = await (await fetch(`http://localhost:${server.port}/`)).text();
    // `sockets` was computed and serialized but never rendered: one socket with
    // zero ops (a host that is talking and being refused) looked identical to no
    // host at all.
    expect(html).toContain("data.sockets");
    expect(html).toContain("socket");
    // The summary strip reports link-level loss even with zero ops.
    expect(html).toContain("linkLoss");
  } finally {
    server.stop();
  }
});

test("/channels counts an open socket", async () => {
  const server = startDebugServer({ port: 0 });
  const base = `http://localhost:${server.port}`;
  try {
    await withSocket(server.port, async () => {
      let sockets = 0;
      for (let i = 0; i < 50 && sockets === 0; i++) {
        sockets = (
          (await (await fetch(`${base}/channels`)).json()) as { sockets: number }
        ).sockets;
        if (sockets === 0) await new Promise((r) => setTimeout(r, 20));
      }
      expect(sockets).toBe(1);
    });
  } finally {
    server.stop();
  }
});

test("a non-integer `dropped` cannot poison the session's droppedByHost total", async () => {
  const server = startDebugServer({ port: 0 });
  const base = `http://localhost:${server.port}`;
  try {
    const frame = encodeFrame("p:1", W.SYSTEM_HANDSHAKE.request, new Uint8Array([1]));
    await withSocket(server.port, async (ws) => {
      // Raw text, because `JSON.stringify` would already turn Infinity into null:
      // `1e999` is VALID JSON that parses to Infinity. Summed, the whole session's
      // total becomes Infinity, which `JSON.stringify` emits as `null` and the UI
      // renders as "0 dropped" for every channel — the declared
      // `droppedByHost: number` contract broken by one envelope.
      ws.send(
        `{"channelId":"liar.dot","dir":"out","frame":"${frame}",` +
          `"schema":"${TRUAPI_WIRE_SCHEMA_HASH}","dropped":1e999}`,
      );
      // A real host's honest count, on another channel, must survive it.
      ws.send(
        JSON.stringify({
          channelId: "honest.dot",
          dir: "out",
          frame,
          schema: TRUAPI_WIRE_SCHEMA_HASH,
          dropped: 5,
        }),
      );
      // Wrong types are discarded too, and counted.
      ws.send(
        JSON.stringify({
          channelId: "liar.dot",
          dir: "out",
          frame,
          schema: TRUAPI_WIRE_SCHEMA_HASH,
          dropped: "5",
        }),
      );
      ws.send(
        JSON.stringify({
          channelId: "liar.dot",
          dir: "out",
          frame,
          schema: TRUAPI_WIRE_SCHEMA_HASH,
          dropped: 1.5,
        }),
      );
      await new Promise((r) => setTimeout(r, 100));
    });
    const stats = await statsUntil(base, (s) => s.invalidDroppedFields === 3);
    // A finite integer total, not `null` — and the honest host's 5 is intact.
    expect(stats.droppedByHost).toBe(5);
    expect(stats.invalidDroppedFields).toBe(3);
    const scoped = await statsUntil(
      base,
      () => true,
      "?channel=liar.dot",
    );
    expect(scoped.droppedByHost).toBe(0);
    // The raw JSON must not carry a `null` where a number is declared.
    const raw = await (await fetch(`${base}/stats`)).text();
    expect(raw).not.toContain('"droppedByHost":null');
  } finally {
    server.stop();
  }
});

test("/view renders a bounded window, not every trace with every payload", async () => {
  const server = startDebugServer({ port: 0, decodeValues: true });
  const base = `http://localhost:${server.port}`;
  try {
    const total = 25;
    await withSocket(server.port, async (ws) => {
      for (let i = 0; i < total; i++) {
        ws.send(
          JSON.stringify({
            channelId: "myapp.dot",
            dir: "out",
            frame: signFrame(`p:${i}`),
            schema: TRUAPI_WIRE_SCHEMA_HASH,
          }),
        );
      }
      for (let i = 0; i < 100; i++) {
        const t = (await (await fetch(`${base}/traces`)).json()) as unknown[];
        if (t.length >= total) break;
        await new Promise((r) => setTimeout(r, 20));
      }
    });

    const count = (html: string): number =>
      html.split('data-request-id="').length - 1;
    // Unbounded, this endpoint renders every retained frame's decoded value into
    // one string — at the engine's own caps that is hundreds of MB and seconds of
    // blocked event loop for a single GET.
    const first = await (await fetch(`${base}/view`)).text();
    expect(count(first)).toBe(20);
    expect(first).toContain(`showing 1-20 of ${total} ops`);
    // The window is addressable, so nothing is unreachable.
    const rest = await (await fetch(`${base}/view?offset=20`)).text();
    expect(count(rest)).toBe(5);
    expect(rest).not.toContain("showing");
    const small = await (await fetch(`${base}/view?limit=2`)).text();
    expect(count(small)).toBe(2);
    // A malformed or unbounded window is a 400, never an unbounded render.
    for (const q of ["?limit=0", "?limit=101", "?limit=abc", "?offset=-1", "?offset=1.5"]) {
      expect((await fetch(`${base}/view${q}`)).status).toBe(400);
    }
  } finally {
    server.stop();
  }
});

test("an empty ?channel= means all channels, not a channel named ''", async () => {
  const server = startDebugServer({ port: 0, decodeValues: true });
  const base = `http://localhost:${server.port}`;
  try {
    await streamFrame(base, server.port, signFrame("p:sign"));
    // A client building the query with `?? ""` used to pin itself to a channel that
    // can never exist, and got a permanent 409 "codec mismatch" that was false.
    const detail = await fetch(`${base}/frame?id=p:sign&i=0&channel=`);
    expect(detail.status).toBe(200);
    expect(JSON.stringify(await detail.json())).toContain("alice.dot");
    const html = await (await fetch(`${base}/op?id=p:sign&channel=&gen=0`)).text();
    expect(html).toContain("alice.dot");
    // The list endpoints agree: empty means unfiltered, not "no such channel".
    expect(await (await fetch(`${base}/op-list?channel=`)).text()).toContain(
      'data-request-id="p:sign"',
    );
    const stats = (await (await fetch(`${base}/stats?channel=`)).json()) as {
      ops: number;
    };
    expect(stats.ops).toBe(1);
  } finally {
    server.stop();
  }
});

test("every numeric query param goes through the same canonical-integer parse", async () => {
  const server = startDebugServer({ port: 0, decodeValues: true });
  const base = `http://localhost:${server.port}`;
  try {
    const frame = encodeFrame("p:1", W.ACCOUNT_GET_ACCOUNT.request, new Uint8Array([0]));
    await streamFrame(base, server.port, frame);
    // `?i=` used to bypass this file's own `optionalInt`, so `Number()` coercion
    // resolved a real frame for four spellings of "not an integer" while `?gen=`
    // correctly 400'd on the same input — split-brain inside one file.
    for (const i of ["-0", "0x0", "1e1", "007", "+1", " ", ""]) {
      const res = await fetch(`${base}/frame?id=p:1&i=${encodeURIComponent(i)}`);
      expect(res.status).toBe(400);
    }
    // Canonical values still resolve (or 404 out of range), unchanged.
    expect((await fetch(`${base}/frame?id=p:1&i=0`)).status).toBe(200);
    expect((await fetch(`${base}/frame?id=p:1&i=-1`)).status).toBe(404);
    expect((await fetch(`${base}/frame?id=p:1&i=0&gen=0`)).status).toBe(200);
    // Same parse on `?gen=` and on `/view`'s window.
    expect((await fetch(`${base}/frame?id=p:1&i=0&gen=-0`)).status).toBe(400);
    expect((await fetch(`${base}/op?gen=0x0`)).status).toBe(400);
    expect((await fetch(`${base}/view?limit=0x2`)).status).toBe(400);
  } finally {
    server.stop();
  }
});

test("the decode kill-switch fails closed on untrimmed env values", () => {
  // The switch that stops full payload decode must not be defeated by the exact
  // shapes a shell or a .env file produces.
  for (const off of ["0", "false", "no", "off", "OFF", "0 ", " false", "false\n", "\tno\t"]) {
    expect(decodeValuesFromEnv(off)).toBe(false);
  }
  // Anything else (including unset) means on: this is a dev tool that decodes.
  for (const on of [undefined, "", " ", "1", "true", "yes", "0x0", "falsey"]) {
    expect(decodeValuesFromEnv(on)).toBe(true);
  }
});

test("TRUAPI_DEBUGGER_PORT is validated, not silently clamped or coerced", () => {
  expect(portFromEnv("9231")).toBe(9231);
  expect(portFromEnv(" 9231 ")).toBe(9231);
  expect(portFromEnv(undefined)).toBe(9231);
  expect(portFromEnv("")).toBe(9231);
  expect(portFromEnv("65535")).toBe(65535);
  expect(portFromEnv("1")).toBe(1);
  // `Number.isFinite(x) && x > 0` accepted every one of these: 99999 binds a
  // DIFFERENT port (the OS truncates to 65535) that no host's debug URL points
  // at, and 1.5 crashes the process on port 1. A debugger listening somewhere
  // else is indistinguishable from a host that never dialed.
  for (const bad of ["99999", "65536", "1.5", "0", "-1", "1e4", "0x10", "abc", "9231x"]) {
    expect(portFromEnv(bad)).toBeNull();
  }
});

test("a replayed backlog keeps the producer's clock through the real socket", async () => {
  // The seam that hid the original bug: the producer stamped `observedAt` and
  // ingest honoured it, but the server built its envelope without the field, so
  // the fix was invisible through the only mount a host actually dials. Drive it
  // end to end rather than unit-testing either half.
  const server = startDebugServer({ port: 0 });
  const base = `http://localhost:${server.port}`;
  try {
    const send = (
      requestId: string,
      legId: number,
      dir: "in" | "out",
      observedAt: number,
    ): string => {
      const frameId = Math.floor(legId / 4);
      const messageType = legId % 4;
      const encoded = encodeWireMessage({
        requestId,
        payload: {
          traitId: Math.floor(frameId / 256),
          methodId: frameId % 256,
          messageType,
          value: new Uint8Array([0]),
        },
      });
      if (encoded.isErr()) throw encoded.error;
      return JSON.stringify({
        channelId: "myapp.dot",
        dir,
        frame: Buffer.from(encoded.value).toString("base64"),
        schema: TRUAPI_WIRE_SCHEMA_HASH,
        codec: TRUAPI_CODEC_VERSION,
        v: WIRE_ENVELOPE_VERSION,
        observedAt,
        buffered: true,
      });
    };

    const ws = new WebSocket(`ws://localhost:${server.port}`);
    await new Promise<void>((resolve, reject) => {
      ws.onopen = () => resolve();
      ws.onerror = () => reject(new Error("ws failed to open"));
    });
    // A 900ms round trip, replayed long after the fact in one burst.
    const origin = 1_700_000_000_000;
    ws.send(send("p:1", W.ACCOUNT_GET_ACCOUNT.request, "out", origin));
    ws.send(send("p:1", W.ACCOUNT_GET_ACCOUNT.response, "in", origin + 900));

    let traces: { requestId: string; startedAt: number; lastAt: number }[] = [];
    for (let i = 0; i < 50 && traces.length === 0; i++) {
      await new Promise((r) => setTimeout(r, 20));
      traces = (await (await fetch(`${base}/traces`)).json()) as typeof traces;
    }
    ws.close();

    const op = traces.find((t) => t.requestId === "p:1");
    expect(op).toBeDefined();
    // The producer's own span, not the 0ms a flush-instant clock would report.
    expect((op?.lastAt ?? 0) - (op?.startedAt ?? 0)).toBe(900);
    // And the op is anchored to when it really happened, not to now.
    expect(op?.startedAt).toBe(origin);
  } finally {
    server.stop();
  }
});

test("a host-terminated subscription stops counting as live on /stats", async () => {
  // `interrupt` ends a subscription just as `stop` does — a chain switch or a
  // revoked permission is ordinary lifecycle, not an anomaly. Testing only for
  // `stop` left every such subscription "live" forever, so the tile climbed all
  // session while the op list beside it showed nothing live. The two mounts
  // disagreed because each had its own aggregation; both now share one.
  const server = startDebugServer({ port: 0 });
  const base = `http://localhost:${server.port}`;
  try {
    const send = (legId: number, dir: "in" | "out"): string => {
      const frameId = Math.floor(legId / 4);
      const messageType = legId % 4;
      const encoded = encodeWireMessage({
        requestId: "p:1",
        payload: {
          traitId: Math.floor(frameId / 256),
          methodId: frameId % 256,
          messageType,
          value: new Uint8Array([0]),
        },
      });
      if (encoded.isErr()) throw encoded.error;
      return JSON.stringify({
        channelId: "myapp.dot",
        dir,
        frame: Buffer.from(encoded.value).toString("base64"),
        schema: TRUAPI_WIRE_SCHEMA_HASH,
        codec: TRUAPI_CODEC_VERSION,
        v: WIRE_ENVELOPE_VERSION,
      });
    };

    const ws = new WebSocket(`ws://localhost:${server.port}`);
    await new Promise<void>((resolve, reject) => {
      ws.onopen = () => resolve();
      ws.onerror = () => reject(new Error("ws failed to open"));
    });
    ws.send(send(W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.start, "out"));
    ws.send(send(W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.interrupt, "in"));

    let stats = { subscriptions: 0, liveSubscriptions: -1 };
    for (let i = 0; i < 50 && stats.subscriptions === 0; i++) {
      await new Promise((r) => setTimeout(r, 20));
      stats = (await (await fetch(`${base}/stats`)).json()) as typeof stats;
    }
    ws.close();

    expect(stats.subscriptions).toBe(1);
    expect(stats.liveSubscriptions).toBe(0);
  } finally {
    server.stop();
  }
});

test("isLoopbackDebugHost refuses the product sandbox realm", () => {
  // `*.app.localhost` is a PRODUCT sandbox realm: an embedding host serves
  // untrusted product code from it. A page there must not be able to dial the
  // debugger and inject frames or drive the decoder. Host realms under
  // `.localhost` stay allowed.
  expect(isLoopbackDebugHost("demo.app.localhost")).toBe(false);
  expect(isLoopbackDebugHost("app.localhost")).toBe(false);
  expect(isLoopbackDebugHost("host.localhost")).toBe(true);
  expect(isLoopbackDebugHost("app.host.localhost")).toBe(true);
});

test("an unattested frame is refused even on a channel that later attests", async () => {
  // The per-frame stamp, in the direction the channel gate cannot cover: frame A
  // arrives with no identity, frame B on the SAME channel attests correctly. As a
  // per-channel latch, B retroactively unlocked A.
  const server = startDebugServer({ port: 0, decodeValues: true });
  const base = `http://localhost:${server.port}`;
  try {
    await withSocket(server.port, async (ws) => {
      // Unattested first.
      ws.send(
        JSON.stringify({
          channelId: "latch.dot",
          dir: "out",
          frame: signFrame("p:a"),
        }),
      );
      // Then a correctly attested frame on the same channel.
      ws.send(
        JSON.stringify({
          v: WIRE_ENVELOPE_VERSION,
          codec: TRUAPI_CODEC_VERSION,
          schema: TRUAPI_WIRE_SCHEMA_HASH,
          channelId: "latch.dot",
          dir: "out",
          frame: signFrame("p:b"),
        }),
      );
      for (let i = 0; i < 50; i++) {
        const t = (await (await fetch(`${base}/traces`)).json()) as unknown[];
        if (t.length >= 2) break;
        await new Promise((r) => setTimeout(r, 20));
      }
    });
    // The attested frame decodes.
    const okRes = await fetch(`${base}/frame?id=p:b&i=0&channel=latch.dot`);
    expect(okRes.status).toBe(200);
    expect(await okRes.text()).toContain("decoded");
    // The unattested one must NOT, despite sharing the channel.
    const badRes = await fetch(`${base}/frame?id=p:a&i=0&channel=latch.dot`);
    const badBody = await badRes.text();
    expect(badBody).not.toContain('"kind":"decoded"');
  } finally {
    server.stop();
  }
});

test("a matching schema with a wrong envelope version is refused", async () => {
  // A host stamping the right schema hash with a wrong `v` is "confirmed AND
  // mismatched", and must not decode.
  //
  // What this pins is the OBSERVABLE refusal, not which gate produced it: at the
  // HTTP surface `decodeTrusted` (the per-channel gate) also refuses this input,
  // so the per-frame stamp's own contribution cannot be isolated here. Mutating
  // the stamp to the schema match alone leaves this test green. The per-frame
  // gate is pinned instead by the latch test above and by decode.test.ts.
  const server = startDebugServer({ port: 0, decodeValues: true });
  const base = `http://localhost:${server.port}`;
  try {
    await withSocket(server.port, async (ws) => {
      // Right schema, wrong envelope version.
      ws.send(
        JSON.stringify({
          v: WIRE_ENVELOPE_VERSION + 5,
          codec: TRUAPI_CODEC_VERSION,
          schema: TRUAPI_WIRE_SCHEMA_HASH,
          channelId: "half.dot",
          dir: "out",
          frame: signFrame("p:h"),
        }),
      );
      for (let i = 0; i < 50; i++) {
        const t = (await (await fetch(`${base}/traces`)).json()) as unknown[];
        if (t.length > 0) break;
        await new Promise((r) => setTimeout(r, 20));
      }
    });
    // Not decoded: the schema matched but the envelope version did not.
    //
    // Pin the STATUS too. The response here is a 409 refusal body, not a served
    // frame, so `not.toContain("decoded")` alone would also be satisfied by a 404
    // or a 500 - i.e. by the endpoint being broken rather than by it refusing.
    const res = await fetch(`${base}/frame?id=p:h&i=0&channel=half.dot`);
    expect(res.status).toBe(409);
    const body = await res.text();
    expect(body).not.toContain('"kind":"decoded"');
    expect(body).toContain("decode refused");
  } finally {
    server.stop();
  }
});
