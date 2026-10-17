import { describe, expect, test } from "bun:test";
import * as S from "./scale.js";
import * as T from "./generated/types.js";
import { TRUAPI_CODEC_VERSION } from "./generated/client.js";
import {
  JAM_PEER_TRANSPORT_CLOSE,
  JAM_PEER_TRANSPORT_DIAL,
  JAM_PEER_TRANSPORT_EVENTS,
  JAM_PEER_TRANSPORT_OPEN,
  JAM_PEER_TRANSPORT_RECV,
  JAM_PEER_TRANSPORT_SEND,
  SYSTEM_HANDSHAKE,
} from "./generated/wire-table.js";
import { decodeWireMessage, encodeWireMessage, MESSAGE_TYPE_CANCEL, MESSAGE_TYPE_REQUEST, type MethodIds } from "./transport.js";
import {
  createJamPeerTransportSession,
  frameTraitId,
  peerUrl,
  JAM_PEER_TRANSPORT_MAX_BUFFERED_BYTES_PER_CONNECTION,
  JAM_PEER_TRANSPORT_MAX_MESSAGE_BYTES,
  type JamPeerTransportSession,
  type WebTransportBidirectionalStreamLike,
  type WebTransportLike,
} from "./jam-peer-transport.js";

const GENESIS = "0x353963b9cedfe4ea22038081052a5c151b06b55a4a026a97522cd0320cabf49f";
const P256 = "0x028874174c8f469438a1b1bab2fde75f9c4999461382ec6d47e9b3b4511294c607";
const LOOPBACK = "0x00000000000000000000ffff7f000001";

interface FakeStream {
  local: WebTransportBidirectionalStreamLike;
  /** Bytes the session wrote, in order. */
  sent: Uint8Array[];
  peerWrite(bytes: Uint8Array): void;
  peerFin(): void;
}

interface FakeTransport extends WebTransportLike {
  url: string;
  hashes: Uint8Array[];
  streams: FakeStream[];
  /** Whether the session closed this transport. */
  closedByHost: boolean;
  /** Simulate the peer opening a stream toward us. */
  peerOpen(): FakeStream;
  peerClose(): void;
}

function fakeStream(write?: (chunk: Uint8Array) => Promise<void>): FakeStream {
  const sent: Uint8Array[] = [];
  let peerController!: ReadableByteStreamController;
  const readable = new ReadableStream({ type: "bytes", start: (c) => (peerController = c) });
  const writable = new WritableStream<Uint8Array>({
    write: (chunk) => {
      sent.push(chunk);
      return write?.(chunk);
    },
  });
  return {
    local: { readable, writable },
    sent,
    peerWrite: (bytes) => peerController.enqueue(bytes),
    peerFin: () => {
      peerController.close();
      peerController.byobRequest?.respond(0);
    },
  };
}

/** `ready` settles as named; `"hang"` models a handshake that never completes. */
type FakeReady = "ok" | "fail" | "hang";

function fakeTransport(url: string, hashes: Uint8Array[], ready: FakeReady = "ok"): FakeTransport {
  let incoming!: ReadableStreamDefaultController<WebTransportBidirectionalStreamLike>;
  const closed = Promise.withResolvers<void>();
  const streams: FakeStream[] = [];
  const transport: FakeTransport = {
    url,
    hashes,
    streams,
    closedByHost: false,
    ready:
      ready === "ok" ? Promise.resolve() : ready === "fail" ? Promise.reject(new Error("refused")) : new Promise(() => undefined),
    closed: closed.promise,
    incomingBidirectionalStreams: new ReadableStream({ start: (c) => (incoming = c) }),
    async createBidirectionalStream() {
      const stream = fakeStream();
      streams.push(stream);
      return stream.local;
    },
    close: () => {
      transport.closedByHost = true;
      closed.resolve();
    },
    peerOpen() {
      const stream = fakeStream();
      streams.push(stream);
      incoming.enqueue(stream.local);
      return stream;
    },
    peerClose: () => closed.resolve(),
  };
  return transport;
}

let requestCounter = 0;
function frame(ids: MethodIds, value: Uint8Array, requestId = `t${requestCounter++}`, messageType = MESSAGE_TYPE_REQUEST): Uint8Array {
  const encoded = encodeWireMessage({
    requestId,
    payload: { traitId: ids.trait, methodId: ids.method, messageType, value },
  });
  if (encoded.isErr()) throw encoded.error;
  return encoded.value;
}

function decodeReply<V>(bytes: Uint8Array, codec: S.Codec<V>): V {
  const response = decodeWireMessage(bytes);
  if (response.isErr()) throw response.error;
  return codec.dec(response.value.payload.value);
}

async function call<V>(session: JamPeerTransportSession, ids: MethodIds, request: Uint8Array, codec: S.Codec<V>): Promise<V> {
  return decodeReply(await session.handleFrame(frame(ids, request)), codec);
}

const dialCodec = S.Result(T.VersionedHostJamPeerTransportDialResponse, S.CallError(T.VersionedHostJamPeerTransportDialError));
const openCodec = S.Result(T.VersionedHostJamPeerTransportOpenResponse, S.CallError(T.VersionedHostJamPeerTransportOpenError));
const sendCodec = S.Result(T.VersionedHostJamPeerTransportSendResponse, S.CallError(T.VersionedHostJamPeerTransportSendError));
const recvCodec = S.Result(T.VersionedHostJamPeerTransportRecvResponse, S.CallError(T.VersionedHostJamPeerTransportRecvError));
const closeCodec = S.Result(T.VersionedHostJamPeerTransportCloseResponse, S.CallError(T.VersionedHostJamPeerTransportCloseError));
const eventsCodec = S.Result(T.VersionedHostJamPeerTransportEventsResponse, S.CallError(T.VersionedHostJamPeerTransportEventsError));
const handshakeCodec = S.Result(T.VersionedHostHandshakeResponse, S.CallError(T.VersionedHostHandshakeError));

