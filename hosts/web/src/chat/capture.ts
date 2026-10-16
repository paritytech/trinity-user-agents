import { bytesToHex, hexToBytes } from "@parity/truapi/scale";
import { sanitizeReason } from "./diagnostics.js";
import { statementHash } from "./statement.js";
import { statementsOf, type RpcTap } from "./statement-store.js";

/**
 * What the chat engine carried, kept for the inspector when the user turns
 * capture on for the session. Off by default, local to this tab, held in
 * memory only, and dropped when the session changes.
 *
 * Bytes come only from the places this module names, each the exact value at
 * one boundary:
 *
 * - {@link PayloadCapture.connectionTap}: every JSON-RPC text one statement
 *   store connection sends and receives, as it goes to and comes from it;
 * - {@link PayloadSink.statement}: an encoded signed statement as received
 *   from the store or as submitted to it;
 * - {@link PayloadSink.plaintext}: a statement's data after decryption, or
 *   before encryption.
 *
 * Statement bytes and frames are public: the store and anyone reading it see
 * them. Plaintext is not: it holds message text, push tokens and device keys,
 * so it is kept only at the `decrypted` level. Keys, shared secrets and the
 * keys derived from them are never passed here.
 */

/** How much is kept: nothing, what travels, or what travels and its decryption. */
export type CaptureLevel = "off" | "wire" | "decrypted";

/** The most bytes all captures hold together. */
export const CAPTURE_BYTE_BUDGET = 4 * 1024 * 1024;
/** The most frames, and separately the most statements, kept. */
export const CAPTURE_ITEM_LIMIT = 300;
/** The most one frame's text or one statement's bytes keeps. */
export const CAPTURE_ITEM_BYTES = 256 * 1024;

export type CaptureStream = "requests" | "sessions";
export type CaptureDirection = "in" | "out";

/** One JSON-RPC text, as sent or received. */
export interface CapturedFrame {
  seq: number;
  /** Which connection carried it. JSON-RPC ids are numbered per connection. */
  connection: number;
  at: number;
  direction: CaptureDirection;
  /** The text, cut to {@link CAPTURE_ITEM_BYTES} characters. */
  text: string;
  /** The length of the whole text, in characters. */
  length: number;
  rpcId: number | null;
  method: string | null;
  /** The subscription a notification is for. */
  subscription: string | null;
  /** Hashes of the statements the frame carries, in the frame's order. */
  statements: string[];
}

/** One statement, keyed by the hash of its encoded bytes. */
export interface CapturedStatement {
  hash: string;
  at: number;
  direction: CaptureDirection;
  stream: CaptureStream;
  /** The encoded signed statement, cut to {@link CAPTURE_ITEM_BYTES}. */
  raw: Uint8Array;
  /** The length of the whole encoding. */
  length: number;
  /** The decrypted data, at the `decrypted` level. */
  plaintext: Uint8Array | null;
  /** Why there is no plaintext, when the engine said. */
  plaintextGap: string | null;
}

/** Where the chat engine hands its payloads. */
export interface PayloadSink {
  statement(
    direction: CaptureDirection,
    stream: CaptureStream,
    encoded: Uint8Array,
  ): void;
  plaintext(hash: string, plaintext: Uint8Array): void;
  gap(hash: string, reason: string): void;
}

/** What eviction and truncation have cost, since the capture was last emptied. */
export interface CaptureLosses {
  evicted: number;
  truncated: number;
}

const hashHex = (encoded: Uint8Array): string =>
  bytesToHex(statementHash(encoded));

interface RpcFields {
  id?: unknown;
  method?: unknown;
  params?: unknown;
  result?: unknown;
}

function parseRpc(text: string): RpcFields | null {
  try {
    const message: unknown = JSON.parse(text);
    return typeof message === "object" && message !== null
      ? (message as RpcFields)
      : null;
  } catch {
    return null;
  }
}

/** The topics a subscribe request's filter names, sorted, or null for another frame. */
function filterTopics(frame: CapturedFrame): string | null {
  if (frame.method !== "statement_subscribeStatement") return null;
  const params = parseRpc(frame.text)?.params;
  const filter = Array.isArray(params)
    ? (params[0] as { matchAll?: unknown; matchAny?: unknown } | undefined)
    : undefined;
  const topics = filter?.matchAll ?? filter?.matchAny;
  return Array.isArray(topics) ? [...topics].sort().join() : null;
}

