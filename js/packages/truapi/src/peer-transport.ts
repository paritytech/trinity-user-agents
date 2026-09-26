import * as S from "./scale.js";
import * as T from "./generated/types.js";
import { TRUAPI_CODEC_VERSION } from "./generated/client.js";
import {
  PEER_TRANSPORT_CLOSE,
  PEER_TRANSPORT_DIAL,
  PEER_TRANSPORT_EVENTS,
  PEER_TRANSPORT_OPEN,
  PEER_TRANSPORT_RECV,
  PEER_TRANSPORT_RESET,
  PEER_TRANSPORT_SEND,
  SYSTEM_HANDSHAKE,
} from "./generated/wire-table.js";
import {
  decodeWireMessage,
  encodeWireMessage,
  MESSAGE_TYPE_CANCEL,
  MESSAGE_TYPE_REQUEST,
  MESSAGE_TYPE_RESPONSE,
  type MethodIds,
  type ProtocolMessage,
} from "./transport.js";
import { webTransportCertificateHashes } from "./peer-transport-cert.js";

/** Caps mirrored from `truapi::v01::peer_transport`. */
export const PEER_TRANSPORT_MAX_CONNECTIONS = 8;
export const PEER_TRANSPORT_MAX_STREAMS_PER_CONNECTION = 16;
export const PEER_TRANSPORT_MAX_MESSAGE_BYTES = 1 << 20;
export const PEER_TRANSPORT_MAX_BUFFERED_BYTES_PER_CONNECTION = 4 << 20;
/** Largest request frame: a `send` of a maximal message plus SCALE and wire overhead. */
export const PEER_TRANSPORT_MAX_FRAME_BYTES = PEER_TRANSPORT_MAX_MESSAGE_BYTES + 4096;
const MAX_PENDING_EVENTS = 1024;
const DIAL_TIMEOUT_MS = 10_000;
const textEncoder = new TextEncoder();

const handshakeResult = S.Result(T.VersionedHostHandshakeResponse, S.CallError(T.VersionedHostHandshakeError));
const frameworkResult = S.Result(S._void, S.CallError(S._void));
const dialResult = S.Result(T.VersionedHostPeerTransportDialResponse, S.CallError(T.VersionedHostPeerTransportDialError));
const openResult = S.Result(T.VersionedHostPeerTransportOpenResponse, S.CallError(T.VersionedHostPeerTransportOpenError));
const sendResult = S.Result(T.VersionedHostPeerTransportSendResponse, S.CallError(T.VersionedHostPeerTransportSendError));
const recvResult = S.Result(T.VersionedHostPeerTransportRecvResponse, S.CallError(T.VersionedHostPeerTransportRecvError));
const resetResult = S.Result(T.VersionedHostPeerTransportResetResponse, S.CallError(T.VersionedHostPeerTransportResetError));
const closeResult = S.Result(T.VersionedHostPeerTransportCloseResponse, S.CallError(T.VersionedHostPeerTransportCloseError));
const eventsResult = S.Result(T.VersionedHostPeerTransportEventsResponse, S.CallError(T.VersionedHostPeerTransportEventsError));

/** Minimal WebTransport surface the session needs; lets tests inject a fake. */
export interface WebTransportLike {
  readonly ready: Promise<unknown>;
  readonly closed: Promise<unknown>;
  readonly incomingBidirectionalStreams: ReadableStream<WebTransportBidirectionalStreamLike>;
  createBidirectionalStream(): Promise<WebTransportBidirectionalStreamLike>;
  close(): void;
}

export interface WebTransportBidirectionalStreamLike {
  readonly readable: ReadableStream<Uint8Array>;
  readonly writable: WritableStream<Uint8Array>;
}

/** A host-owned grant for one JAM genesis; the guest can neither create nor widen it. */
export interface PeerTransportGrant {
  /** `0x`-prefixed lower-case 32-byte genesis header hash. */
  genesis: string;
}