function dialRequest(overrides: Partial<T.HostJamPeerTransportDialRequest> = {}): Uint8Array {
  return T.VersionedHostJamPeerTransportDialRequest.enc({
    tag: "V1",
    value: { genesis: GENESIS, ip: LOOPBACK, port: 43000, ed25519: `0x${"11".repeat(32)}`, p256: P256, ...overrides },
  });
}

const OTHER_GENESIS = `0x${"ab".repeat(32)}`;
const NOT_GRANTED = { success: false, value: { tag: "Domain", value: { tag: "V1", value: "NotGranted" } } };

async function negotiated(
  options: {
    /** How the handshake of the `index`-th transport settles. */
    ready?: (index: number) => FakeReady;
    authorize?: (genesis: string) => Promise<boolean>;
    dialTimeoutMs?: number;
  } = {},
): Promise<{ session: JamPeerTransportSession; transports: FakeTransport[] }> {
  const transports: FakeTransport[] = [];
  const session = createJamPeerTransportSession({
    authorize: options.authorize ?? (async (genesis) => genesis === GENESIS),
    now: () => 1_790_380_800,
    dialTimeoutMs: options.dialTimeoutMs,
    connect: (url, hashes) => {
      const transport = fakeTransport(url, hashes, options.ready?.(transports.length));
      transports.push(transport);
      return transport;
    },
  });
  const handshake = await call(session, SYSTEM_HANDSHAKE, T.VersionedHostHandshakeRequest.enc({ tag: "V1", value: { codecVersion: TRUAPI_CODEC_VERSION } }), handshakeCodec);
  expect(handshake.success).toBe(true);
  return { session, transports };
}

async function dialed(): Promise<{ session: JamPeerTransportSession; transport: FakeTransport; conn: number }> {
  const { session, transports } = await negotiated();
  const dial = await call(session, JAM_PEER_TRANSPORT_DIAL, dialRequest(), dialCodec);
  if (!dial.success) throw new Error("dial failed");
  return { session, transport: transports[0]!, conn: dial.value.value.conn };
}

/** Yield one macrotask so stream pumps observe enqueued chunks; 0 ms, not a duration guess. */
function tick(): Promise<void> {
  const { promise, resolve } = Promise.withResolvers<void>();
  setTimeout(resolve, 0);
  return promise;
}

describe("peerUrl", () => {
  test("renders v4-mapped and native IPv6 authorities", () => {
    expect(peerUrl(S.hexToBytes(LOOPBACK), 43000)).toBe("https://127.0.0.1:43000");
    expect(peerUrl(S.hexToBytes(`0x${"00".repeat(15)}01`), 443)).toBe("https://[0:0:0:0:0:0:0:1]:443");
  });
});