function frameFields(
  direction: CaptureDirection,
  text: string,
): Pick<CapturedFrame, "rpcId" | "method" | "subscription" | "statements"> {
  const none = {
    rpcId: null,
    method: null,
    subscription: null,
    statements: [],
  };
  const message = parseRpc(text);
  if (message === null) return none;
  const rpcId = typeof message.id === "number" ? message.id : null;
  const method = typeof message.method === "string" ? message.method : null;
  if (direction === "out") {
    const params = Array.isArray(message.params) ? message.params : [];
    const statements =
      method === "statement_submit" && typeof params[0] === "string"
        ? [hashHex(hexToBytes(params[0] as `0x${string}`))]
        : [];
    return { rpcId, method, subscription: null, statements };
  }
  const params = message.params as
    | { subscription?: unknown; result?: unknown }
    | undefined;
  const subscription =
    typeof params?.subscription === "string" ? params.subscription : null;
  return {
    rpcId,
    method,
    subscription,
    statements:
      subscription === null ? [] : statementsOf(params?.result).map(hashHex),
  };
}

type Entry =
  | { kind: "frame"; seq: number }
  | { kind: "statement"; hash: string };

export class PayloadCapture implements PayloadSink {
  private levelValue: CaptureLevel = "off";
  private frames = new Map<number, CapturedFrame>();
  private statements = new Map<string, CapturedStatement>();
  private order: Entry[] = [];
  private bytes = 0;
  private seq = 0;
  private lost: CaptureLosses = { evicted: 0, truncated: 0 };
  private connections = 0;
  private generationValue = 0;
  private readonly listeners = new Set<() => void>();

  constructor(
    private readonly budget = CAPTURE_BYTE_BUDGET,
    private readonly now: () => number = Date.now,
  ) {}

  get level(): CaptureLevel {
    return this.levelValue;
  }

  /** Bumped whenever captured data is dropped or the level changes. */
  get generation(): number {
    return this.generationValue;
  }

  /** Change the level. A lower level drops what it no longer keeps at once. */
  setLevel(level: CaptureLevel): void {
    if (level === this.levelValue) return;
    this.levelValue = level;
    this.generationValue += 1;
    if (level === "off") this.empty();
    else if (level === "wire")
      for (const statement of this.statements.values())
        if (statement.plaintext !== null) {
          this.bytes -= statement.plaintext.length;
          statement.plaintext = null;
          statement.plaintextGap = "decrypted capture was turned off";
        }
    this.notify();
  }

  /** Forget everything and turn capture off, as at a session change. */
  reset(): void {
    this.levelValue = "off";
    this.empty();
    this.notify();
  }

  /** Forget what was captured, keeping the level. */
  forget(): void {
    this.empty();
    this.notify();
  }

  /** A tap for one new statement store connection. */
  connectionTap(): RpcTap {
    const connection = ++this.connections;
    return (direction, text) => this.frame(connection, direction, text);
  }

  private frame(
    connection: number,
    direction: CaptureDirection,
    text: string,
  ): void {
    if (this.levelValue === "off") return;
    const kept =
      text.length > CAPTURE_ITEM_BYTES
        ? text.slice(0, CAPTURE_ITEM_BYTES)
        : text;
    if (kept !== text) this.lost.truncated += 1;
    const frame: CapturedFrame = {
      seq: ++this.seq,
      connection,
      at: this.now(),
      direction,
      text: kept,
      length: text.length,
      ...frameFields(direction, text),
    };
    this.frames.set(frame.seq, frame);
    this.order.push({ kind: "frame", seq: frame.seq });
    this.bytes += kept.length;
    this.trim();
    this.notify();
  }

  statement(
    direction: CaptureDirection,
    stream: CaptureStream,
    encoded: Uint8Array,
  ): void {
    if (this.levelValue === "off") return;
    const hash = hashHex(encoded);
    if (this.statements.has(hash)) return;
    const raw = encoded.slice(0, CAPTURE_ITEM_BYTES);
    if (raw.length < encoded.length) this.lost.truncated += 1;
    this.statements.set(hash, {
      hash,
      at: this.now(),
      direction,
      stream,
      raw,
      length: encoded.length,
      plaintext: null,
      plaintextGap: null,
    });
    this.order.push({ kind: "statement", hash });
    this.bytes += raw.length;
    this.trim();
    this.notify();
  }

  plaintext(hash: string, plaintext: Uint8Array): void {
    const statement = this.statements.get(hash);
    if (statement === undefined || statement.plaintext !== null) return;
    if (this.levelValue !== "decrypted") {
      statement.plaintextGap =
        "decrypted capture is off: the wire level keeps no plaintext";
      this.notify();
      return;
    }
    const kept = plaintext.slice(0, CAPTURE_ITEM_BYTES);
    if (kept.length < plaintext.length) this.lost.truncated += 1;
    statement.plaintext = kept;
    statement.plaintextGap =
      kept.length < plaintext.length
        ? `cut to ${kept.length} of ${plaintext.length} bytes`
        : null;
    this.bytes += kept.length;
    this.trim();
    this.notify();
  }