export interface PeerTransportOptions extends PeerTransportGrant {
  /** Host transport injection; defaults to the browser `WebTransport` constructor. */
  connect?: (url: string, certificateHashes: Uint8Array[]) => WebTransportLike;
  /** Unix seconds used to select certificate validity periods; defaults to the wall clock. */
  now?: () => number;
}

/** Execution-local peer endpoint. It provides no account or signing authority. */
export interface PeerTransportSession {
  /** Handle one request frame; CANCEL frames return zero bytes. */
  handleFrame(frame: Uint8Array): Promise<Uint8Array>;
  /** Revoke the grant and close every connection on stop or replacement. */
  close(): void;
}

/** Validate and normalize the manifest `capabilities.network.jam.genesis` value. */
export function validatePeerTransportGenesis(genesis: string): string {
  const hex = genesis.startsWith("0x") ? genesis.slice(2) : genesis;
  if (!/^[0-9a-f]{64}$/.test(hex)) {
    throw new Error("JAM genesis must be a 32-byte lower-case hex header hash");
  }
  return `0x${hex}`;
}

/** Trait id of a request frame, or `undefined` when it does not decode. */
export function frameTraitId(frame: Uint8Array): number | undefined {
  const decoded = decodeWireMessage(frame);
  return decoded.isOk() ? decoded.value.payload.traitId : undefined;
}

function exact<V>(codec: S.Codec<V>, bytes: Uint8Array): V {
  const value = codec.dec(bytes);
  const canonical = codec.enc(value);
  if (canonical.length !== bytes.length || canonical.some((byte, index) => byte !== bytes[index])) {
    throw new Error("Noncanonical or trailing SCALE bytes");
  }
  return value;
}

function decodeFrame(bytes: Uint8Array): ProtocolMessage {
  if (!(bytes instanceof Uint8Array) || bytes.length > PEER_TRANSPORT_MAX_FRAME_BYTES) {
    throw new Error("Invalid or oversized peer-transport frame");
  }
  const decoded = decodeWireMessage(bytes);
  if (decoded.isErr()) throw decoded.error;
  const message = decoded.value;
  if (textEncoder.encode(message.requestId).length > 64) throw new Error("Oversized request id");
  const encoded = encodeWireMessage(message);
  if (encoded.isErr()) throw encoded.error;
  if (encoded.value.length !== bytes.length || encoded.value.some((byte, index) => byte !== bytes[index])) {
    throw new Error("Noncanonical request frame");
  }
  return message;
}

function hasIds(message: ProtocolMessage, ids: MethodIds): boolean {
  return message.payload.traitId === ids.trait && message.payload.methodId === ids.method;
}

function reply(request: ProtocolMessage, value: Uint8Array): Uint8Array {
  const encoded = encodeWireMessage({
    requestId: request.requestId,
    payload: { ...request.payload, messageType: MESSAGE_TYPE_RESPONSE, value },
  });
  if (encoded.isErr()) throw encoded.error;
  return encoded.value;
}

function ok<V, E>(codec: S.Codec<S.Result<V, E>>, value: NoInfer<V>): Uint8Array {
  return codec.enc({ success: true, value });
}

function domain<V, E>(codec: S.Codec<S.Result<V, S.CallErrorValue<{ tag: "V1"; value: E }>>>, error: E): Uint8Array {
  return codec.enc({ success: false, value: { tag: "Domain", value: { tag: "V1", value: error } } });
}

/** `https://` authority for a 16-byte IPv6 or v4-mapped address. */
export function peerUrl(ip: Uint8Array, port: number): string {
  if (ip.length !== 16) throw new Error("peer ip must be 16 bytes");
  const v4Mapped = ip.subarray(0, 10).every((byte) => byte === 0) && ip[10] === 0xff && ip[11] === 0xff;
  if (v4Mapped) return `https://${ip[12]}.${ip[13]}.${ip[14]}.${ip[15]}:${port}`;
  const groups: string[] = [];
  for (let i = 0; i < 16; i += 2) groups.push(((ip[i]! << 8) | ip[i + 1]!).toString(16));
  return `https://[${groups.join(":")}]:${port}`;
}

