import * as S from "./scale.js";
import * as T from "./generated/types.js";
import { TRUAPI_CODEC_VERSION } from "./generated/client.js";
import {
  JAM_PEER_TRANSPORT_CLOSE,
  JAM_PEER_TRANSPORT_DIAL,
  JAM_PEER_TRANSPORT_EVENTS,
  JAM_PEER_TRANSPORT_OPEN,
  JAM_PEER_TRANSPORT_RECV,
  JAM_PEER_TRANSPORT_RESET,
  JAM_PEER_TRANSPORT_SEND,
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
import { webTransportCertificateHashes } from "./jam-peer-transport-cert.js";

/** Caps mirrored from `truapi::v01::jam_peer_transport`. */
export const JAM_PEER_TRANSPORT_MAX_CONNECTIONS = 8;
export const JAM_PEER_TRANSPORT_MAX_STREAMS_PER_CONNECTION = 16;
export const JAM_PEER_TRANSPORT_MAX_MESSAGE_BYTES = 1 << 20;
export const JAM_PEER_TRANSPORT_MAX_BUFFERED_BYTES_PER_CONNECTION = 4 << 20;
/** Largest request frame: a `send` of a maximal message plus SCALE and wire overhead. */
export const JAM_PEER_TRANSPORT_MAX_FRAME_BYTES = JAM_PEER_TRANSPORT_MAX_MESSAGE_BYTES + 4096;
const MAX_PENDING_EVENTS = 1024;
/**
 * Bound on one `dial`, from its arrival to its reply, the permission decision
 * included. A guest that waits at least this long for a dial reply never
 * misses one, and anything a dial would open after it is closed instead.
 */
export const JAM_PEER_TRANSPORT_DIAL_TIMEOUT_MS = 10_000;
const textEncoder = new TextEncoder();

const handshakeResult = S.Result(T.VersionedHostHandshakeResponse, S.CallError(T.VersionedHostHandshakeError));
const frameworkResult = S.Result(S._void, S.CallError(S._void));
const dialResult = S.Result(T.VersionedHostJamPeerTransportDialResponse, S.CallError(T.VersionedHostJamPeerTransportDialError));
const openResult = S.Result(T.VersionedHostJamPeerTransportOpenResponse, S.CallError(T.VersionedHostJamPeerTransportOpenError));
const sendResult = S.Result(T.VersionedHostJamPeerTransportSendResponse, S.CallError(T.VersionedHostJamPeerTransportSendError));
const recvResult = S.Result(T.VersionedHostJamPeerTransportRecvResponse, S.CallError(T.VersionedHostJamPeerTransportRecvError));
const resetResult = S.Result(T.VersionedHostJamPeerTransportResetResponse, S.CallError(T.VersionedHostJamPeerTransportResetError));
const closeResult = S.Result(T.VersionedHostJamPeerTransportCloseResponse, S.CallError(T.VersionedHostJamPeerTransportCloseError));
const eventsResult = S.Result(T.VersionedHostJamPeerTransportEventsResponse, S.CallError(T.VersionedHostJamPeerTransportEventsError));
const cancelledReply = frameworkResult.enc({ success: false, value: { tag: "Cancelled" } });

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

export interface JamPeerTransportOptions {
  /**
   * Decide whether this execution may dial peers of `genesis`, a `0x`-prefixed
   * lower-case 32-byte genesis header hash: the host's check of the
   * `RemotePermission::JamPeers` runtime permission. The session asks at most
   * once per genesis and concurrent dials share the pending answer; `false` or
   * a rejection answers `NotGranted` for the rest of the session.
   * At most eight distinct genesis decisions, including pending/refused ones,
   * are retained per session; a new ninth genesis answers `Limit`.
   */
  authorize(genesis: string): Promise<boolean>;
  /** Host transport injection; defaults to the browser `WebTransport` constructor. */
  connect?: (url: string, certificateHashes: Uint8Array[]) => WebTransportLike;
  /** Unix seconds used to select certificate validity periods; defaults to the wall clock. */
  now?: () => number;
  /** Dial deadline in milliseconds; defaults to {@link JAM_PEER_TRANSPORT_DIAL_TIMEOUT_MS}. */
  dialTimeoutMs?: number;
}

/** Execution-local peer endpoint. It provides no account or signing authority. */
export interface JamPeerTransportSession {
  /** Handle one request frame; CANCEL frames return zero bytes. */
  handleFrame(frame: Uint8Array): Promise<Uint8Array>;
  /** Close every connection on stop or replacement and refuse further requests. */
  close(): void;
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
  if (!(bytes instanceof Uint8Array) || bytes.length > JAM_PEER_TRANSPORT_MAX_FRAME_BYTES) {
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
  reader: ReadableStreamBYOBReader;
  /** Reserved receive bytes, including each message's length prefix. */
  rxBytes: number;
  /** Complete messages not yet delivered by `recv`. */
  messages: Uint8Array[];
  fin: boolean;
  reset: boolean;
  /** `recv` reported `fin` with an empty queue; further reads are `Closed`. */
  rxConsumed: boolean;
  txClosed: boolean;
  onClose: Set<() => void>;
}

interface PeerConnection {
  id: number;
  transport: WebTransportLike;
  streams: Map<number, PeerStream>;
  opening: number;
  rxBytes: number;
  /** Outgoing frames retain their reservation until writes settle, even after stream removal. */
  txBytes: number;
  rxWaiters: Set<() => void>;
  onClose: Set<() => void>;
  closed: boolean;
}

interface PendingRequest {
  method: number;
  response?: Uint8Array;
  promise: Promise<Uint8Array>;
  withdraw(response: Uint8Array): void;
}

/** Fill exactly one header/payload without reading or allocating the following message. */
async function readExact(reader: ReadableStreamBYOBReader, length: number): Promise<Uint8Array | undefined> {
  let bytes = new Uint8Array(length);
  let offset = 0;
  while (offset < length) {
    const { value, done } = await reader.read(bytes.subarray(offset));
    if (value !== undefined) {
      bytes = new Uint8Array(value.buffer);
      offset += value.byteLength;
    }
    if (done) {
      if (offset !== 0) throw new Error("Truncated peer message");
      return undefined;
    }
  }
  return bytes;
}

/**
 * Create the browser JamPeerTransport endpoint for one execution. Every `dial`
 * is authorized for its genesis through `options.authorize` before anything
 * connects; the other methods act only on connections an authorized dial
 * opened. A dial answers within its deadline, prompt included: one still
 * waiting then answers `Unreachable`, a CANCEL naming it answers `Cancelled`,
 * and in both cases whatever it opened is closed without holding a slot. The
 * permission decision is remembered either way, so a retry does not ask
 * again. The host must fence late replies against execution stop or
 * replacement.
 * Pending permission/handshake dials share the eight-connection admission
 * budget with established connections, before any permission request is made.
 */
export function createJamPeerTransportSession(options: JamPeerTransportOptions): JamPeerTransportSession {
  const decisions = new Map<string, {
    result?: boolean;
    waiters: Set<(granted: boolean) => void>;
  }>();
  const authorized = async (genesis: string, withdrawn: Promise<Uint8Array>): Promise<boolean | Uint8Array> => {
    let decision = decisions.get(genesis);
    if (decision === undefined) {
      decision = { waiters: new Set() };
      decisions.set(genesis, decision);
      const current = decision;
      void (async () => {
        let granted = false;
        try {
          granted = (await options.authorize(genesis)) === true;
        } catch {
          // A failed or dismissed permission request grants nothing.
        }
        if (closed) return;
        current.result = granted;
        for (const settle of current.waiters) settle(granted);
        current.waiters.clear();
      })();
    }
    if (decision.result !== undefined) return decision.result;
    // Only live dials subscribe; repeated cancellation cannot accumulate reactions
    // on the retained, potentially indefinitely pending permission request.
    let settle!: (granted: boolean) => void;
    const answer = new Promise<boolean>((resolve) => { settle = resolve; });
    decision.waiters.add(settle);
    try {
      return await Promise.race([answer, withdrawn]);
    } finally {
      decision.waiters.delete(settle);
    }
  };
  const connect =
    options.connect ??
    ((url, hashes): WebTransportLike =>
      new WebTransport(url, {
        serverCertificateHashes: hashes.map((value) => ({ algorithm: "sha-256", value: value as Uint8Array<ArrayBuffer> })),
      }) as unknown as WebTransportLike);
  const now = options.now ?? ((): number => Math.floor(Date.now() / 1000));
  const dialTimeoutMs = options.dialTimeoutMs ?? JAM_PEER_TRANSPORT_DIAL_TIMEOUT_MS;
  /** Every asynchronous request remains addressable until its reply is settled. */
  const pendingRequests = new Map<string, PendingRequest>();
  let closed = false;
  let negotiated = false;
  let nextConn = 1;
  let nextStream = 1;
  const connections = new Map<number, PeerConnection>();
  let pendingDialSlots = 0;
  const streams = new Map<number, PeerStream>();
  const events: T.JamPeerTransportEvent[] = [];

  const pushEvent = (event: T.JamPeerTransportEvent): void => {
    if (events.length < MAX_PENDING_EVENTS) events.push(event);
  };

  const wakeReaders = (conn: PeerConnection): void => {
    for (const wake of conn.rxWaiters) wake();
    conn.rxWaiters.clear();
  };

  const releaseReceived = (stream: PeerStream): void => {
    stream.conn.rxBytes -= stream.rxBytes;
    stream.rxBytes = 0;
    stream.messages.length = 0;
    wakeReaders(stream.conn);
  };

  const abortReceive = (stream: PeerStream): void => {
    stream.reset = true;
    for (const close of stream.onClose) close();
    stream.onClose.clear();
    releaseReceived(stream);
    void stream.writer.abort().catch(() => undefined);
    void stream.reader.cancel().catch(() => undefined);
  };

  const abortBidi = (bidi: WebTransportBidirectionalStreamLike): void => {
    void bidi.writable.abort().catch(() => undefined);
    void bidi.readable.cancel().catch(() => undefined);
  };

  const dropStream = (stream: PeerStream, abort: boolean): void => {
    streams.delete(stream.id);
    stream.conn.streams.delete(stream.id);
    for (const close of stream.onClose) close();
    stream.onClose.clear();
    releaseReceived(stream);
    if (abort) {
      stream.reset = true;
      void stream.writer.abort().catch(() => undefined);
      void stream.reader.cancel().catch(() => undefined);
    }
  };

  const dropConnection = (conn: PeerConnection): void => {
    if (conn.closed) return;
    conn.closed = true;
    connections.delete(conn.id);
    for (const close of conn.onClose) close();
    conn.onClose.clear();
    wakeReaders(conn);
    for (const stream of [...conn.streams.values()]) dropStream(stream, true);
    try {
      conn.transport.close();
    } catch {
      // Already closed by the peer.
    }
    pushEvent({ tag: "ConnClosed", value: { conn: conn.id } });
  };

  const reserveReceive = async (stream: PeerStream, bytes: number): Promise<boolean> => {
    while (!stream.reset && !stream.conn.closed) {
      if (stream.conn.rxBytes + stream.conn.txBytes + bytes <= JAM_PEER_TRANSPORT_MAX_BUFFERED_BYTES_PER_CONNECTION) {
        stream.rxBytes += bytes;
        stream.conn.rxBytes += bytes;
        return true;
      }
      // The package targets ES2022, before Promise.withResolvers.
      await new Promise<void>((resolve) => stream.conn.rxWaiters.add(resolve));
    }
    return false;
  };

  const pump = async (stream: PeerStream): Promise<void> => {
    try {
      while (!stream.reset && !stream.conn.closed) {
        if (!await reserveReceive(stream, 4)) return;
        const header = await readExact(stream.reader, 4);
        if (stream.reset || stream.conn.closed) return;
        if (header === undefined) {
          stream.rxBytes -= 4;
          stream.conn.rxBytes -= 4;
          wakeReaders(stream.conn);
          stream.fin = true;
          if (streams.has(stream.id)) pushEvent({ tag: "StreamFin", value: { stream: stream.id } });
          return;
        }
        const length = new DataView(header.buffer, header.byteOffset, 4).getUint32(0, true);
        if (length > JAM_PEER_TRANSPORT_MAX_MESSAGE_BYTES) {
          abortReceive(stream);
          return;
        }
        if (!await reserveReceive(stream, length)) return;
        const message = await readExact(stream.reader, length);
        if (stream.reset || stream.conn.closed) return;
        if (message === undefined) throw new Error("Truncated peer message");
        stream.messages.push(message);
      }
    } catch {
      if (!stream.reset && !stream.conn.closed) abortReceive(stream);
    }
  };

  const register = (conn: PeerConnection, bidi: WebTransportBidirectionalStreamLike, reader = bidi.readable.getReader({ mode: "byob" })): PeerStream => {
    const stream: PeerStream = {
      id: nextStream++,
      conn,
      writer: bidi.writable.getWriter(),
      reader,
      rxBytes: 0,
      messages: [],
      fin: false,
      reset: false,
      rxConsumed: false,
      txClosed: false,
      onClose: new Set(),
    };
    streams.set(stream.id, stream);
    conn.streams.set(stream.id, stream);
    void pump(stream);
    return stream;
  };

  const acceptLoop = async (conn: PeerConnection): Promise<void> => {
    const incoming = conn.transport.incomingBidirectionalStreams.getReader();
    const stop = (): void => { void incoming.cancel().catch(() => undefined); };
    conn.onClose.add(stop);
    try {
      while (!conn.closed) {
        const { value: bidi, done } = await incoming.read();
        if (done) break;
        if (conn.closed || conn.streams.size + conn.opening >= JAM_PEER_TRANSPORT_MAX_STREAMS_PER_CONNECTION) {
          abortBidi(bidi);
          continue;
        }
        conn.opening++;
        const reader = bidi.readable.getReader({ mode: "byob" });
        const cancel = (): void => {
          void reader.cancel().catch(() => undefined);
          void bidi.writable.abort().catch(() => undefined);
        };
        conn.onClose.add(cancel);
        // A peer that withholds one kind byte must not block the other reserved streams.
        void (async () => {
          try {
            const kind = await readExact(reader, 1);
            if (kind === undefined || conn.closed || events.length >= MAX_PENDING_EVENTS) {
              cancel();
              return;
            }
            const stream = register(conn, bidi, reader);
            pushEvent({ tag: "Accepted", value: { conn: conn.id, stream: stream.id, kind: kind[0]! } });
          } catch {
            cancel();
          } finally {
            conn.opening--;
            conn.onClose.delete(cancel);
          }
        })();
      }
    } catch {
      // The connection is closing; `closed` handling reports it.
    } finally {
      conn.onClose.delete(stop);
      incoming.releaseLock();
    }
  };

  /**
   * One dial. `withdrawn` settles with the reply when the guest cancels or the
   * deadline passes; the guest then no longer waits for what this dial opens,
   * so nothing it opens outlives it or holds a connection slot.
   */
  const dial = async (request: T.HostJamPeerTransportDialRequest, withdrawn: Promise<Uint8Array>): Promise<Uint8Array> => {
    if (connections.size + pendingDialSlots >= JAM_PEER_TRANSPORT_MAX_CONNECTIONS ||
        (!decisions.has(request.genesis) && decisions.size >= JAM_PEER_TRANSPORT_MAX_CONNECTIONS)) {
      return domain(dialResult, "Limit");
    }
    pendingDialSlots++;
    let granted: boolean | Uint8Array;
    try {
      granted = await authorized(request.genesis, withdrawn);
    } finally {
      pendingDialSlots--;
    }
    if (granted instanceof Uint8Array) return granted;
    if (!granted) return domain(dialResult, "NotGranted");
    if (closed) return frameworkResult.enc({ success: false, value: { tag: "Denied" } });
    if (connections.size >= JAM_PEER_TRANSPORT_MAX_CONNECTIONS) return domain(dialResult, "Limit");
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
    const conn: PeerConnection = {
      id: nextConn++, transport, streams: new Map(), closed: false,
      opening: 0, rxBytes: 0, txBytes: 0, rxWaiters: new Set(), onClose: new Set(),
    };
    connections.set(conn.id, conn);
    const failure = await Promise.race([
      transport.ready.then(
        () => undefined,
        () => domain(dialResult, "Refused"),
      ),
      withdrawn,
    ]);
    if (failure !== undefined || closed) {
      // The guest never learns this connection id, so it frees its slot
      // without a `ConnClosed` event.
      connections.delete(conn.id);
      conn.closed = true;
      try {
        transport.close();
      } catch {
        // Never opened.
      }
      return failure ?? frameworkResult.enc({ success: false, value: { tag: "Denied" } });
    }
    void transport.closed.then(
      () => dropConnection(conn),
      () => dropConnection(conn),
    );
    void acceptLoop(conn);
    return ok(dialResult, { tag: "V1", value: { conn: conn.id } });
  };

  const requestFrame = async (
    request: ProtocolMessage,
    run: (pending: PendingRequest) => Promise<Uint8Array>,
    timeout?: number,
  ): Promise<Uint8Array> => {
    // Executor form is required by this package's ES2022 target.
    let resolve!: (response: Uint8Array) => void;
    const promise = new Promise<Uint8Array>((settle) => { resolve = settle; });
    const pending: PendingRequest = {
      method: request.payload.methodId,
      promise,
      withdraw(response) {
        if (pending.response !== undefined) return;
        pending.response = response;
        resolve(response);
      },
    };
    pendingRequests.set(request.requestId, pending);
    const timer = timeout === undefined ? undefined :
      setTimeout(() => pending.withdraw(domain(dialResult, "Unreachable")), timeout);
    try {
      return await run(pending);
    } finally {
      clearTimeout(timer);
      pendingRequests.delete(request.requestId);
    }
  };

  const open = async (request: T.HostJamPeerTransportOpenRequest, pending: PendingRequest): Promise<Uint8Array> => {
    const conn = connections.get(request.conn);
    if (conn === undefined || conn.closed) return domain(openResult, "Closed");
    if (conn.streams.size + conn.opening >= JAM_PEER_TRANSPORT_MAX_STREAMS_PER_CONNECTION) return domain(openResult, "Limit");
    conn.opening++;
    let reserved = true;
    const close = (): void => pending.withdraw(domain(openResult, "Closed"));
    conn.onClose.add(close);
    let stream: PeerStream | undefined;
    try {
      const opening = conn.transport.createBidirectionalStream();
      void opening.then((bidi) => {
        if (pending.response !== undefined) abortBidi(bidi);
      }, () => undefined);
      const bidi = await Promise.race([opening, pending.promise]);
      if (bidi instanceof Uint8Array) return bidi;
      if (pending.response !== undefined || conn.closed) {
        abortBidi(bidi);
        return pending.response ?? domain(openResult, "Closed");
      }
      conn.opening--;
      reserved = false;
      stream = register(conn, bidi);
      stream.onClose.add(close);
      const result = await Promise.race([stream.writer.write(new Uint8Array([request.kind])), pending.promise]);
      if (result instanceof Uint8Array) return result;
      return ok(openResult, { tag: "V1", value: { stream: stream.id } });
    } catch {
      if (stream !== undefined) dropStream(stream, true);
      return domain(openResult, "Closed");
    } finally {
      if (reserved) conn.opening--;
      conn.onClose.delete(close);
      stream?.onClose.delete(close);
      if (pending.response !== undefined && stream !== undefined) dropStream(stream, true);
    }
  };

  const send = async (request: T.HostJamPeerTransportSendRequest, pending: PendingRequest): Promise<Uint8Array> => {
    const stream = streams.get(request.stream);
    if (stream === undefined || stream.txClosed || stream.reset || stream.conn.closed) return domain(sendResult, "Closed");
    const message = S.hexToBytes(request.message);
    if (message.length > JAM_PEER_TRANSPORT_MAX_MESSAGE_BYTES) return domain(sendResult, "TooLarge");
    if (stream.conn.rxBytes + stream.conn.txBytes + message.length + 4 > JAM_PEER_TRANSPORT_MAX_BUFFERED_BYTES_PER_CONNECTION) return domain(sendResult, "Limit");
    const frame = new Uint8Array(4 + message.length);
    new DataView(frame.buffer).setUint32(0, message.length, true);
    frame.set(message, 4);
    stream.conn.txBytes += frame.length;
    const close = (): void => pending.withdraw(domain(sendResult, "Closed"));
    stream.onClose.add(close);
    // Reserve FIN before yielding, so a concurrent send cannot pass it.
    if (request.fin) stream.txClosed = true;
    try {
      // Cancellation settles the guest request immediately, but a browser
      // write can still retain its frame until the underlying sink settles.
      const writing = stream.writer.write(frame).finally(() => {
        stream.conn.txBytes -= frame.length;
        wakeReaders(stream.conn);
      });
      let result = await Promise.race([writing, pending.promise]);
      if (result instanceof Uint8Array) return result;
      if (request.fin) {
        result = await Promise.race([stream.writer.close(), pending.promise]);
        if (result instanceof Uint8Array) return result;
        if (stream.rxConsumed) {
          stream.onClose.delete(close);
          dropStream(stream, false);
        }
      }
      return ok(sendResult, { tag: "V1" });
    } catch {
      stream.txClosed = true;
      return domain(sendResult, "Closed");
    } finally {
      stream.onClose.delete(close);
      if (pending.response !== undefined) dropStream(stream, true);
    }
  };

  const recv = (request: T.HostJamPeerTransportRecvRequest): Uint8Array => {
    const stream = streams.get(request.stream);
    if (stream === undefined || stream.rxConsumed) return domain(recvResult, "Closed");
    const next = stream.messages[0];
    if (next !== undefined && next.length > request.max) {
      // The guest cannot take this message; treat it as a protocol violation.
      abortReceive(stream);
    }
    let message: S.HexString | undefined;
    if (!stream.reset && next !== undefined) {
      stream.messages.shift();
      stream.rxBytes -= next.length + 4;
      stream.conn.rxBytes -= next.length + 4;
      wakeReaders(stream.conn);
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

  const reset = (request: T.HostJamPeerTransportResetRequest): Uint8Array => {
    const stream = streams.get(request.stream);
    if (stream === undefined) return domain(resetResult, "Closed");
    dropStream(stream, true);
    return ok(resetResult, { tag: "V1" });
  };

  const close = (request: T.HostJamPeerTransportCloseRequest): Uint8Array => {
    const conn = connections.get(request.conn);
    if (conn === undefined || conn.closed) return domain(closeResult, "Closed");
    dropConnection(conn);
    return ok(closeResult, { tag: "V1" });
  };

  return {
    async handleFrame(bytes) {
      const request = decodeFrame(bytes);
      if (request.payload.messageType === MESSAGE_TYPE_CANCEL) {
        if (request.payload.traitId !== JAM_PEER_TRANSPORT_DIAL.trait || request.payload.value.length !== 0) {
          throw new Error("Invalid cancellation frame");
        }
        const pending = pendingRequests.get(request.requestId);
        if (pending?.method === request.payload.methodId) pending.withdraw(cancelledReply);
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
      if (request.payload.traitId !== JAM_PEER_TRANSPORT_DIAL.trait) {
        return reply(request, frameworkResult.enc({ success: false, value: { tag: "Denied" } }));
      }
      const malformed = (): Uint8Array =>
        reply(request, frameworkResult.enc({ success: false, value: { tag: "MalformedFrame", value: { reason: "invalid peer-transport request" } } }));
      if (pendingRequests.has(request.requestId)) return new Uint8Array();
      try {
        if (hasIds(request, JAM_PEER_TRANSPORT_DIAL)) {
          const value = exact(T.VersionedHostJamPeerTransportDialRequest, request.payload.value).value;
          if (!negotiated) return reply(request, domain(dialResult, "NotGranted"));
          return reply(request, await requestFrame(request, (pending) => dial(value, pending.promise), dialTimeoutMs));
        }
        if (hasIds(request, JAM_PEER_TRANSPORT_OPEN)) {
          const value = exact(T.VersionedHostJamPeerTransportOpenRequest, request.payload.value).value;
          return reply(request, negotiated ? await requestFrame(request, (pending) => open(value, pending)) : domain(openResult, "NotGranted"));
        }
        if (hasIds(request, JAM_PEER_TRANSPORT_SEND)) {
          const value = exact(T.VersionedHostJamPeerTransportSendRequest, request.payload.value).value;
          return reply(request, negotiated ? await requestFrame(request, (pending) => send(value, pending)) : domain(sendResult, "Closed"));
        }
        if (hasIds(request, JAM_PEER_TRANSPORT_RECV)) {
          const value = exact(T.VersionedHostJamPeerTransportRecvRequest, request.payload.value).value;
          return reply(request, negotiated ? recv(value) : domain(recvResult, "Closed"));
        }
        if (hasIds(request, JAM_PEER_TRANSPORT_RESET)) {
          const value = exact(T.VersionedHostJamPeerTransportResetRequest, request.payload.value).value;
          return reply(request, negotiated ? reset(value) : domain(resetResult, "Closed"));
        }
        if (hasIds(request, JAM_PEER_TRANSPORT_CLOSE)) {
          const value = exact(T.VersionedHostJamPeerTransportCloseRequest, request.payload.value).value;
          return reply(request, negotiated ? close(value) : domain(closeResult, "Closed"));
        }
        if (hasIds(request, JAM_PEER_TRANSPORT_EVENTS)) {
          exact(T.VersionedHostJamPeerTransportEventsRequest, request.payload.value);
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
      const denied = frameworkResult.enc({ success: false, value: { tag: "Denied" } });
      for (const pending of pendingRequests.values()) pending.withdraw(denied);
      for (const conn of [...connections.values()]) dropConnection(conn);
      for (const decision of decisions.values()) decision.waiters.clear();
      decisions.clear();
      events.length = 0;
    },
  };
}
