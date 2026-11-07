/**
 * PROTOTYPE. DRAFT, NOT TRUAPI. `nova-chat/1`: the page-local port between
 * the host and Nova Chat's worker, the one product it serves. The host keeps
 * every key and does the steps of the official apps' chat protocol that need
 * one (`./boundary.ts`); the worker keeps the conversations and moves
 * statements through its own TrUAPI Statement Store.
 *
 * The worker knows people only by contact handle, `0x` hex. Bytes travel as
 * `0x` hex and 64-bit integers as decimal strings. Each call has an `id` and
 * one `result`; the host also pushes `state` unasked.
 */

export const NOVA_HELLO = "nova-chat-product";
export const NOVA_PROTOCOL = "nova-chat/1";

export interface NovaHello {
  type: typeof NOVA_HELLO;
  protocol: typeof NOVA_PROTOCOL;
}

/** The hello `data` is, when it hands over exactly one port. */
export function isNovaHello(data: unknown, ports: readonly unknown[]): boolean {
  const hello = data as Partial<NovaHello> | null;
  return (
    hello?.type === NOVA_HELLO &&
    hello.protocol === NOVA_PROTOCOL &&
    ports.length === 1
  );
}

/** An unsigned statement, as `sealFrame` answers it. */
export interface WireStatement {
  expiry: string;
  channel: string | null;
  topics: string[];
  data: string;
}

/** A statement the worker read, with its sr25519 proof. */
export interface WireSignedStatement extends WireStatement {
  signer: string;
  signature: string;
}

/** A transport frame: messages are encoded `ChatMessageStatement`s. */
export type WireFrame =
  | { kind: "Request"; requestId: string; messages: string[] }
  | { kind: "Response"; requestId: string; code: number };

/** User content, as the worker sends and receives it. */
export type WireContent =
  | { kind: "Text"; text: string }
  | { kind: "RichText"; text: string | null }
  | { kind: "Reacted"; messageId: string; emoji: string }
  | { kind: "ReactionRemoved"; messageId: string; emoji: string }
  | { kind: "Reply"; messageId: string; text: string | null }
  | { kind: "Edited"; messageId: string; text: string | null }
  | { kind: "LeftChat" }
  /** A kind this prototype does not show: calls, files, payments, device changes, Seen, and any newer one. */
  | { kind: "Unsupported"; index: number };

export interface WireMessage {
  id: string;
  /** Sender's clock, Unix milliseconds, decimal. */
  timestamp: string;
  content: WireContent;
}

/** An opened frame: a peer's request with its messages, or a peer's acknowledgement. */
export type OpenedFrame =
  | { kind: "Request"; requestId: string; messages: WireMessage[] }
  | { kind: "Response"; requestId: string; code: number };

/** Where a conversation stands, as the host tells the worker. */
export type NovaState =
  | "outgoing"
  | "incoming"
  | "established"
  | "blocked"
  | "gone";

export interface NovaPeer {
  handle: string;
  state: NovaState;
}

export type NovaCall =
  | { id: number; call: "list" }
  | { id: number; call: "request"; handle: string; welcome: string | null }
  | { id: number; call: "routes"; handle: string }
  | {
      id: number;
      call: "seal";
      handle: string;
      frame: WireFrame;
      expiry: string;
    }
  | { id: number; call: "open"; handle: string; statement: WireSignedStatement }
  | { id: number; call: "conversationAccount"; handle: string; index: string };

export type NovaResult =
  | { ok: true; value?: unknown }
  /**
   * `final`: a refusal that trying again does not change, such as a blocked
   * person or a chat that is not set up. Absent: it may pass later, such as
   * a claim that found no free slot or a chain the host could not reach.
   */
  | { ok: false; reason: string; final?: true };

export type NovaHostMessage =
  | { kind: "result"; id: number; result: NovaResult }
  /** A conversation changed: a request was sent, accepted, or the person was blocked. */
  | { kind: "state"; peer: NovaPeer };

const isHex = (value: unknown, bytes?: number): value is string =>
  typeof value === "string" &&
  /^0x([0-9a-fA-F]{2})*$/.test(value) &&
  (bytes === undefined || value.length === 2 + bytes * 2);

const isDecimal = (value: unknown): value is string =>
  typeof value === "string" && /^\d{1,20}$/.test(value);

function readFrame(value: unknown): WireFrame | null {
  const frame = value as Partial<{
    kind: string;
    requestId: unknown;
    messages: unknown;
    code: unknown;
  }> | null;
  if (typeof frame?.requestId !== "string" || frame.requestId.length > 128)
    return null;
  if (
    frame.kind === "Request" &&
    Array.isArray(frame.messages) &&
    frame.messages.length <= 64 &&
    frame.messages.every((message) => isHex(message))
  )
    return {
      kind: "Request",
      requestId: frame.requestId,
      messages: frame.messages,
    };
  if (
    frame.kind === "Response" &&
    typeof frame.code === "number" &&
    Number.isInteger(frame.code) &&
    frame.code >= 0 &&
    frame.code < 256
  )
    return { kind: "Response", requestId: frame.requestId, code: frame.code };
  return null;
}

function readStatement(value: unknown): WireSignedStatement | null {
  const statement = value as Partial<WireSignedStatement> | null;
  if (
    statement === null ||
    !isDecimal(statement.expiry) ||
    !(statement.channel === null || isHex(statement.channel, 32)) ||
    !Array.isArray(statement.topics) ||
    statement.topics.length > 4 ||
    !statement.topics.every((topic) => isHex(topic, 32)) ||
    !isHex(statement.data) ||
    !isHex(statement.signer, 32) ||
    !isHex(statement.signature, 64)
  )
    return null;
  return {
    expiry: statement.expiry,
    channel: statement.channel,
    topics: statement.topics,
    data: statement.data,
    signer: statement.signer,
    signature: statement.signature,
  };
}

/** A worker's call as the host reads it, or null when it is not one. */
export function readNovaCall(data: unknown): NovaCall | null {
  const call = data as Record<string, unknown> | null;
  if (call === null || typeof call !== "object") return null;
  const id = call.id;
  if (typeof id !== "number" || !Number.isSafeInteger(id)) return null;
  const handle = isHex(call.handle, 32) ? call.handle.toLowerCase() : null;
  switch (call.call) {
    case "list":
      return { id, call: "list" };
    case "request":
      return handle !== null &&
        (call.welcome === null ||
          (typeof call.welcome === "string" && call.welcome.length <= 2000))
        ? { id, call: "request", handle, welcome: call.welcome }
        : null;
    case "routes":
      return handle === null ? null : { id, call: "routes", handle };
    case "seal": {
      const frame = readFrame(call.frame);
      return handle !== null && frame !== null && isDecimal(call.expiry)
        ? { id, call: "seal", handle, frame, expiry: call.expiry }
        : null;
    }
    case "open": {
      const statement = readStatement(call.statement);
      return handle !== null && statement !== null
        ? { id, call: "open", handle, statement }
        : null;
    }
    case "conversationAccount":
      return handle !== null && isHex(call.index, 32)
        ? {
            id,
            call: "conversationAccount",
            handle,
            index: (call.index as string).toLowerCase(),
          }
        : null;
    default:
      return null;
  }
}