describe("authorization", () => {
  const LIMIT = { success: false, value: { tag: "Domain", value: { tag: "V1", value: "Limit" } } };

  test("permission waits consume connection slots and cancellation releases them before a retry", async () => {
    const pending = Promise.withResolvers<boolean>();
    const asked: string[] = [];
    const { session, transports } = await negotiated({
      authorize: (genesis) => {
        asked.push(genesis);
        return genesis === GENESIS ? pending.promise : Promise.resolve(false);
      },
    });
    const dials = Array.from({ length: 8 }, (_, i) =>
      session.handleFrame(frame(JAM_PEER_TRANSPORT_DIAL, dialRequest(), `permission-${i}`)));
    expect(await call(session, JAM_PEER_TRANSPORT_DIAL, dialRequest({ genesis: OTHER_GENESIS }), dialCodec)).toEqual(LIMIT);
    expect(asked).toEqual([GENESIS]);
    expect(transports).toHaveLength(0);
    await session.handleFrame(frame(JAM_PEER_TRANSPORT_DIAL, new Uint8Array(), "permission-0", MESSAGE_TYPE_CANCEL));
    expect(decodeReply(await dials[0]!, dialCodec)).toEqual({ success: false, value: { tag: "Cancelled" } });
    expect(await call(session, JAM_PEER_TRANSPORT_DIAL, dialRequest({ genesis: OTHER_GENESIS }), dialCodec)).toEqual(NOT_GRANTED);
    expect(asked).toEqual([GENESIS, OTHER_GENESIS]);
    pending.resolve(true);
    expect((await Promise.all(dials.slice(1))).every((bytes) => decodeReply(bytes, dialCodec).success)).toBe(true);
    expect((await call(session, JAM_PEER_TRANSPORT_DIAL, dialRequest(), dialCodec)).success).toBe(true);
    expect(await call(session, JAM_PEER_TRANSPORT_DIAL, dialRequest(), dialCodec)).toEqual(LIMIT);
    expect(transports).toHaveLength(8);
    session.close();
  });

  test("denied and failed distinct-genesis decisions fill the lifetime budget without eviction", async () => {
    const asked: string[] = [];
    const { session, transports } = await negotiated({
      authorize: async (genesis) => {
        asked.push(genesis);
        if (asked.length % 2 === 0) throw new Error("dismissed");
        return false;
      },
    });
    const networks = Array.from({ length: 9 }, (_, i) => `0x${i.toString(16).padStart(2, "0").repeat(32)}` as const);
    for (const genesis of networks.slice(0, 8)) {
      expect(await call(session, JAM_PEER_TRANSPORT_DIAL, dialRequest({ genesis }), dialCodec)).toEqual(NOT_GRANTED);
    }
    expect(await call(session, JAM_PEER_TRANSPORT_DIAL, dialRequest({ genesis: networks[8]! }), dialCodec)).toEqual(LIMIT);
    expect(await call(session, JAM_PEER_TRANSPORT_DIAL, dialRequest({ genesis: networks[0]! }), dialCodec)).toEqual(NOT_GRANTED);
    expect(asked).toEqual(networks.slice(0, 8));
    expect(transports).toHaveLength(0);
    session.close();
  });

  test("cancelled distinct-network prompts stay bounded and close withdraws repeated waiters", async () => {
    const pending = Promise.withResolvers<boolean>();
    const asked: string[] = [];
    const { session, transports } = await negotiated({
      authorize: (genesis) => {
        asked.push(genesis);
        return pending.promise;
      },
    });
    const networks = Array.from({ length: 9 }, (_, i) => `0x${i.toString(16).padStart(2, "0").repeat(32)}` as const);
    for (const genesis of networks.slice(0, 8)) {
      const dial = session.handleFrame(frame(JAM_PEER_TRANSPORT_DIAL, dialRequest({ genesis }), "cancel-prompt"));
      await session.handleFrame(frame(JAM_PEER_TRANSPORT_DIAL, new Uint8Array(), "cancel-prompt", MESSAGE_TYPE_CANCEL));
      expect(decodeReply(await dial, dialCodec)).toEqual({ success: false, value: { tag: "Cancelled" } });
    }
    expect(await call(session, JAM_PEER_TRANSPORT_DIAL, dialRequest({ genesis: networks[8]! }), dialCodec)).toEqual(LIMIT);
    // Repeated cancelled subscribers must not hold operation slots or ask again.
    for (let i = 0; i < 16; i++) {
      const dial = session.handleFrame(frame(JAM_PEER_TRANSPORT_DIAL, dialRequest({ genesis: networks[0]! }), "retry-prompt"));
      await session.handleFrame(frame(JAM_PEER_TRANSPORT_DIAL, new Uint8Array(), "retry-prompt", MESSAGE_TYPE_CANCEL));
      expect(decodeReply(await dial, dialCodec)).toEqual({ success: false, value: { tag: "Cancelled" } });
    }
    const waiting = Array.from({ length: 8 }, () => call(session, JAM_PEER_TRANSPORT_DIAL,
      dialRequest({ genesis: networks[0]! }), dialCodec));
    session.close();
    expect(await Promise.all(waiting)).toEqual(Array.from({ length: 8 }, () => ({ success: false, value: { tag: "Denied" } })));
    pending.resolve(true);
    await tick();
    expect(asked).toEqual(networks.slice(0, 8));
    expect(transports).toHaveLength(0);
  });

  test("dial before the handshake is NotGranted and asks nothing", async () => {
    const asked: string[] = [];
    const session = createJamPeerTransportSession({
      authorize: async (genesis) => {
        asked.push(genesis);
        return true;
      },
      connect: () => fakeTransport("", []),
    });
    const dial = await call(session, JAM_PEER_TRANSPORT_DIAL, dialRequest(), dialCodec);
    expect(dial).toEqual(NOT_GRANTED);
    expect(asked).toEqual([]);
  });

  test("a granted genesis is asked once for the whole session", async () => {
    const asked: string[] = [];
    const { session, transports } = await negotiated({
      authorize: async (genesis) => {
        asked.push(genesis);
        return true;
      },
    });
    for (let i = 0; i < 3; i++) {
      expect((await call(session, JAM_PEER_TRANSPORT_DIAL, dialRequest(), dialCodec)).success).toBe(true);
    }
    expect(asked).toEqual([GENESIS]);
    expect(transports).toHaveLength(3);
  });

  test("a denied or failed decision is NotGranted, remembered, and connects nothing", async () => {
    const asked: string[] = [];
    const { session, transports } = await negotiated({
      authorize: async (genesis) => {
        asked.push(genesis);
        if (genesis === OTHER_GENESIS) throw new Error("prompt dismissed");
        return false;
      },
    });
    for (let i = 0; i < 2; i++) {
      expect(await call(session, JAM_PEER_TRANSPORT_DIAL, dialRequest(), dialCodec)).toEqual(NOT_GRANTED);
      expect(await call(session, JAM_PEER_TRANSPORT_DIAL, dialRequest({ genesis: OTHER_GENESIS }), dialCodec)).toEqual(NOT_GRANTED);
    }
    expect(asked).toEqual([GENESIS, OTHER_GENESIS]);
    expect(transports).toHaveLength(0);
  });

  test("concurrent dials share one pending decision per genesis", async () => {
    const asked: string[] = [];
    const pending = new Map<string, (granted: boolean) => void>();
    const { session, transports } = await negotiated({
      authorize: (genesis) => {
        asked.push(genesis);
        const { promise, resolve } = Promise.withResolvers<boolean>();
        pending.set(genesis, resolve);
        return promise;
      },
    });
    const granted = [0, 1, 2].map(() => call(session, JAM_PEER_TRANSPORT_DIAL, dialRequest(), dialCodec));
    const refused = [0, 1].map(() => call(session, JAM_PEER_TRANSPORT_DIAL, dialRequest({ genesis: OTHER_GENESIS }), dialCodec));
    await tick();
    expect(asked).toEqual([GENESIS, OTHER_GENESIS]);
    expect(transports).toHaveLength(0);

    pending.get(GENESIS)!(true);
    pending.get(OTHER_GENESIS)!(false);
    expect((await Promise.all(granted)).map((dial) => dial.success)).toEqual([true, true, true]);
    expect(await Promise.all(refused)).toEqual([NOT_GRANTED, NOT_GRANTED]);
    expect(asked).toEqual([GENESIS, OTHER_GENESIS]);
    expect(transports).toHaveLength(3);
  });

  test("closing the session answers a dial still waiting for its decision, and connects nothing", async () => {
    const { promise, resolve } = Promise.withResolvers<boolean>();
    const { session, transports } = await negotiated({ authorize: () => promise });
    const dial = call(session, JAM_PEER_TRANSPORT_DIAL, dialRequest(), dialCodec);
    await tick();
    session.close();
    expect(await dial).toEqual({ success: false, value: { tag: "Denied" } });
    resolve(true);
    await tick();
    expect(transports).toHaveLength(0);
  });

  test("a granted dial without p256 is Unreachable in the browser", async () => {
    const { session, transports } = await negotiated();
    const quicOnly = await call(session, JAM_PEER_TRANSPORT_DIAL, dialRequest({ p256: undefined }), dialCodec);
    expect(quicOnly).toEqual({ success: false, value: { tag: "Domain", value: { tag: "V1", value: "Unreachable" } } });
    expect(transports).toHaveLength(0);
  });

  test("frames for other traits are Denied and the trait id is exposed for routing", async () => {
    const { session } = await negotiated();
    const bytes = frame({ trait: 20, method: 0, kind: "request" }, new Uint8Array());
    expect(frameTraitId(bytes)).toBe(20);
    const response = decodeWireMessage(await session.handleFrame(bytes));
    expect(response.isOk() && S.Result(S._void, S.CallError(S._void)).dec(response.value.payload.value)).toEqual({ success: false, value: { tag: "Denied" } });
  });
});