  gap(hash: string, reason: string): void {
    const statement = this.statements.get(hash);
    if (statement === undefined || statement.plaintext !== null) return;
    statement.plaintextGap = sanitizeReason(reason);
    this.notify();
  }

  /** The statement whose encoding hashes to `hash`, if it was captured. */
  statementFor(hash: string): CapturedStatement | undefined {
    return this.statements.get(hash);
  }

  /** The first answer to a request frame: same connection, same JSON-RPC id, after it. */
  private answerTo(request: CapturedFrame): CapturedFrame | undefined {
    if (request.rpcId === null) return undefined;
    for (const frame of this.frames.values())
      if (
        frame.seq > request.seq &&
        frame.direction === "in" &&
        frame.method === null &&
        frame.connection === request.connection &&
        frame.rpcId === request.rpcId
      )
        return frame;
    return undefined;
  }

  /**
   * The frames that carried the statement, oldest first: notifications that
   * delivered it, the submission that sent it, and the answer to that
   * submission.
   */
  framesFor(hash: string): CapturedFrame[] {
    const carrying = [...this.frames.values()].filter((frame) =>
      frame.statements.includes(hash),
    );
    const answers = carrying
      .filter((frame) => frame.direction === "out")
      .map((frame) => this.answerTo(frame))
      .filter((frame) => frame !== undefined);
    return [...carrying, ...answers].sort(
      (left, right) => left.seq - right.seq,
    );
  }

  /**
   * The newest subscribe request whose filter names exactly `topicIds`, and
   * the answer to it. Unsubscribing names the subscription id that answer
   * gave, so its request is included when it was sent.
   */
  subscriptionFrames(
    topicIds: readonly string[],
    action: "subscribe" | "unsubscribe",
  ): CapturedFrame[] {
    const frames = [...this.frames.values()];
    const wanted = [...topicIds].sort().join();
    const request = frames.findLast((frame) => filterTopics(frame) === wanted);
    if (request === undefined) return [];
    const answer = this.answerTo(request);
    const subscription =
      answer === undefined ? undefined : parseRpc(answer.text)?.result;
    const stop =
      action === "unsubscribe" && typeof subscription === "string"
        ? frames.filter((frame) => {
            if (
              frame.method !== "statement_unsubscribeStatement" ||
              frame.connection !== request.connection
            )
              return false;
            const params = parseRpc(frame.text)?.params;
            return Array.isArray(params) && params[0] === subscription;
          })
        : [];
    return [request, ...(answer ? [answer] : []), ...stop];
  }

  /** Everything held, oldest first. */
  snapshot(): { frames: CapturedFrame[]; statements: CapturedStatement[] } {
    return {
      frames: [...this.frames.values()],
      statements: [...this.statements.values()],
    };
  }

  /** How many frames and statements are held, and their bytes. */
  usage(): {
    frames: number;
    statements: number;
    bytes: number;
    budget: number;
  } {
    return {
      frames: this.frames.size,
      statements: this.statements.size,
      bytes: this.bytes,
      budget: this.budget,
    };
  }

  losses(): CaptureLosses {
    return { ...this.lost };
  }

  onChange(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private empty(): void {
    this.generationValue += 1;
    this.frames.clear();
    this.statements.clear();
    this.order = [];
    this.bytes = 0;
    this.lost = { evicted: 0, truncated: 0 };
  }

  /** Drop the oldest entries until each kind is within its count and all are within the byte budget. */
  private trim(): void {
    const evict = (index: number) => {
      const [entry] = this.order.splice(index, 1);
      if (entry.kind === "frame") {
        this.bytes -= this.frames.get(entry.seq)!.text.length;
        this.frames.delete(entry.seq);
      } else {
        const statement = this.statements.get(entry.hash)!;
        this.bytes -= statement.raw.length + (statement.plaintext?.length ?? 0);
        this.statements.delete(entry.hash);
      }
      this.lost.evicted += 1;
    };
    while (this.frames.size > CAPTURE_ITEM_LIMIT)
      evict(this.order.findIndex((entry) => entry.kind === "frame"));
    while (this.statements.size > CAPTURE_ITEM_LIMIT)
      evict(this.order.findIndex((entry) => entry.kind === "statement"));
    while (this.bytes > this.budget && this.order.length > 0) evict(0);
  }

  private notify(): void {
    for (const listener of this.listeners) listener();
  }
}
