import { describe, expect, test } from "bun:test";
import * as S from "./scale.js";
import * as T from "./generated/types.js";
import { TRUAPI_CODEC_VERSION } from "./generated/client.js";
import {
  PEER_TRANSPORT_CLOSE,
  PEER_TRANSPORT_DIAL,
  PEER_TRANSPORT_EVENTS,
  PEER_TRANSPORT_OPEN,
  PEER_TRANSPORT_RECV,
  PEER_TRANSPORT_SEND,
  SYSTEM_HANDSHAKE,
} from "./generated/wire-table.js";
import { decodeWireMessage, encodeWireMessage, MESSAGE_TYPE_REQUEST, type MethodIds } from "./transport.js";
import {
  createPeerTransportSession,
  frameTraitId,
  peerUrl,
  PEER_TRANSPORT_MAX_MESSAGE_BYTES,
  type PeerTransportSession,
  type WebTransportBidirectionalStreamLike,
  type WebTransportLike,
} from "./peer-transport.js";

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
  /** Simulate the peer opening a stream toward us. */
  peerOpen(): FakeStream;
  peerClose(): void;
}

function fakeStream(): FakeStream {
  const sent: Uint8Array[] = [];
  let peerController!: ReadableStreamDefaultController<Uint8Array>;
  const readable = new ReadableStream<Uint8Array>({ start: (c) => (peerController = c) });
  const writable = new WritableStream<Uint8Array>({ write: (chunk) => void sent.push(chunk) });
  return {
    local: { readable, writable },
    sent,
    peerWrite: (bytes) => peerController.enqueue(bytes),
    peerFin: () => peerController.close(),
  };
}

function fakeTransport(url: string, hashes: Uint8Array[], failReady = false): FakeTransport {
  let incoming!: ReadableStreamDefaultController<WebTransportBidirectionalStreamLike>;
  const closed = Promise.withResolvers<void>();
  const streams: FakeStream[] = [];
  return {
    url,
    hashes,
    streams,
    ready: failReady ? Promise.reject(new Error("refused")) : Promise.resolve(),
    closed: closed.promise,
    incomingBidirectionalStreams: new ReadableStream({ start: (c) => (incoming = c) }),
    async createBidirectionalStream() {
      const stream = fakeStream();
      streams.push(stream);
      return stream.local;
    },
    close: () => closed.resolve(),
    peerOpen() {
      const stream = fakeStream();
      streams.push(stream);
      incoming.enqueue(stream.local);
      return stream;
    },
    peerClose: () => closed.resolve(),
  };
}

let requestCounter = 0;
function frame(ids: MethodIds, value: Uint8Array): Uint8Array {
  const encoded = encodeWireMessage({
    requestId: `t${requestCounter++}`,
    payload: { traitId: ids.trait, methodId: ids.method, messageType: MESSAGE_TYPE_REQUEST, value },
  });
  if (encoded.isErr()) throw encoded.error;
  return encoded.value;
}

async function call<V>(session: PeerTransportSession, ids: MethodIds, request: Uint8Array, codec: S.Codec<V>): Promise<V> {
  const response = decodeWireMessage(await session.handleFrame(frame(ids, request)));
  if (response.isErr()) throw response.error;
  return codec.dec(response.value.payload.value);
}

const dialCodec = S.Result(T.VersionedHostPeerTransportDialResponse, S.CallError(T.VersionedHostPeerTransportDialError));
const openCodec = S.Result(T.VersionedHostPeerTransportOpenResponse, S.CallError(T.VersionedHostPeerTransportOpenError));
const sendCodec = S.Result(T.VersionedHostPeerTransportSendResponse, S.CallError(T.VersionedHostPeerTransportSendError));
const recvCodec = S.Result(T.VersionedHostPeerTransportRecvResponse, S.CallError(T.VersionedHostPeerTransportRecvError));
const closeCodec = S.Result(T.VersionedHostPeerTransportCloseResponse, S.CallError(T.VersionedHostPeerTransportCloseError));
const eventsCodec = S.Result(T.VersionedHostPeerTransportEventsResponse, S.CallError(T.VersionedHostPeerTransportEventsError));
const handshakeCodec = S.Result(T.VersionedHostHandshakeResponse, S.CallError(T.VersionedHostHandshakeError));

function dialRequest(overrides: Partial<T.HostPeerTransportDialRequest> = {}): Uint8Array {
  return T.VersionedHostPeerTransportDialRequest.enc({
    tag: "V1",
    value: { genesis: GENESIS, ip: LOOPBACK, port: 43000, ed25519: `0x${"11".repeat(32)}`, p256: P256, ...overrides },
  });
}

async function negotiated(options: { failReady?: boolean } = {}): Promise<{ session: PeerTransportSession; transports: FakeTransport[] }> {
  const transports: FakeTransport[] = [];
  const session = createPeerTransportSession({
    genesis: GENESIS,
    now: () => 1_790_380_800,
    connect: (url, hashes) => {
      const transport = fakeTransport(url, hashes, options.failReady);
      transports.push(transport);
      return transport;
    },
  });
  const handshake = await call(session, SYSTEM_HANDSHAKE, T.VersionedHostHandshakeRequest.enc({ tag: "V1", value: { codecVersion: TRUAPI_CODEC_VERSION } }), handshakeCodec);
  expect(handshake.success).toBe(true);
  return { session, transports };
}