describe("dial", () => {
  test("connects to the peer URL with both serial variants of the three period certificate hashes", async () => {
    const { transport, conn } = await dialed();
    expect(conn).toBe(1);
    expect(transport.url).toBe("https://127.0.0.1:43000");
    expect(transport.hashes.map((h) => S.bytesToHex(h))).toEqual([
      "0x51fc12ea78bc97eb7d969bd4ff221f2063f205111893cbff22cd9a1b5f8c8ad6",
      "0xccf30196b29007b42fca6f406363ce17781bab0f011bac47dcbe307e0e6a316d",
      "0x7d89ee4abc9820e55e5f8c3b9cdfb10967391be26707f5d3397bf2bd46cdea08",
      "0xeb09b6b027f5953cb8ca2e8f296e21052c3423370876e67f1634180ddf99f1ec",
      "0x505c184ed39a7dfa39876a5a1734d118f8fb9ad90932b78d7c90d3a062646224",
      "0x8bdfa3a2b7822822f5da33fadfa118d39d05b0a6b1086fc82a62a2209f6fae27",
    ]);
  });

  test("a rejected handshake is Refused and holds no connection slot", async () => {
    const { session } = await negotiated({ ready: () => "fail" });
    const dial = await call(session, JAM_PEER_TRANSPORT_DIAL, dialRequest(), dialCodec);
    expect(dial).toEqual({ success: false, value: { tag: "Domain", value: { tag: "V1", value: "Refused" } } });
    const close = await call(session, JAM_PEER_TRANSPORT_CLOSE, T.VersionedHostJamPeerTransportCloseRequest.enc({ tag: "V1", value: { conn: 1 } }), closeCodec);
    expect(close.success).toBe(false);
  });

  test("the ninth connection hits Limit", async () => {
    const { session } = await negotiated();
    for (let i = 0; i < 8; i++) {
      expect((await call(session, JAM_PEER_TRANSPORT_DIAL, dialRequest(), dialCodec)).success).toBe(true);
    }
    const ninth = await call(session, JAM_PEER_TRANSPORT_DIAL, dialRequest(), dialCodec);
    expect(ninth).toEqual({ success: false, value: { tag: "Domain", value: { tag: "V1", value: "Limit" } } });
  });
});