interface PeerStream {
  id: number;
  conn: PeerConnection;
  writer: WritableStreamDefaultWriter<Uint8Array>;
  reader: ReadableStreamDefaultReader<Uint8Array>;
  /** Unparsed receive bytes. */
  rx: Uint8Array;
  /** Complete messages not yet delivered by `recv`. */
  messages: Uint8Array[];
  fin: boolean;
  reset: boolean;
  /** `recv` reported `fin` with an empty queue; further reads are `Closed`. */
  rxConsumed: boolean;
  txClosed: boolean;
  /** Bytes of frames handed to the writer that have not been accepted yet. */
  txPending: number;
}

interface PeerConnection {
  id: number;
  transport: WebTransportLike;
  streams: Map<number, PeerStream>;
  closed: boolean;
}

/**
 * Create the browser PeerTransport endpoint for one execution. The host must
 * have checked the manifest grant before calling this constructor and must
 * fence late replies against execution stop or replacement.
 */
export function createPeerTransportSession(options: PeerTransportOptions): PeerTransportSession {
  const genesis = validatePeerTransportGenesis(options.genesis);
  const connect =
    options.connect ??
    ((url, hashes): WebTransportLike =>
      new WebTransport(url, {
        serverCertificateHashes: hashes.map((value) => ({ algorithm: "sha-256", value: value as Uint8Array<ArrayBuffer> })),
      }) as unknown as WebTransportLike);
  const now = options.now ?? ((): number => Math.floor(Date.now() / 1000));
  let closed = false;
  let negotiated = false;
  let nextConn = 1;
  let nextStream = 1;
  const connections = new Map<number, PeerConnection>();
  const streams = new Map<number, PeerStream>();
  const events: T.PeerTransportEvent[] = [];

  const pushEvent = (event: T.PeerTransportEvent): void => {
    if (events.length < MAX_PENDING_EVENTS) events.push(event);
  };

  const dropStream = (stream: PeerStream, abort: boolean): void => {
    streams.delete(stream.id);
    stream.conn.streams.delete(stream.id);
    if (abort) {
      void stream.writer.abort().catch(() => undefined);
      void stream.reader.cancel().catch(() => undefined);
    }
  };

  const dropConnection = (conn: PeerConnection): void => {
    if (conn.closed) return;
    conn.closed = true;
    connections.delete(conn.id);
    for (const stream of [...conn.streams.values()]) dropStream(stream, true);
    try {
      conn.transport.close();
    } catch {
      // Already closed by the peer.
    }
    pushEvent({ tag: "ConnClosed", value: { conn: conn.id } });
  };

  /** Parse complete `u32`-LE framed messages out of `stream.rx`. */
  const unframe = (stream: PeerStream): void => {
    while (stream.rx.length >= 4) {
      const view = new DataView(stream.rx.buffer, stream.rx.byteOffset, stream.rx.byteLength);
      const length = view.getUint32(0, true);
      if (length > PEER_TRANSPORT_MAX_MESSAGE_BYTES) {
        stream.reset = true;
        void stream.writer.abort().catch(() => undefined);
        void stream.reader.cancel().catch(() => undefined);
        stream.rx = new Uint8Array();
        return;
      }
      if (stream.rx.length < 4 + length) return;
      stream.messages.push(stream.rx.slice(4, 4 + length));
      stream.rx = stream.rx.slice(4 + length);
    }
  };

  const pump = async (stream: PeerStream, initial: Uint8Array): Promise<void> => {
    stream.rx = initial;
    unframe(stream);
    try {
      while (!stream.reset) {
        const { value, done } = await stream.reader.read();
        if (done) break;
        const next = new Uint8Array(stream.rx.length + value.length);
        next.set(stream.rx);
        next.set(value, stream.rx.length);
        stream.rx = next;
        unframe(stream);
      }
      if (!stream.reset) {
        stream.fin = true;
        if (stream.rx.length !== 0) stream.reset = true;
        if (streams.has(stream.id)) pushEvent({ tag: "StreamFin", value: { stream: stream.id } });
      }
    } catch {
      stream.reset = true;
    }
  };

  const register = (conn: PeerConnection, bidi: WebTransportBidirectionalStreamLike, initial: Uint8Array): PeerStream => {
    const stream: PeerStream = {
      id: nextStream++,
      conn,
      writer: bidi.writable.getWriter(),
      reader: bidi.readable.getReader(),
      rx: new Uint8Array(),
      messages: [],
      fin: false,
      reset: false,
      rxConsumed: false,
      txClosed: false,
      txPending: 0,
    };
    streams.set(stream.id, stream);
    conn.streams.set(stream.id, stream);
    void pump(stream, initial);
    return stream;
  };

  const acceptLoop = async (conn: PeerConnection): Promise<void> => {
    const incoming = conn.transport.incomingBidirectionalStreams.getReader();
    try {
      while (!conn.closed) {
        const { value: bidi, done } = await incoming.read();
        if (done || conn.closed) break;
        if (conn.streams.size >= PEER_TRANSPORT_MAX_STREAMS_PER_CONNECTION) {
          void bidi.writable.abort().catch(() => undefined);
          void bidi.readable.cancel().catch(() => undefined);
          continue;
        }
        // The peer's first byte is the stream kind; anything after it is message data.
        const reader = bidi.readable.getReader();
        const first = await reader.read();
        reader.releaseLock();
        if (first.done || first.value.length === 0 || conn.closed) {
          void bidi.writable.abort().catch(() => undefined);
          continue;
        }
        const stream = register(conn, bidi, first.value.subarray(1));
        pushEvent({ tag: "Accepted", value: { conn: conn.id, stream: stream.id, kind: first.value[0]! } });
      }
    } catch {
      // The connection is closing; `closed` handling reports it.
    }
  };

  const dial = async (request: T.HostPeerTransportDialRequest): Promise<Uint8Array> => {
    if (request.genesis !== genesis) return domain(dialResult, "NotGranted");
    if (connections.size >= PEER_TRANSPORT_MAX_CONNECTIONS) return domain(dialResult, "Limit");
    // Browsers only expose WebTransport; JAMNP-S QUIC needs the P-256 identity.
    if (request.p256 === undefined) return domain(dialResult, "Unreachable");
    const p256 = S.hexToBytes(request.p256);
    if (p256.length !== 33 || (p256[0] !== 2 && p256[0] !== 3)) return domain(dialResult, "Refused");
    let transport: WebTransportLike;
    try {
      transport = connect(peerUrl(S.hexToBytes(request.ip), request.port), webTransportCertificateHashes(p256, now()));
    } catch {
      return domain(dialResult, "Unreachable");
    }
    const conn: PeerConnection = { id: nextConn++, transport, streams: new Map(), closed: false };
    connections.set(conn.id, conn);
    // Executor form: this package's lib target predates Promise.withResolvers.
    let timer: number | undefined;
    try {
      await Promise.race([
        transport.ready,
        new Promise<never>((_resolve, reject) => {
          timer = setTimeout(() => reject(new Error("timeout")), DIAL_TIMEOUT_MS) as unknown as number;
        }),
      ]);
    } catch (error) {
      connections.delete(conn.id);
      conn.closed = true;
      try {
        transport.close();
      } catch {
        // Never opened.
      }
      return domain(dialResult, error instanceof Error && error.message === "timeout" ? "Unreachable" : "Refused");
    } finally {
      clearTimeout(timer);
    }
    if (closed) {
      dropConnection(conn);
      return frameworkResult.enc({ success: false, value: { tag: "Denied" } });
    }
    void transport.closed.then(
      () => dropConnection(conn),
      () => dropConnection(conn),
    );
    void acceptLoop(conn);
    return ok(dialResult, { tag: "V1", value: { conn: conn.id } });
  };

  const open = async (request: T.HostPeerTransportOpenRequest): Promise<Uint8Array> => {
    const conn = connections.get(request.conn);
    if (conn === undefined || conn.closed) return domain(openResult, "Closed");
    if (conn.streams.size >= PEER_TRANSPORT_MAX_STREAMS_PER_CONNECTION) return domain(openResult, "Limit");
    try {
      const bidi = await conn.transport.createBidirectionalStream();
      const stream = register(conn, bidi, new Uint8Array());
      await stream.writer.write(new Uint8Array([request.kind]));
      return ok(openResult, { tag: "V1", value: { stream: stream.id } });
    } catch {
      return domain(openResult, "Closed");
    }
  };

  const send = async (request: T.HostPeerTransportSendRequest): Promise<Uint8Array> => {
    const stream = streams.get(request.stream);
    if (stream === undefined || stream.txClosed || stream.reset || stream.conn.closed) return domain(sendResult, "Closed");
    const message = S.hexToBytes(request.message);
    if (message.length > PEER_TRANSPORT_MAX_MESSAGE_BYTES) return domain(sendResult, "TooLarge");
    let pending = 0;
    for (const other of stream.conn.streams.values()) pending += other.txPending;
    if (pending + message.length + 4 > PEER_TRANSPORT_MAX_BUFFERED_BYTES_PER_CONNECTION) return domain(sendResult, "Limit");
    const frame = new Uint8Array(4 + message.length);
    new DataView(frame.buffer).setUint32(0, message.length, true);
    frame.set(message, 4);
    stream.txPending += frame.length;
    try {
      await stream.writer.write(frame);
      if (request.fin) {
        stream.txClosed = true;
        await stream.writer.close();
      }
      return ok(sendResult, { tag: "V1" });
    } catch {
      stream.txClosed = true;
      return domain(sendResult, "Closed");
    } finally {
      stream.txPending -= frame.length;
    }
  };

  const recv = (request: T.HostPeerTransportRecvRequest): Uint8Array => {
    const stream = streams.get(request.stream);
    if (stream === undefined || stream.rxConsumed) return domain(recvResult, "Closed");
    const next = stream.messages[0];
    if (next !== undefined && next.length > request.max) {
      // The guest cannot take this message; treat it as a protocol violation.
      stream.messages.length = 0;
      stream.reset = true;
      void stream.writer.abort().catch(() => undefined);
      void stream.reader.cancel().catch(() => undefined);
    }
    let message: S.HexString | undefined;
    if (!stream.reset && next !== undefined) {
      stream.messages.shift();
      message = S.bytesToHex(next);
    }
    const drained = stream.messages.length === 0;
    const fin = stream.fin && drained;
    const reset = stream.reset;
    if (message === undefined && (fin || reset)) {
      stream.rxConsumed = true;
      if (stream.txClosed || reset) dropStream(stream, reset);
    }
    return ok(recvResult, { tag: "V1", value: { message, fin, reset } });
  };

  const reset = (request: T.HostPeerTransportResetRequest): Uint8Array => {
    const stream = streams.get(request.stream);
    if (stream === undefined) return domain(resetResult, "Closed");
    dropStream(stream, true);
    return ok(resetResult, { tag: "V1" });
  };

  const close = (request: T.HostPeerTransportCloseRequest): Uint8Array => {
    const conn = connections.get(request.conn);
    if (conn === undefined || conn.closed) return domain(closeResult, "Closed");
    dropConnection(conn);
    return ok(closeResult, { tag: "V1" });
  };

  return {
    async handleFrame(bytes) {
      const request = decodeFrame(bytes);
      if (request.payload.messageType === MESSAGE_TYPE_CANCEL) {
        if (request.payload.traitId !== PEER_TRANSPORT_DIAL.trait || request.payload.value.length !== 0) {
          throw new Error("Invalid cancellation frame");
        }
        // Every method answers within one host tick; nothing is cancellable.
        return new Uint8Array();
      }
      if (request.payload.messageType !== MESSAGE_TYPE_REQUEST) {
        throw new Error("Invalid request frame");
      }
      if (closed) return reply(request, frameworkResult.enc({ success: false, value: { tag: "Denied" } }));
      if (hasIds(request, SYSTEM_HANDSHAKE)) {
        let handshake: T.VersionedHostHandshakeRequest;
        try {
          handshake = exact(T.VersionedHostHandshakeRequest, request.payload.value);
        } catch {
          return reply(request, frameworkResult.enc({ success: false, value: { tag: "MalformedFrame", value: { reason: "invalid handshake" } } }));
        }
        if (handshake.value.codecVersion !== TRUAPI_CODEC_VERSION) {
          return reply(request, handshakeResult.enc({ success: false, value: { tag: "Domain", value: { tag: "V1", value: { tag: "UnsupportedProtocolVersion" } } } }));
        }
        negotiated = true;
        return reply(request, handshakeResult.enc({ success: true, value: { tag: "V1" } }));
      }
      if (request.payload.traitId !== PEER_TRANSPORT_DIAL.trait) {
        return reply(request, frameworkResult.enc({ success: false, value: { tag: "Denied" } }));
      }
      const malformed = (): Uint8Array =>
        reply(request, frameworkResult.enc({ success: false, value: { tag: "MalformedFrame", value: { reason: "invalid peer-transport request" } } }));
      try {
        if (hasIds(request, PEER_TRANSPORT_DIAL)) {
          const value = exact(T.VersionedHostPeerTransportDialRequest, request.payload.value).value;
          return reply(request, negotiated ? await dial(value) : domain(dialResult, "NotGranted"));
        }
        if (hasIds(request, PEER_TRANSPORT_OPEN)) {
          const value = exact(T.VersionedHostPeerTransportOpenRequest, request.payload.value).value;
          return reply(request, negotiated ? await open(value) : domain(openResult, "NotGranted"));
        }
        if (hasIds(request, PEER_TRANSPORT_SEND)) {
          const value = exact(T.VersionedHostPeerTransportSendRequest, request.payload.value).value;
          return reply(request, negotiated ? await send(value) : domain(sendResult, "Closed"));
        }
        if (hasIds(request, PEER_TRANSPORT_RECV)) {
          const value = exact(T.VersionedHostPeerTransportRecvRequest, request.payload.value).value;
          return reply(request, negotiated ? recv(value) : domain(recvResult, "Closed"));
        }
        if (hasIds(request, PEER_TRANSPORT_RESET)) {
          const value = exact(T.VersionedHostPeerTransportResetRequest, request.payload.value).value;
          return reply(request, negotiated ? reset(value) : domain(resetResult, "Closed"));
        }
        if (hasIds(request, PEER_TRANSPORT_CLOSE)) {
          const value = exact(T.VersionedHostPeerTransportCloseRequest, request.payload.value).value;
          return reply(request, negotiated ? close(value) : domain(closeResult, "Closed"));
        }
        if (hasIds(request, PEER_TRANSPORT_EVENTS)) {
          exact(T.VersionedHostPeerTransportEventsRequest, request.payload.value);
          if (!negotiated) return reply(request, domain(eventsResult, "NotGranted"));
          return reply(request, ok(eventsResult, { tag: "V1", value: { events: events.splice(0, events.length) } }));
        }
      } catch {
        return malformed();
      }
      return reply(request, frameworkResult.enc({ success: false, value: { tag: "Unsupported" } }));
    },
    close() {
      closed = true;
      for (const conn of [...connections.values()]) dropConnection(conn);
      events.length = 0;
    },
  };
}