async function dialed(): Promise<{ session: PeerTransportSession; transport: FakeTransport; conn: number }> {
  const { session, transports } = await negotiated();
  const dial = await call(session, PEER_TRANSPORT_DIAL, dialRequest(), dialCodec);
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

describe("grant", () => {
  test("dial before the handshake is NotGranted", async () => {
    const session = createPeerTransportSession({ genesis: GENESIS, connect: () => fakeTransport("", []) });
    const dial = await call(session, PEER_TRANSPORT_DIAL, dialRequest(), dialCodec);
    expect(dial).toEqual({ success: false, value: { tag: "Domain", value: { tag: "V1", value: "NotGranted" } } });
  });

  test("dial for another genesis is NotGranted; without p256 it is Unreachable", async () => {
    const { session, transports } = await negotiated();
    const other = await call(session, PEER_TRANSPORT_DIAL, dialRequest({ genesis: `0x${"ab".repeat(32)}` }), dialCodec);
    expect(other).toEqual({ success: false, value: { tag: "Domain", value: { tag: "V1", value: "NotGranted" } } });
    const quicOnly = await call(session, PEER_TRANSPORT_DIAL, dialRequest({ p256: undefined }), dialCodec);
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
  test("connects to the peer URL with the three period certificate hashes", async () => {
    const { transport, conn } = await dialed();
    expect(conn).toBe(1);
    expect(transport.url).toBe("https://127.0.0.1:43000");
    expect(transport.hashes.map((h) => S.bytesToHex(h))).toEqual([
      "0xccf30196b29007b42fca6f406363ce17781bab0f011bac47dcbe307e0e6a316d",
      "0xeb09b6b027f5953cb8ca2e8f296e21052c3423370876e67f1634180ddf99f1ec",
      "0x8bdfa3a2b7822822f5da33fadfa118d39d05b0a6b1086fc82a62a2209f6fae27",
    ]);
  });

  test("a rejected handshake is Refused and holds no connection slot", async () => {
    const { session } = await negotiated({ failReady: true });
    const dial = await call(session, PEER_TRANSPORT_DIAL, dialRequest(), dialCodec);
    expect(dial).toEqual({ success: false, value: { tag: "Domain", value: { tag: "V1", value: "Refused" } } });
    const close = await call(session, PEER_TRANSPORT_CLOSE, T.VersionedHostPeerTransportCloseRequest.enc({ tag: "V1", value: { conn: 1 } }), closeCodec);
    expect(close.success).toBe(false);
  });

  test("the ninth connection hits Limit", async () => {
    const { session } = await negotiated();
    for (let i = 0; i < 8; i++) {
      expect((await call(session, PEER_TRANSPORT_DIAL, dialRequest(), dialCodec)).success).toBe(true);
    }
    const ninth = await call(session, PEER_TRANSPORT_DIAL, dialRequest(), dialCodec);
    expect(ninth).toEqual({ success: false, value: { tag: "Domain", value: { tag: "V1", value: "Limit" } } });
  });
});

describe("streams", () => {
  test("open sends the kind byte; send frames with a u32-LE prefix; recv unframes", async () => {
    const { session, transport, conn } = await dialed();
    const open = await call(session, PEER_TRANSPORT_OPEN, T.VersionedHostPeerTransportOpenRequest.enc({ tag: "V1", value: { conn, kind: 128 } }), openCodec);
    if (!open.success) throw new Error("open failed");
    const stream = open.value.value.stream;
    const send = await call(session, PEER_TRANSPORT_SEND, T.VersionedHostPeerTransportSendRequest.enc({ tag: "V1", value: { stream, message: "0x0102", fin: true } }), sendCodec);
    expect(send.success).toBe(true);
    const wire = transport.streams[0]!;
    expect(wire.sent.map((c) => S.bytesToHex(c))).toEqual(["0x80", "0x020000000102"]);

    // Peer replies with two messages split across arbitrary chunk boundaries, then FIN.
    wire.peerWrite(new Uint8Array([3, 0, 0, 0, 0xaa]));
    wire.peerWrite(new Uint8Array([0xbb, 0xcc, 1, 0, 0]));
    await tick();
    const early = await call(session, PEER_TRANSPORT_RECV, T.VersionedHostPeerTransportRecvRequest.enc({ tag: "V1", value: { stream, max: 1 << 20 } }), recvCodec);
    expect(early).toEqual({ success: true, value: { tag: "V1", value: { message: "0xaabbcc", fin: false, reset: false } } });
    wire.peerWrite(new Uint8Array([0, 0xdd]));
    wire.peerFin();
    await tick();
    await tick();
    const second = await call(session, PEER_TRANSPORT_RECV, T.VersionedHostPeerTransportRecvRequest.enc({ tag: "V1", value: { stream, max: 1 << 20 } }), recvCodec);
    expect(second).toEqual({ success: true, value: { tag: "V1", value: { message: "0xdd", fin: true, reset: false } } });
    const drained = await call(session, PEER_TRANSPORT_RECV, T.VersionedHostPeerTransportRecvRequest.enc({ tag: "V1", value: { stream, max: 1 << 20 } }), recvCodec);
    expect(drained).toEqual({ success: true, value: { tag: "V1", value: { message: undefined, fin: true, reset: false } } });
    const consumed = await call(session, PEER_TRANSPORT_RECV, T.VersionedHostPeerTransportRecvRequest.enc({ tag: "V1", value: { stream, max: 1 << 20 } }), recvCodec);
    expect(consumed).toEqual({ success: false, value: { tag: "Domain", value: { tag: "V1", value: "Closed" } } });
    const events = await call(session, PEER_TRANSPORT_EVENTS, T.VersionedHostPeerTransportEventsRequest.enc({ tag: "V1" }), eventsCodec);
    expect(events).toEqual({ success: true, value: { tag: "V1", value: { events: [{ tag: "StreamFin", value: { stream } }] } } });
  });

  test("oversized send is TooLarge and a message above the caller's max resets the stream", async () => {
    const { session, transport, conn } = await dialed();
    const open = await call(session, PEER_TRANSPORT_OPEN, T.VersionedHostPeerTransportOpenRequest.enc({ tag: "V1", value: { conn, kind: 0 } }), openCodec);
    if (!open.success) throw new Error("open failed");
    const stream = open.value.value.stream;
    const big = `0x${"00".repeat(PEER_TRANSPORT_MAX_MESSAGE_BYTES + 1)}` as const;
    const send = await call(session, PEER_TRANSPORT_SEND, T.VersionedHostPeerTransportSendRequest.enc({ tag: "V1", value: { stream, message: big, fin: false } }), sendCodec);
    expect(send).toEqual({ success: false, value: { tag: "Domain", value: { tag: "V1", value: "TooLarge" } } });
    transport.streams[0]!.peerWrite(new Uint8Array([2, 0, 0, 0, 1, 2]));
    await tick();
    const recv = await call(session, PEER_TRANSPORT_RECV, T.VersionedHostPeerTransportRecvRequest.enc({ tag: "V1", value: { stream, max: 1 } }), recvCodec);
    expect(recv).toEqual({ success: true, value: { tag: "V1", value: { message: undefined, fin: false, reset: true } } });
  });

  test("peer-opened streams surface as Accepted with their kind byte", async () => {
    const { session, transport, conn } = await dialed();
    const incoming = transport.peerOpen();
    incoming.peerWrite(new Uint8Array([0, 2, 0, 0, 0, 9, 9]));
    await tick();
    await tick();
    const events = await call(session, PEER_TRANSPORT_EVENTS, T.VersionedHostPeerTransportEventsRequest.enc({ tag: "V1" }), eventsCodec);
    expect(events).toEqual({ success: true, value: { tag: "V1", value: { events: [{ tag: "Accepted", value: { conn, stream: 1, kind: 0 } }] } } });
    const recv = await call(session, PEER_TRANSPORT_RECV, T.VersionedHostPeerTransportRecvRequest.enc({ tag: "V1", value: { stream: 1, max: 1 << 20 } }), recvCodec);
    expect(recv).toEqual({ success: true, value: { tag: "V1", value: { message: "0x0909", fin: false, reset: false } } });
  });

  test("a peer close reports ConnClosed and invalidates streams; session close denies everything", async () => {
    const { session, transport, conn } = await dialed();
    const open = await call(session, PEER_TRANSPORT_OPEN, T.VersionedHostPeerTransportOpenRequest.enc({ tag: "V1", value: { conn, kind: 0 } }), openCodec);
    if (!open.success) throw new Error("open failed");
    transport.peerClose();
    await tick();
    await tick();
    const events = await call(session, PEER_TRANSPORT_EVENTS, T.VersionedHostPeerTransportEventsRequest.enc({ tag: "V1" }), eventsCodec);
    expect(events).toEqual({ success: true, value: { tag: "V1", value: { events: [{ tag: "ConnClosed", value: { conn } }] } } });
    const send = await call(session, PEER_TRANSPORT_SEND, T.VersionedHostPeerTransportSendRequest.enc({ tag: "V1", value: { stream: open.value.value.stream, message: "0x00", fin: false } }), sendCodec);
    expect(send).toEqual({ success: false, value: { tag: "Domain", value: { tag: "V1", value: "Closed" } } });
    session.close();
    const response = decodeWireMessage(await session.handleFrame(frame(PEER_TRANSPORT_DIAL, dialRequest())));
    expect(response.isOk() && S.Result(S._void, S.CallError(S._void)).dec(response.value.payload.value)).toEqual({ success: false, value: { tag: "Denied" } });
  });
});