describe("withdrawn dials", () => {
  const UNREACHABLE = { success: false, value: { tag: "Domain", value: { tag: "V1", value: "Unreachable" } } };
  const CANCELLED = { success: false, value: { tag: "Cancelled" } };
  const cancel = (session: JamPeerTransportSession, requestId: string): Promise<Uint8Array> =>
    session.handleFrame(frame(JAM_PEER_TRANSPORT_DIAL, new Uint8Array(), requestId, MESSAGE_TYPE_CANCEL));
  const events = async (session: JamPeerTransportSession): Promise<unknown> =>
    call(session, JAM_PEER_TRANSPORT_EVENTS, T.VersionedHostJamPeerTransportEventsRequest.enc({ tag: "V1", value: undefined }), eventsCodec);

  test("a prompt outlasting the guest's wait opens nothing, and the retry reuses the answer", async () => {
    const asked: string[] = [];
    const decision = Promise.withResolvers<boolean>();
    const { session, transports } = await negotiated({
      dialTimeoutMs: 20,
      authorize: (genesis) => {
        asked.push(genesis);
        return decision.promise;
      },
    });
    expect(await call(session, JAM_PEER_TRANSPORT_DIAL, dialRequest(), dialCodec)).toEqual(UNREACHABLE);
    // The user answers after the guest stopped waiting: that dial opens nothing.
    decision.resolve(true);
    await tick();
    expect(transports).toHaveLength(0);

    expect((await call(session, JAM_PEER_TRANSPORT_DIAL, dialRequest(), dialCodec)).success).toBe(true);
    expect(asked).toEqual([GENESIS]);
    expect(transports).toHaveLength(1);
  });

  test("a handshake outlasting the deadline is closed and frees its slot", async () => {
    const { session, transports } = await negotiated({ dialTimeoutMs: 20, ready: (index) => (index === 0 ? "hang" : "ok") });
    expect(await call(session, JAM_PEER_TRANSPORT_DIAL, dialRequest(), dialCodec)).toEqual(UNREACHABLE);
    expect(transports[0]!.closedByHost).toBe(true);
    for (let i = 0; i < 8; i++) {
      expect((await call(session, JAM_PEER_TRANSPORT_DIAL, dialRequest(), dialCodec)).success).toBe(true);
    }
    expect(await events(session)).toEqual({ success: true, value: { tag: "V1", value: { events: [] } } });
  });

  test("a CANCEL during the prompt answers Cancelled and opens nothing", async () => {
    const decision = Promise.withResolvers<boolean>();
    const { session, transports } = await negotiated({ authorize: () => decision.promise });
    const dial = session.handleFrame(frame(JAM_PEER_TRANSPORT_DIAL, dialRequest(), "d1"));
    await tick();
    expect(await cancel(session, "d1")).toEqual(new Uint8Array());
    expect(decodeReply(await dial, dialCodec)).toEqual(CANCELLED);
    decision.resolve(true);
    await tick();
    expect(transports).toHaveLength(0);
    // A CANCEL naming nothing in flight lost the race and changes nothing.
    expect(await cancel(session, "d1")).toEqual(new Uint8Array());
    expect((await call(session, JAM_PEER_TRANSPORT_DIAL, dialRequest(), dialCodec)).success).toBe(true);
  });

  test("a CANCEL during the handshake closes the connection without consuming the cap", async () => {
    const { session, transports } = await negotiated({ ready: (index) => (index === 7 ? "hang" : "ok") });
    for (let i = 0; i < 7; i++) {
      expect((await call(session, JAM_PEER_TRANSPORT_DIAL, dialRequest(), dialCodec)).success).toBe(true);
    }
    const eighth = session.handleFrame(frame(JAM_PEER_TRANSPORT_DIAL, dialRequest(), "d8"));
    await tick();
    expect(transports).toHaveLength(8);
    await cancel(session, "d8");
    expect(decodeReply(await eighth, dialCodec)).toEqual(CANCELLED);
    expect(transports[7]!.closedByHost).toBe(true);

    expect((await call(session, JAM_PEER_TRANSPORT_DIAL, dialRequest(), dialCodec)).success).toBe(true);
    expect(await call(session, JAM_PEER_TRANSPORT_DIAL, dialRequest(), dialCodec)).toEqual({
      success: false,
      value: { tag: "Domain", value: { tag: "V1", value: "Limit" } },
    });
    expect(await events(session)).toEqual({ success: true, value: { tag: "V1", value: { events: [] } } });
  });

  test("a CANCEL for a completed or unknown dial leaves its connection open", async () => {
    const { session, transports } = await negotiated();
    const reply = decodeReply(await session.handleFrame(frame(JAM_PEER_TRANSPORT_DIAL, dialRequest(), "d1")), dialCodec);
    expect(reply.success).toBe(true);
    expect(await cancel(session, "d1")).toEqual(new Uint8Array());
    expect(await cancel(session, "unknown")).toEqual(new Uint8Array());
    expect(transports[0]!.closedByHost).not.toBe(true);
    expect(await events(session)).toEqual({ success: true, value: { tag: "V1", value: { events: [] } } });
  });
});

describe("streams", () => {
  test("concurrent opens reserve the sixteen slots before transport creation settles", async () => {
    const { session, transport, conn } = await dialed();
    const pending: Array<(stream: WebTransportBidirectionalStreamLike) => void> = [];
    transport.createBidirectionalStream = () => {
      const { promise, resolve } = Promise.withResolvers<WebTransportBidirectionalStreamLike>();
      pending.push(resolve);
      return promise;
    };
    const opens = Array.from({ length: 16 }, () =>
      call(session, JAM_PEER_TRANSPORT_OPEN, T.VersionedHostJamPeerTransportOpenRequest.enc({ tag: "V1", value: { conn, kind: 0 } }), openCodec));
    const excess = await call(session, JAM_PEER_TRANSPORT_OPEN, T.VersionedHostJamPeerTransportOpenRequest.enc({ tag: "V1", value: { conn, kind: 0 } }), openCodec);
    expect(excess).toEqual({ success: false, value: { tag: "Domain", value: { tag: "V1", value: "Limit" } } });
    expect(pending).toHaveLength(16);
    for (const resolve of pending) resolve(fakeStream().local);
    expect((await Promise.all(opens)).every((result) => result.success)).toBe(true);
    session.close();
  });

  test("an incoming stream waiting for its kind reserves a slot too", async () => {
    const { session, transport, conn } = await dialed();
    const incoming = transport.peerOpen();
    await tick();
    for (let i = 0; i < 15; i++) {
      const opened = await call(session, JAM_PEER_TRANSPORT_OPEN, T.VersionedHostJamPeerTransportOpenRequest.enc({ tag: "V1", value: { conn, kind: 0 } }), openCodec);
      expect(opened.success).toBe(true);
    }
    const excess = await call(session, JAM_PEER_TRANSPORT_OPEN, T.VersionedHostJamPeerTransportOpenRequest.enc({ tag: "V1", value: { conn, kind: 0 } }), openCodec);
    expect(excess).toEqual({ success: false, value: { tag: "Domain", value: { tag: "V1", value: "Limit" } } });
    incoming.peerWrite(new Uint8Array([128]));
    await tick();
    const events = await call(session, JAM_PEER_TRANSPORT_EVENTS, T.VersionedHostJamPeerTransportEventsRequest.enc({ tag: "V1" }), eventsCodec);
    expect(events.success && events.value.value.events).toEqual([{ tag: "Accepted", value: { conn, stream: 16, kind: 128 } }]);
    session.close();
  });

  test("cancelled and closed pending opens cannot publish late transport streams", async () => {
    for (const cancel of [true, false]) {
      const { session, transport, conn } = await dialed();
      const pending = Promise.withResolvers<WebTransportBidirectionalStreamLike>();
      transport.createBidirectionalStream = () => pending.promise;
      const opening = session.handleFrame(frame(JAM_PEER_TRANSPORT_OPEN,
        T.VersionedHostJamPeerTransportOpenRequest.enc({ tag: "V1", value: { conn, kind: 0 } }), "pending-open"));
      if (cancel) {
        await session.handleFrame(frame(JAM_PEER_TRANSPORT_OPEN, new Uint8Array(), "pending-open", MESSAGE_TYPE_CANCEL));
        expect(decodeReply(await opening, openCodec)).toEqual({ success: false, value: { tag: "Cancelled" } });
      } else {
        await call(session, JAM_PEER_TRANSPORT_CLOSE, T.VersionedHostJamPeerTransportCloseRequest.enc({ tag: "V1", value: { conn } }), closeCodec);
        expect(decodeReply(await opening, openCodec)).toEqual({ success: false, value: { tag: "Domain", value: { tag: "V1", value: "Closed" } } });
      }
      const late = fakeStream();
      pending.resolve(late.local);
      await tick();
      expect(late.sent).toEqual([]);
      expect(() => late.peerWrite(new Uint8Array([0]))).toThrow();
      session.close();
    }
  });

  test("CANCEL settles blocked kind writes and message writes without waiting for the peer", async () => {
    for (const duringOpen of [true, false]) {
      const { session, transport, conn } = await dialed();
      const blocked = Promise.withResolvers<void>();
      const wire = fakeStream((chunk) => duringOpen || chunk.length > 1 ? blocked.promise : Promise.resolve());
      transport.createBidirectionalStream = async () => wire.local;
      const opening = session.handleFrame(frame(JAM_PEER_TRANSPORT_OPEN,
        T.VersionedHostJamPeerTransportOpenRequest.enc({ tag: "V1", value: { conn, kind: 0 } }), "blocked"));
      let request = opening;
      const ids = duringOpen ? JAM_PEER_TRANSPORT_OPEN : JAM_PEER_TRANSPORT_SEND;
      if (!duringOpen) {
        const opened = decodeReply(await opening, openCodec);
        if (!opened.success) throw new Error("open failed");
        request = session.handleFrame(frame(ids, T.VersionedHostJamPeerTransportSendRequest.enc({
          tag: "V1", value: { stream: opened.value.value.stream, message: "0x01", fin: false },
        }), "blocked"));
      }
      await tick();
      await session.handleFrame(frame(ids, new Uint8Array(), "blocked", MESSAGE_TYPE_CANCEL));
      expect(decodeReply(await request, S.Result(S._void, S.CallError(S._void)))).toEqual({ success: false, value: { tag: "Cancelled" } });
      blocked.resolve();
      await tick();
      session.close();
    }
  });

  test("cancelled writes retain connection quota until the underlying sink settles", async () => {
    const { session, transport, conn } = await dialed();
    const blocked = Promise.withResolvers<void>();
    const sending: Promise<Uint8Array>[] = [];
    for (let i = 0; i < 3; i++) {
      const wire = fakeStream((chunk) => chunk.length > 1 ? blocked.promise : Promise.resolve());
      transport.createBidirectionalStream = async () => wire.local;
      const opened = await call(session, JAM_PEER_TRANSPORT_OPEN,
        T.VersionedHostJamPeerTransportOpenRequest.enc({ tag: "V1", value: { conn, kind: 0 } }), openCodec);
      if (!opened.success) throw new Error("open failed");
      sending.push(session.handleFrame(frame(JAM_PEER_TRANSPORT_SEND,
        T.VersionedHostJamPeerTransportSendRequest.enc({
          tag: "V1", value: { stream: opened.value.value.stream, message: `0x${"00".repeat(JAM_PEER_TRANSPORT_MAX_MESSAGE_BYTES)}`, fin: false },
        }), `retained-${i}`)));
    }
    await tick();
    for (let i = 0; i < sending.length; i++) {
      await session.handleFrame(frame(JAM_PEER_TRANSPORT_SEND, new Uint8Array(), `retained-${i}`, MESSAGE_TYPE_CANCEL));
      expect(decodeReply(await sending[i]!, sendCodec)).toEqual({ success: false, value: { tag: "Cancelled" } });
    }
    transport.createBidirectionalStream = async () => fakeStream().local;
    const opened = await call(session, JAM_PEER_TRANSPORT_OPEN,
      T.VersionedHostJamPeerTransportOpenRequest.enc({ tag: "V1", value: { conn, kind: 0 } }), openCodec);
    if (!opened.success) throw new Error("open failed");
    const request = T.VersionedHostJamPeerTransportSendRequest.enc({
      tag: "V1", value: { stream: opened.value.value.stream, message: `0x${"00".repeat(JAM_PEER_TRANSPORT_MAX_MESSAGE_BYTES)}`, fin: false },
    });
    expect(await call(session, JAM_PEER_TRANSPORT_SEND, request, sendCodec))
      .toEqual({ success: false, value: { tag: "Domain", value: { tag: "V1", value: "Limit" } } });
    blocked.resolve();
    await tick();
    expect((await call(session, JAM_PEER_TRANSPORT_SEND, request, sendCodec)).success).toBe(true);
    session.close();
  });

  test("receive backpressure counts frames across streams and resumes after recv releases space", async () => {
    const { session, transport, conn } = await dialed();
    const ids: number[] = [];
    const length = JAM_PEER_TRANSPORT_MAX_BUFFERED_BYTES_PER_CONNECTION / 4 - 4;
    for (let i = 0; i < 2; i++) {
      const opened = await call(session, JAM_PEER_TRANSPORT_OPEN, T.VersionedHostJamPeerTransportOpenRequest.enc({ tag: "V1", value: { conn, kind: 0 } }), openCodec);
      if (!opened.success) throw new Error("open failed");
      ids.push(opened.value.value.stream);
      for (let message = 0; message < (i === 0 ? 4 : 1); message++) {
        const bytes = new Uint8Array((i === 0 ? length : 1) + 4);
        new DataView(bytes.buffer).setUint32(0, bytes.length - 4, true);
        transport.streams[i]!.peerWrite(bytes);
      }
      await tick();
    }
    const receive = (stream: number) => call(session, JAM_PEER_TRANSPORT_RECV,
      T.VersionedHostJamPeerTransportRecvRequest.enc({ tag: "V1", value: { stream, max: JAM_PEER_TRANSPORT_MAX_MESSAGE_BYTES } }), recvCodec);
    expect(await receive(ids[1]!)).toEqual({ success: true, value: { tag: "V1", value: { message: undefined, fin: false, reset: false } } });
    const first = await receive(ids[0]!);
    expect(first.success && first.value.value.message?.length).toBe(2 + length * 2);
    await tick();
    expect(await receive(ids[1]!)).toEqual({ success: true, value: { tag: "V1", value: { message: "0x00", fin: false, reset: false } } });
    session.close();
  });

  test("FIN excludes concurrent sends before its blocked write completes", async () => {
    const { session, transport, conn } = await dialed();
    const blocked = Promise.withResolvers<void>();
    const wire = fakeStream((chunk) => chunk.length > 1 ? blocked.promise : Promise.resolve());
    transport.createBidirectionalStream = async () => wire.local;
    const opened = await call(session, JAM_PEER_TRANSPORT_OPEN,
      T.VersionedHostJamPeerTransportOpenRequest.enc({ tag: "V1", value: { conn, kind: 0 } }), openCodec);
    if (!opened.success) throw new Error("open failed");
    const stream = opened.value.value.stream;
    const finishing = call(session, JAM_PEER_TRANSPORT_SEND,
      T.VersionedHostJamPeerTransportSendRequest.enc({ tag: "V1", value: { stream, message: "0x01", fin: true } }), sendCodec);
    const following = call(session, JAM_PEER_TRANSPORT_SEND,
      T.VersionedHostJamPeerTransportSendRequest.enc({ tag: "V1", value: { stream, message: "0x02", fin: false } }), sendCodec);
    blocked.resolve();
    expect((await finishing).success).toBe(true);
    expect(await following).toEqual({ success: false, value: { tag: "Domain", value: { tag: "V1", value: "Closed" } } });
    expect(wire.sent).toHaveLength(2);
    session.close();
  });

  test("a peer withholding one kind byte does not block other incoming streams", async () => {
    const { session, transport, conn } = await dialed();
    transport.peerOpen();
    const ready = transport.peerOpen();
    ready.peerWrite(new Uint8Array([128]));
    await tick();
    const events = await call(session, JAM_PEER_TRANSPORT_EVENTS,
      T.VersionedHostJamPeerTransportEventsRequest.enc({ tag: "V1" }), eventsCodec);
    expect(events.success && events.value.value.events).toEqual([
      { tag: "Accepted", value: { conn, stream: 1, kind: 128 } },
    ]);
    session.close();
  });

  test("a final send releases a stream whose receive side was already consumed", async () => {
    const { session, transport, conn } = await dialed();
    for (let i = 0; i < 17; i++) {
      const opened = await call(session, JAM_PEER_TRANSPORT_OPEN,
        T.VersionedHostJamPeerTransportOpenRequest.enc({ tag: "V1", value: { conn, kind: 0 } }), openCodec);
      if (!opened.success) throw new Error("completed stream retained its slot");
      const stream = opened.value.value.stream;
      transport.streams[i]!.peerFin();
      await tick();
      expect(await call(session, JAM_PEER_TRANSPORT_RECV,
        T.VersionedHostJamPeerTransportRecvRequest.enc({ tag: "V1", value: { stream, max: 1 } }), recvCodec))
        .toEqual({ success: true, value: { tag: "V1", value: { message: undefined, fin: true, reset: false } } });
      expect((await call(session, JAM_PEER_TRANSPORT_SEND,
        T.VersionedHostJamPeerTransportSendRequest.enc({ tag: "V1", value: { stream, message: "0x", fin: true } }), sendCodec)).success).toBe(true);
    }
    session.close();
  });

  test("open sends the kind byte; send frames with a u32-LE prefix; recv unframes", async () => {
    const { session, transport, conn } = await dialed();
    const open = await call(session, JAM_PEER_TRANSPORT_OPEN, T.VersionedHostJamPeerTransportOpenRequest.enc({ tag: "V1", value: { conn, kind: 128 } }), openCodec);
    if (!open.success) throw new Error("open failed");
    const stream = open.value.value.stream;
    const send = await call(session, JAM_PEER_TRANSPORT_SEND, T.VersionedHostJamPeerTransportSendRequest.enc({ tag: "V1", value: { stream, message: "0x0102", fin: true } }), sendCodec);
    expect(send.success).toBe(true);
    const wire = transport.streams[0]!;
    expect(wire.sent.map((c) => S.bytesToHex(c))).toEqual(["0x80", "0x020000000102"]);

    // Peer replies with two messages split across arbitrary chunk boundaries, then FIN.
    wire.peerWrite(new Uint8Array([3, 0, 0, 0, 0xaa]));
    wire.peerWrite(new Uint8Array([0xbb, 0xcc, 1, 0, 0]));
    await tick();
    const early = await call(session, JAM_PEER_TRANSPORT_RECV, T.VersionedHostJamPeerTransportRecvRequest.enc({ tag: "V1", value: { stream, max: 1 << 20 } }), recvCodec);
    expect(early).toEqual({ success: true, value: { tag: "V1", value: { message: "0xaabbcc", fin: false, reset: false } } });
    wire.peerWrite(new Uint8Array([0, 0xdd]));
    wire.peerFin();
    await tick();
    await tick();
    const second = await call(session, JAM_PEER_TRANSPORT_RECV, T.VersionedHostJamPeerTransportRecvRequest.enc({ tag: "V1", value: { stream, max: 1 << 20 } }), recvCodec);
    expect(second).toEqual({ success: true, value: { tag: "V1", value: { message: "0xdd", fin: true, reset: false } } });
    const drained = await call(session, JAM_PEER_TRANSPORT_RECV, T.VersionedHostJamPeerTransportRecvRequest.enc({ tag: "V1", value: { stream, max: 1 << 20 } }), recvCodec);
    expect(drained).toEqual({ success: true, value: { tag: "V1", value: { message: undefined, fin: true, reset: false } } });
    const consumed = await call(session, JAM_PEER_TRANSPORT_RECV, T.VersionedHostJamPeerTransportRecvRequest.enc({ tag: "V1", value: { stream, max: 1 << 20 } }), recvCodec);
    expect(consumed).toEqual({ success: false, value: { tag: "Domain", value: { tag: "V1", value: "Closed" } } });
    const events = await call(session, JAM_PEER_TRANSPORT_EVENTS, T.VersionedHostJamPeerTransportEventsRequest.enc({ tag: "V1" }), eventsCodec);
    expect(events).toEqual({ success: true, value: { tag: "V1", value: { events: [{ tag: "StreamFin", value: { stream } }] } } });
  });

  test("oversized send is TooLarge and a message above the caller's max resets the stream", async () => {
    const { session, transport, conn } = await dialed();
    const open = await call(session, JAM_PEER_TRANSPORT_OPEN, T.VersionedHostJamPeerTransportOpenRequest.enc({ tag: "V1", value: { conn, kind: 0 } }), openCodec);
    if (!open.success) throw new Error("open failed");
    const stream = open.value.value.stream;
    const big = `0x${"00".repeat(JAM_PEER_TRANSPORT_MAX_MESSAGE_BYTES + 1)}` as const;
    const send = await call(session, JAM_PEER_TRANSPORT_SEND, T.VersionedHostJamPeerTransportSendRequest.enc({ tag: "V1", value: { stream, message: big, fin: false } }), sendCodec);
    expect(send).toEqual({ success: false, value: { tag: "Domain", value: { tag: "V1", value: "TooLarge" } } });
    transport.streams[0]!.peerWrite(new Uint8Array([2, 0, 0, 0, 1, 2]));
    await tick();
    const recv = await call(session, JAM_PEER_TRANSPORT_RECV, T.VersionedHostJamPeerTransportRecvRequest.enc({ tag: "V1", value: { stream, max: 1 } }), recvCodec);
    expect(recv).toEqual({ success: true, value: { tag: "V1", value: { message: undefined, fin: false, reset: true } } });
  });

  test("peer-opened streams surface as Accepted with their kind byte", async () => {
    const { session, transport, conn } = await dialed();
    const incoming = transport.peerOpen();
    incoming.peerWrite(new Uint8Array([0, 2, 0, 0, 0, 9, 9]));
    await tick();
    await tick();
    const events = await call(session, JAM_PEER_TRANSPORT_EVENTS, T.VersionedHostJamPeerTransportEventsRequest.enc({ tag: "V1" }), eventsCodec);
    expect(events).toEqual({ success: true, value: { tag: "V1", value: { events: [{ tag: "Accepted", value: { conn, stream: 1, kind: 0 } }] } } });
    const recv = await call(session, JAM_PEER_TRANSPORT_RECV, T.VersionedHostJamPeerTransportRecvRequest.enc({ tag: "V1", value: { stream: 1, max: 1 << 20 } }), recvCodec);
    expect(recv).toEqual({ success: true, value: { tag: "V1", value: { message: "0x0909", fin: false, reset: false } } });
  });

  test("a peer close reports ConnClosed and invalidates streams; session close denies everything", async () => {
    const { session, transport, conn } = await dialed();
    const open = await call(session, JAM_PEER_TRANSPORT_OPEN, T.VersionedHostJamPeerTransportOpenRequest.enc({ tag: "V1", value: { conn, kind: 0 } }), openCodec);
    if (!open.success) throw new Error("open failed");
    transport.peerClose();
    await tick();
    await tick();
    const events = await call(session, JAM_PEER_TRANSPORT_EVENTS, T.VersionedHostJamPeerTransportEventsRequest.enc({ tag: "V1" }), eventsCodec);
    expect(events).toEqual({ success: true, value: { tag: "V1", value: { events: [{ tag: "ConnClosed", value: { conn } }] } } });
    const send = await call(session, JAM_PEER_TRANSPORT_SEND, T.VersionedHostJamPeerTransportSendRequest.enc({ tag: "V1", value: { stream: open.value.value.stream, message: "0x00", fin: false } }), sendCodec);
    expect(send).toEqual({ success: false, value: { tag: "Domain", value: { tag: "V1", value: "Closed" } } });
    session.close();
    const response = decodeWireMessage(await session.handleFrame(frame(JAM_PEER_TRANSPORT_DIAL, dialRequest())));
    expect(response.isOk() && S.Result(S._void, S.CallError(S._void)).dec(response.value.payload.value)).toEqual({ success: false, value: { tag: "Denied" } });
  });
});
