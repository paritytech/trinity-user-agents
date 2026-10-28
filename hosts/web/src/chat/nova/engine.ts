import { bytesToHex, hexToBytes } from "@parity/truapi/scale";
import type { HexString } from "@parity/truapi/scale";
import { equalBytes } from "./codec.js";
import { cryptoRandom, type RandomBytes } from "./crypto.js";
import {
  sanitizeReason,
  shortHex,
  shortId,
  subscriptionEvent,
  type CaptureStream,
  type ChatEventBody,
  type ChatEventSink,
  type PayloadSink,
} from "./events.js";
import {
  lookupChatIdentity,
  lookupOwner,
  type ChainDirectory,
} from "./directory.js";
import {
  HOP_MAX_FILE_BYTES,
  hopDownload,
  hopUpload,
  knownNode,
  type HopConnection,
  type HopSender,
} from "./hop.js";
import type { NovaIdentity, PairTopics } from "./identity.js";
import {
  ACKNOWLEDGED,
  decodeMessage,
  decodeSessionData,
  encodeMessage,
  encodeSessionData,
  messageKind,
  openRequestPlaintext,
  requestTopic,
  withoutTickets,
  type ChatMessage,
  type Attachment,
  type MessageContent,
  type OpenedRequest,
  type SessionData,
} from "./protocol.js";
import {
  decodeStatement,
  expiryAt,
  nextExpiry,
  requestDay,
  statementHash,
  verifyStatement,
} from "./statement.js";
import {
  isStored,
  type SubmitResult,
  type TopicFilter,
} from "./statement-store.js";
import {
  loadChatRecord,
  saveChatRecord,
  type ChatRecord,
  type Contact,
  type IncomingRequest,
  type PendingMessage,
  type StoredAttachment,
  type StoredMessage,
} from "./store.js";

/**
 * The chat engine for one wallet on one network, written from the Polkadot
 * app's pairwise protocol.
 *
 * Constructing it does nothing. {@link ChatEngine.start} listens for
 * requests addressed to the wallet's identity and for its contacts' sessions,
 * and reconnects when the connection drops, until {@link ChatEngine.stop}.
 * Every write (accepting, sending a request or text, acknowledging) needs
 * {@link ChatEngineDeps.writes} to answer true at the time of the write; when
 * it does not, the engine publishes nothing, acknowledges nothing, and
 * refuses user actions that would publish. It announces no device key, so a
 * peer talks to it pairwise.
 */

export interface StatementStoreLike {
  subscribe(
    filter: TopicFilter,
    onStatements: (statements: Uint8Array[]) => void,
  ): Promise<() => void>;
  submit(statement: Uint8Array): Promise<SubmitResult>;
  close(): void;
  /** Settles when the connection ends for any reason, including {@link close}. */
  readonly closed: Promise<void>;
}

export interface ChatEngineDeps {
  /** The wallet's chat keys, as operations: the engine never holds a secret key. */
  identity: NovaIdentity;
  openStore(): Promise<StatementStoreLike>;
  directory: ChainDirectory;
  storage: Storage;
  recordKey: string;
  now?: () => number;
  random?: RandomBytes;
  newId?: () => string;
  /** Whether the user allowed this engine to publish statements now. Absent means never. */
  writes?: () => boolean;
  /** How long to wait before reconnecting after a failed start or a dropped connection. */
  retryDelayMs?: number;
  /** Where to report what the engine does, as metadata only. */
  diagnostics?: ChatEventSink;
  /**
   * Where to hand the statements it receives and sends, and their data in
   * the clear. Only statement bytes and plaintext go here, never a key.
   */
  payloads?: PayloadSink;
  /** How files travel. Absent: this engine neither sends nor fetches files. */
  hop?: HopService;
}

/** The HOP nodes this host may use, and the account that pays for uploads. */
export interface HopService {
  /** The nodes configured for the network. Only these are ever connected to. */
  nodes(): readonly string[];
  connect(node: string): Promise<HopConnection>;
  /** Null when this host has no account to submit with. */
  sender: HopSender | null;
}

/** A file to send, read by the host from what the user picked. */
export interface OutgoingFile {
  bytes: Uint8Array;
  name: string;
  mimeType: string;
  /** For images whose size the host could read. */
  image?: { width: number; height: number };
}

/** A statement ready to submit, and its data before it was sealed. */
interface Built {
  statement: Uint8Array;
  plaintext: Uint8Array;
}

/** Why a submission was made, for diagnostics. */
interface SubmitContext {
  purpose: "chat request" | "accept" | "messages" | "leave" | "ack";
  peer: string;
  requestId?: string;
  messages?: number;
}

export type EngineStatus =
  | { state: "starting" }
  | { state: "listening" }
  | { state: "stopped" }
  | { state: "failed"; reason: string; retryInMs: number };

/** How this identity's chat key on chain compares with the one the wallet derives. */
export type ChainKeyState =
  | "unknown"
  | "matches"
  | "differs"
  | "missing"
  | "conflict"
  | "unreadable";

export interface ChatSnapshot {
  status: EngineStatus;
  account: HexString;
  chatKey: HexString;
  name: string | null;
  chainKey: ChainKeyState;
  /** Whether this identity holds a statement allowance; null until read. */
  canSend: boolean | null;
  requests: IncomingRequest[];
  contacts: Contact[];
  /** Statements seen and set aside, such as multi-device envelopes this host does not open. */
  skipped: string[];
}

export const WRITES_OFF =
  "Chat writes are off. Allow them for this session to publish statements.";
const NO_ALLOWANCE =
  "This identity has no statement allowance on the People chain, so it cannot publish chat statements.";
const SKIPPED_LIMIT = 20;
/** Reaction and call signal ids kept per contact to recognise one delivered again. */
const SEEN_SIGNALS_LIMIT = 500;
/** Call signals older than this are not sent again: the call is long over. */
const CALL_SIGNAL_TTL_MS = 2 * 60_000;

/** A call signal from a peer, for whoever runs calls. */
export interface IncomingCallSignal {
  account: HexString;
  /** The message's id; for an offer, the call's id. */
  id: string;
  /** As the sender stamped it. */
  at: number;
  content: Extract<
    MessageContent,
    { kind: "call-offer" | "call-answer" | "call-candidates" | "call-closed" }
  >;
}

/** A call signal this host sends. */
export type OutgoingCallSignal = Extract<
  MessageContent,
  { kind: "call-offer" | "call-answer" | "call-candidates" | "call-closed" }
>;

/** One message of a request this host sends: a bubble or a pending reaction. */
interface Outgoing {
  id: string;
  at: number;
  content: MessageContent;
  /** The stored bubble, whose status follows the request. */
  message?: StoredMessage;
}
const RETRY_DELAY_MS = 5_000;
const MAX_RETRY_DELAY_MS = 60_000;

const errorText = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

export class ChatEngine {
  private record: ChatRecord;
  private status: EngineStatus = { state: "stopped" };
  private name: string | null = null;
  private chainKey: ChainKeyState = "unknown";
  private canSend: boolean | null = null;
  private skipped: string[] = [];
  private store: StatementStoreLike | null = null;
  private stopRequests: (() => void) | null = null;
  private stopSessions: (() => void) | null = null;
  private sessionTopics = "";
  /** Session topics per peer and chat key; public, so kept for the engine's life. */
  private readonly topics = new Map<string, PairTopics>();
  private queue: Promise<void> = Promise.resolve();
  /** Bumped by every start and stop, so work from an earlier run ends quietly. */
  private run = 0;
  private retryTimer: ReturnType<typeof setTimeout> | null = null;
  private failures = 0;
  private readonly listeners = new Set<() => void>();
  private readonly now: () => number;
  private readonly random: RandomBytes;
  private readonly newId: () => string;
  /** Transfers in flight, ended by {@link stop}. */
  private readonly transfers = new Map<string, AbortController>();
  private readonly callListeners = new Set<
    (signal: IncomingCallSignal) => void
  >();

  constructor(private readonly deps: ChatEngineDeps) {
    this.record = loadChatRecord(deps.storage, deps.recordKey);
    this.now = deps.now ?? Date.now;
    this.random = deps.random ?? cryptoRandom;
    this.newId = deps.newId ?? (() => crypto.randomUUID());
  }

  private get me(): Uint8Array {
    return this.deps.identity.identity;
  }

  private emit(event: ChatEventBody): void {
    this.deps.diagnostics?.(event);
  }

  /** Run a user action in order, and report it when it fails. */
  private userAction(action: string, work: () => Promise<void>): Promise<void> {
    return this.serial(work).catch((error: unknown) => {
      this.emit({
        type: "action-failed",
        action,
        reason: sanitizeReason(errorText(error)),
      });
      throw error;
    });
  }

  onChange(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  snapshot(): ChatSnapshot {
    return {
      status: this.status,
      account: bytesToHex(this.me),
      chatKey: bytesToHex(this.deps.identity.chatPublic),
      name: this.name,
      chainKey: this.chainKey,
      canSend: this.canSend,
      requests: this.record.requests,
      contacts: this.record.contacts,
      skipped: this.skipped,
    };
  }

  private changed(): void {
    saveChatRecord(this.deps.storage, this.deps.recordKey, this.record);
    for (const listener of this.listeners) listener();
  }

  /** Run engine work one piece at a time, in order. */
  private serial<T>(work: () => Promise<T>): Promise<T> {
    const run = this.queue.then(work);
    this.queue = run.then(
      () => undefined,
      () => undefined,
    );
    return run;
  }

  /** Start listening. Does nothing while already starting or listening. */
  async start(): Promise<void> {
    if (this.status.state === "starting" || this.status.state === "listening")
      return;
    this.clearRetry();
    const run = ++this.run;
    this.status = { state: "starting" };
    this.emit({ type: "lifecycle", state: "starting" });
    this.changed();
    void this.readOwnStanding();
    let store: StatementStoreLike | null = null;
    try {
      store = await this.deps.openStore();
      if (run !== this.run) {
        store.close();
        return;
      }
      this.store = store;
      const opened = store;
      void opened.closed.then(() => this.lost(run, opened));
      const inbox = requestTopic(this.me);
      this.stopRequests = await opened.subscribe(
        { matchAll: [inbox] },
        (statements) =>
          void this.serial(() => this.receiveRequests(statements)),
      );
      this.emit(
        subscriptionEvent("requests", "subscribe", [bytesToHex(inbox)]),
      );
      await this.serial(() => this.listenToSessions());
      if (run !== this.run) return;
      this.failures = 0;
      this.status = { state: "listening" };
      this.emit({ type: "lifecycle", state: "listening" });
      this.changed();
    } catch (error) {
      if (run !== this.run) return;
      this.dropConnection();
      this.retryAfter(errorText(error));
    }
  }

  /** Stop listening, and stop reconnecting. */
  stop(): void {
    for (const transfer of this.transfers.values()) transfer.abort();
    this.transfers.clear();
    this.run += 1;
    this.clearRetry();
    this.dropConnection();
    this.failures = 0;
    this.status = { state: "stopped" };
    this.emit({ type: "lifecycle", state: "stopped" });
    this.changed();
  }

  private dropConnection(): void {
    this.stopRequests?.();
    this.stopSessions?.();
    this.stopRequests = this.stopSessions = null;
    this.sessionTopics = "";
    const store = this.store;
    this.store = null;
    store?.close();
  }

  /** The connection of run `run` ended. Unless it was replaced or stopped, reconnect. */
  private lost(run: number, store: StatementStoreLike): void {
    if (run !== this.run || this.store !== store) return;
    this.run += 1;
    this.dropConnection();
    this.retryAfter("the People chain connection closed");
  }

  private retryAfter(reason: string): void {
    const base = this.deps.retryDelayMs ?? RETRY_DELAY_MS;
    const retryInMs = Math.min(base * 2 ** this.failures, MAX_RETRY_DELAY_MS);
    this.failures += 1;
    this.status = { state: "failed", reason, retryInMs };
    this.emit({
      type: "lifecycle",
      state: "failed",
      reason: sanitizeReason(reason),
      retryInMs,
    });
    this.changed();
    this.retryTimer = setTimeout(() => {
      this.retryTimer = null;
      void this.start();
    }, retryInMs);
  }

  private clearRetry(): void {
    if (this.retryTimer !== null) clearTimeout(this.retryTimer);
    this.retryTimer = null;
  }

  private async readOwnStanding(): Promise<void> {
    const [identity, allowance] = await Promise.allSettled([
      lookupChatIdentity(this.deps.directory, this.me),
      this.deps.directory.hasAllowance(this.me),
    ]);
    this.emit(
      identity.status === "fulfilled"
        ? {
            type: "lookup",
            account: shortHex(bytesToHex(this.me)),
            outcome: identity.value?.conflict
              ? "conflict"
              : identity.value?.chatKey
                ? "found"
                : "missing",
            from: identity.value?.from ?? [],
          }
        : {
            type: "lookup",
            account: shortHex(bytesToHex(this.me)),
            outcome: "error",
            reason: sanitizeReason(errorText(identity.reason)),
          },
    );
    if (identity.status === "fulfilled") {
      const onChain = identity.value?.chatKey ?? null;
      this.name = identity.value?.name ?? null;
      this.chainKey = identity.value?.conflict
        ? "conflict"
        : onChain === null
          ? "missing"
          : equalBytes(onChain, this.deps.identity.chatPublic)
            ? "matches"
            : "differs";
    } else this.chainKey = "unreadable";
    this.canSend = allowance.status === "fulfilled" ? allowance.value : null;
    this.changed();
  }

  // -------------------------------------------------------------------------
  // Receiving
  // -------------------------------------------------------------------------

  private skip(reason: string): void {
    this.skipped = [reason, ...this.skipped].slice(0, SKIPPED_LIMIT);
  }

  /** Hand a received statement to the payload sink, and name it by its hash. */
  private received(stream: CaptureStream, encoded: Uint8Array): string {
    this.deps.payloads?.statement("in", stream, encoded);
    return bytesToHex(statementHash(encoded));
  }

  /** Decrypt with `open`, handing the plaintext, or why there is none, to the payload sink. */
  private async decrypt(
    statement: string,
    open: () => Promise<Uint8Array>,
  ): Promise<Uint8Array> {
    let plaintext: Uint8Array;
    try {
      plaintext = await open();
    } catch (error) {
      this.deps.payloads?.gap(
        statement,
        sanitizeReason(`could not decrypt: ${errorText(error)}`),
      );
      throw error;
    }
    this.deps.payloads?.plaintext(statement, withoutTickets(plaintext));
    return plaintext;
  }

  private async receiveRequests(statements: Uint8Array[]): Promise<void> {
    for (const encoded of statements) {
      const hash = this.received("requests", encoded);
      try {
        const statement = decodeStatement(encoded);
        if (!verifyStatement(statement))
          throw new Error("its proof does not verify");
        const plaintext = await this.decrypt(hash, () =>
          this.deps.identity.openRequest(statement.data),
        );
        const request = openRequestPlaintext(plaintext, this.me);
        const peer = bytesToHex(request.peer);
        const seen = (
          outcome: Extract<ChatEventBody, { type: "request" }>["outcome"],
          problem?: string | null,
        ) =>
          this.emit({
            type: "request",
            requestId: shortId(request.id),
            peer: shortHex(peer),
            version: request.content.version,
            outcome,
            ...(problem ? { problem: sanitizeReason(problem) } : {}),
            statement: hash,
          });
        const known = this.record.requests.findIndex(
          (candidate) => candidate.id === request.id,
        );
        // A request whose sender's key could not be found is looked up again
        // each time the store delivers it, so a key read later is picked up.
        if (known !== -1) {
          if (this.record.requests[known].peerChatKey === null) {
            this.record.requests[known] = await this.describeRequest(
              request,
              peer,
            );
            seen("looked up again", this.record.requests[known].problem);
          } else seen("known");
          continue;
        }
        if (this.record.declined.includes(request.id)) {
          seen("declined earlier");
          continue;
        }
        const existing = this.contact(peer);
        if (existing?.blocked) {
          seen("blocked");
          continue;
        }
        // A peer who left and asks again is a new request, as on the apps.
        if (existing?.state === "active" && existing.peerLeft !== true) {
          seen("contact active");
          continue;
        }
        const waiting = existing?.state === "requested" ? existing : undefined;
        if (waiting !== undefined) {
          // Both sides asked at once; the peer's request answers this host's.
          waiting.state = "active";
          seen("answers ours");
          continue;
        }
        const described = await this.describeRequest(request, peer);
        this.record.requests.push(described);
        seen("new", described.problem);
      } catch (error) {
        this.skip(`A chat request could not be read: ${errorText(error)}`);
        this.emit({
          type: "set-aside",
          stream: "requests",
          reason: sanitizeReason(errorText(error)),
          statement: hash,
        });
      }
    }
    this.changed();
  }

  private async describeRequest(
    request: OpenedRequest,
    peer: HexString,
  ): Promise<IncomingRequest> {
    const described: IncomingRequest = {
      id: request.id,
      peer,
      peerChatKey: null,
      name: null,
      welcome: request.content.welcome,
      at: Number(request.timestampMs),
      version: request.content.version,
      problem: null,
    };
    try {
      const identity = await lookupChatIdentity(
        this.deps.directory,
        request.peer,
      );
      const chatKey = identity?.chatKey ?? null;
      this.emit({
        type: "lookup",
        account: shortHex(peer),
        outcome: identity?.conflict
          ? "conflict"
          : chatKey !== null
            ? "found"
            : "missing",
        from: identity?.from ?? [],
      });
      described.name = identity?.name ?? null;
      if (identity?.conflict) {
        described.problem =
          "The sender's chat key on Asset Hub and on the People chain differ, so neither is used.";
        return described;
      }
      if (chatKey === null) {
        described.problem =
          "The sender has no chat key on Asset Hub or the People chain, so no session can be opened.";
        return described;
      }
      described.peerChatKey = bytesToHex(chatKey);
      if (
        request.content.version === 2 &&
        !(await this.deps.identity.identityProofMatches(chatKey, {
          identity: request.content.identity,
          signer: request.signer,
          proof: request.content.identityProof,
        }))
      )
        described.problem =
          "The request's identity proof does not match the sender's chat key.";
    } catch (error) {
      described.problem = `The sender's chat key could not be read: ${errorText(error)}`;
      this.emit({
        type: "lookup",
        account: shortHex(peer),
        outcome: "error",
        reason: sanitizeReason(errorText(error)),
      });
    }
    return described;
  }

  private contact(account: HexString): Contact | undefined {
    return this.record.contacts.find((contact) => contact.account === account);
  }

  /** The pair's session topics, asked of the identity once per peer and chat key. */
  private async topicsOf(contact: Contact): Promise<PairTopics> {
    const key = `${contact.account}:${contact.chatKey}`;
    let topics = this.topics.get(key);
    if (topics === undefined) {
      topics = await this.deps.identity.sessionTopics(
        hexToBytes(contact.account),
        hexToBytes(contact.chatKey),
      );
      this.topics.set(key, topics);
    }
    return topics;
  }

  /** Subscribe to every contact's incoming session, again whenever the contacts change. */
  private async listenToSessions(): Promise<void> {
    const store = this.store;
    if (store === null) return;
    const topics: Uint8Array[] = [];
    for (const contact of this.record.contacts)
      if (contact.blocked !== true)
        topics.push((await this.topicsOf(contact)).incoming);
    if (this.store !== store) return;
    const topicIds = topics.map(bytesToHex);
    const fingerprint = topicIds.join();
    if (fingerprint === this.sessionTopics) return;
    if (this.stopSessions !== null) {
      this.stopSessions();
      this.emit(
        subscriptionEvent(
          "sessions",
          "unsubscribe",
          this.sessionTopics.split(",").filter(Boolean),
        ),
      );
    }
    this.stopSessions = null;
    this.sessionTopics = fingerprint;
    if (topics.length === 0) return;
    this.stopSessions = await store.subscribe(
      { matchAny: topics },
      (statements) => void this.serial(() => this.receiveSessions(statements)),
    );
    this.emit(subscriptionEvent("sessions", "subscribe", topicIds));
  }

  private async receiveSessions(statements: Uint8Array[]): Promise<void> {
    for (const encoded of statements) {
      const hash = this.received("sessions", encoded);
      try {
        const statement = decodeStatement(encoded);
        if (!verifyStatement(statement))
          throw new Error("its proof does not verify");
        const topic = statement.topics[0];
        let contact: Contact | undefined;
        for (const candidate of this.record.contacts)
          if (
            candidate.blocked !== true &&
            topic !== undefined &&
            equalBytes((await this.topicsOf(candidate)).incoming, topic)
          ) {
            contact = candidate;
            break;
          }
        if (contact === undefined) {
          this.deps.payloads?.gap(
            hash,
            "no contact's session topic matches, so there is no key to decrypt with",
          );
          this.emit({
            type: "set-aside",
            stream: "sessions",
            reason: "no contact's session topic matches",
            statement: hash,
          });
          continue;
        }
        const plaintext = await this.decrypt(hash, () =>
          this.deps.identity.openSession(
            hexToBytes(contact.chatKey),
            statement.data,
          ),
        );
        await this.receiveSessionData(
          contact,
          decodeSessionData(plaintext),
          hash,
        );
      } catch (error) {
        this.skip(`A session statement could not be read: ${errorText(error)}`);
        this.emit({
          type: "set-aside",
          stream: "sessions",
          reason: sanitizeReason(errorText(error)),
          statement: hash,
        });
      }
    }
    this.changed();
  }

  private async receiveSessionData(
    contact: Contact,
    data: SessionData,
    statement: string,
  ): Promise<void> {
    const peer = shortHex(contact.account);
    switch (data.kind) {
      case "multi-device":
        this.emit({ type: "session", peer, data: "multi-device", statement });
        this.skip(
          `${contact.name ?? contact.account} sent a multi-device envelope, which this host does not open.`,
        );
        return;
      case "response": {
        const matched = contact.outbound?.requestId === data.requestId;
        this.emit({
          type: "session",
          peer,
          data: "response",
          requestId: shortId(data.requestId),
          code: data.code,
          matched,
          statement,
        });
        if (
          data.code === ACKNOWLEDGED &&
          contact.outbound?.requestId === data.requestId
        ) {
          const delivered = new Set(contact.outbound.messageIds);
          for (const message of contact.messages)
            if (delivered.has(message.id)) {
              message.status = "acknowledged";
              if (message.call !== undefined) delete message.call.setup;
            }
          contact.pending = (contact.pending ?? []).filter(
            (pending) => !delivered.has(pending.id),
          );
          contact.outbound = null;
          if (this.writesAllowed() && this.outgoing(contact).length > 0)
            await this.flush(contact);
        }
        return;
      }
      case "request": {
        contact.state = "active";
        let duplicates = 0;
        for (const encoded of data.messages)
          if (this.receiveMessage(contact, encoded) === "duplicate")
            duplicates += 1;
        this.emit({
          type: "session",
          peer,
          data: "request",
          requestId: shortId(data.requestId),
          kinds: data.messages.map(messageKind),
          duplicates,
          statement,
        });
        await this.acknowledge(contact, data.requestId);
      }
    }
  }

  /** Add one received message to the conversation, and say what became of it. */
  private receiveMessage(
    contact: Contact,
    encoded: Uint8Array,
  ): "added" | "duplicate" | "control" | "unreadable" {
    let message: ChatMessage;
    try {
      message = decodeMessage(encoded);
    } catch (error) {
      this.skip(
        `A message from ${contact.name ?? contact.account} could not be read: ${errorText(error)}`,
      );
      return "unreadable";
    }
    if (message.content.kind === "reaction")
      return this.receiveReaction(contact, message.id, message.content);
    if (
      message.content.kind === "call-answer" ||
      message.content.kind === "call-candidates" ||
      message.content.kind === "call-closed"
    )
      return this.receiveCallSignal(
        contact,
        message.id,
        Number(message.timestampMs),
        message.content,
      );
    if (contact.messages.some((known) => known.id === message.id))
      return "duplicate";
    const base = {
      id: message.id,
      direction: "in" as const,
      at: Number(message.timestampMs),
      status: "received" as const,
    };
    switch (message.content.kind) {
      case "text":
        contact.messages.push({
          ...base,
          kind: "text",
          text: message.content.text,
        });
        return "added";
      case "rich":
        contact.messages.push({
          ...base,
          kind: "text",
          text: message.content.text ?? "",
          attachments: message.content.attachments.map(storedAttachment),
        });
        return "added";
      case "accepted":
        contact.messages.push({
          ...base,
          kind: "accepted",
          text: "Accepted the chat request",
        });
        return "added";
      case "call-offer":
        contact.messages.push({
          ...base,
          kind: "call",
          text: "",
          call: { video: message.content.video },
        });
        this.emitCall({
          account: contact.account,
          id: message.id,
          at: base.at,
          content: message.content,
        });
        return "added";
      case "left":
        contact.peerLeft = true;
        contact.messages.push({ ...base, kind: "left", text: "" });
        return "added";
      case "contact-added":
        contact.peerLeft = false;
        return "control";
      case "control":
        // A peer's push token and device list. This host sends no push
        // notifications and talks to the peer pairwise, so neither is kept.
        return "control";
      case "other":
        contact.messages.push({
          ...base,
          kind: "unsupported",
          text: `A message this host does not show yet (kind ${message.content.tag})`,
        });
        return "added";
    }
  }

  /**
   * Apply the peer's reaction to the message it names. Each emoji is kept
   * once per side, as the apps key it; a removal takes that emoji away. A
   * reaction to a message this host does not have is not kept.
   */
  private receiveReaction(
    contact: Contact,
    id: string,
    reaction: Extract<MessageContent, { kind: "reaction" }>,
  ): "added" | "duplicate" | "control" {
    if (this.seenBefore(contact, id)) return "duplicate";
    const target = contact.messages.find(
      (message) => message.id === reaction.messageId,
    );
    if (target === undefined) return "control";
    const others = (target.reactions ?? []).filter(
      (entry) => !(entry.from === "peer" && entry.emoji === reaction.emoji),
    );
    target.reactions = reaction.removed
      ? others
      : [...others, { emoji: reaction.emoji, from: "peer" }];
    return "added";
  }

  /** Whether a signal was handled before; remembers it when not. */
  private seenBefore(contact: Contact, id: string): boolean {
    const seen = contact.seenSignals ?? [];
    if (seen.includes(id)) return true;
    contact.seenSignals = [...seen, id].slice(-SEEN_SIGNALS_LIMIT);
    return false;
  }

  /** Note what an answer or an end does to the call's line, and hand the signal on. */
  private receiveCallSignal(
    contact: Contact,
    id: string,
    at: number,
    content: Extract<
      MessageContent,
      { kind: "call-answer" | "call-candidates" | "call-closed" }
    >,
  ): "added" | "duplicate" | "control" {
    if (this.seenBefore(contact, id)) return "duplicate";
    const call = contact.messages.find(
      (message) => message.kind === "call" && message.id === content.offerId,
    )?.call;
    if (call !== undefined && content.kind === "call-answer")
      call.answeredAt ??= at;
    if (
      call !== undefined &&
      content.kind === "call-closed" &&
      call.closedBy === undefined
    ) {
      call.closedBy = "peer";
      call.endedAt = at;
    }
    this.emitCall({ account: contact.account, id, at, content });
    return content.kind === "call-candidates" ? "control" : "added";
  }

  private emitCall(signal: IncomingCallSignal): void {
    for (const listener of [...this.callListeners]) listener(signal);
  }

  /** Follow the call signals peers send. */
  onCall(listener: (signal: IncomingCallSignal) => void): () => void {
    this.callListeners.add(listener);
    return () => this.callListeners.delete(listener);
  }

  // -------------------------------------------------------------------------
  // Sending
  // -------------------------------------------------------------------------

  private writesAllowed(): boolean {
    return this.deps.writes?.() === true;
  }

  /** Refuse a user action that would publish while writes are off, before it changes anything. */
  private requireWrites(): void {
    if (!this.writesAllowed()) throw new Error(WRITES_OFF);
  }

  /** Check this engine may publish, then build the statement with `build` and submit it. */
  private async submit(
    build: () => Promise<Built>,
    context: SubmitContext,
    stream: CaptureStream,
  ): Promise<void> {
    let reported = false;
    let hash: string | undefined;
    const report = (
      result: "stored" | "refused" | "error",
      details: { status?: string; reason?: string },
    ) => {
      reported = true;
      this.emit({
        type: "submit",
        ...context,
        ...(context.requestId !== undefined
          ? { requestId: shortId(context.requestId) }
          : {}),
        result,
        ...details,
        ...(hash !== undefined ? { statement: hash } : {}),
      });
    };
    try {
      if (!this.writesAllowed()) throw new Error(WRITES_OFF);
      if (this.canSend === false) throw new Error(NO_ALLOWANCE);
      const store = this.store;
      if (store === null)
        throw new Error("Chat is not connected to the People chain.");
      const { statement: encoded, plaintext } = await build();
      hash = bytesToHex(statementHash(encoded));
      this.deps.payloads?.statement("out", stream, encoded);
      this.deps.payloads?.plaintext(hash, withoutTickets(plaintext));
      const result = await store.submit(encoded);
      if (!isStored(result)) {
        report("refused", {
          status: sanitizeReason(result.status),
          ...(result.reason ? { reason: sanitizeReason(result.reason) } : {}),
        });
        throw new Error(
          `The statement store did not keep the statement: ${result.status}${result.reason ? ` (${result.reason})` : ""}.`,
        );
      }
      report("stored", { status: sanitizeReason(result.status) });
    } catch (error) {
      if (!reported)
        report("error", { reason: sanitizeReason(errorText(error)) });
      throw error;
    }
  }

  /** Every message the peer has not acknowledged: bubbles first, then pending reactions. */
  private outgoing(contact: Contact): Outgoing[] {
    const bubbles: Outgoing[] = contact.messages
      .filter(
        (message) =>
          message.direction === "out" &&
          (message.kind === "text" ||
            message.kind === "accepted" ||
            (message.kind === "call" &&
              message.call?.setup !== undefined &&
              message.call.closedBy === undefined)) &&
          message.status !== "acknowledged" &&
          uploaded(message),
      )
      .map((message) => ({
        id: message.id,
        at: message.at,
        content:
          message.kind === "accepted"
            ? { kind: "accepted", requestId: message.text }
            : message.kind === "call"
              ? {
                  kind: "call-offer",
                  sdp: hexToBytes(message.call!.setup!),
                  video: message.call!.video,
                }
              : message.attachments !== undefined
                ? {
                    kind: "rich",
                    text: message.text === "" ? null : message.text,
                    attachments: message.attachments.map(wireAttachment),
                  }
                : { kind: "text", text: message.text },
        message,
      }));
    const now = this.now();
    const signals = (contact.pending ?? [])
      .filter(
        (pending) =>
          pending.content.kind === "reaction" ||
          now - pending.at < CALL_SIGNAL_TTL_MS,
      )
      .map((pending: PendingMessage): Outgoing => {
        const content = pending.content;
        return {
          id: pending.id,
          at: pending.at,
          content:
            content.kind === "call-answer"
              ? {
                  kind: "call-answer",
                  offerId: content.offerId,
                  sdp: hexToBytes(content.sdp),
                }
              : content.kind === "call-candidates"
                ? {
                    kind: "call-candidates",
                    offerId: content.offerId,
                    sdp: hexToBytes(content.sdp),
                  }
                : content,
        };
      });
    return [...bubbles, ...signals];
  }

  /**
   * Put every unacknowledged message, and `extra`, in one request on this
   * host's request channel. A newer request replaces the one before it, as
   * the app does. Resolves to whether the store kept it.
   */
  private async flush(
    contact: Contact,
    extra: Outgoing[] = [],
  ): Promise<boolean> {
    const outgoing = [...this.outgoing(contact), ...extra];
    if (outgoing.length === 0) return true;
    const messages = outgoing.flatMap((entry) =>
      entry.message === undefined ? [] : [entry.message],
    );
    const requestId = this.newId();
    const encoded = outgoing.map((entry) =>
      encodeMessage({
        id: entry.id,
        timestampMs: BigInt(entry.at),
        content: entry.content,
      }),
    );
    const expiry = nextExpiry(
      this.lastExpiry(contact.expiry.request),
      this.now(),
    );
    const plaintext = encodeSessionData({
      kind: "request",
      requestId,
      messages: encoded,
    });
    try {
      await this.submit(
        () => this.sessionStatement(contact, plaintext, expiry),
        {
          purpose: extra.some((entry) => entry.content.kind === "left")
            ? "leave"
            : messages.some((message) => message.kind === "accepted")
              ? "accept"
              : "messages",
          peer: shortHex(contact.account),
          requestId,
          messages: outgoing.length,
        },
        "sessions",
      );
      contact.expiry.request = expiry.toString();
      contact.outbound = {
        requestId,
        messageIds: outgoing.map((entry) => entry.id),
      };
      for (const message of messages) {
        message.status = "sent";
        delete message.problem;
      }
      return true;
    } catch (error) {
      for (const message of messages) {
        message.status = "failed";
        message.problem = errorText(error);
      }
      return false;
    }
  }

  private async sessionStatement(
    contact: Contact,
    plaintext: Uint8Array,
    expiry: bigint,
  ): Promise<Built> {
    const statement = await this.deps.identity.sessionStatement({
      peer: hexToBytes(contact.account),
      peerChatKey: hexToBytes(contact.chatKey),
      plaintext,
      expiry,
    });
    return { statement, plaintext };
  }

  private lastExpiry(value: string | null): bigint | null {
    return value === null ? null : BigInt(value);
  }

  private async acknowledge(
    contact: Contact,
    requestId: string,
  ): Promise<void> {
    if (
      contact.acknowledged === requestId ||
      this.canSend === false ||
      !this.writesAllowed()
    )
      return;
    const expiry = nextExpiry(
      this.lastExpiry(contact.expiry.response),
      this.now(),
    );
    const plaintext = encodeSessionData({
      kind: "response",
      requestId,
      code: ACKNOWLEDGED,
    });
    try {
      await this.submit(
        () => this.sessionStatement(contact, plaintext, expiry),
        { purpose: "ack", peer: shortHex(contact.account), requestId },
        "sessions",
      );
      contact.expiry.response = expiry.toString();
      contact.acknowledged = requestId;
    } catch (error) {
      this.skip(
        `Could not acknowledge ${contact.name ?? contact.account}: ${errorText(error)}`,
      );
    }
  }

  /** Accept a request: open the pairwise session and tell the requester. */
  accept(requestId: string): Promise<void> {
    return this.userAction("accept", async () => {
      this.requireWrites();
      const request = this.record.requests.find(
        (candidate) => candidate.id === requestId,
      );
      if (request === undefined)
        throw new Error("That request is no longer here.");
      if (request.problem !== null || request.peerChatKey === null)
        throw new Error(request.problem ?? "The sender has no chat key.");
      const opened: StoredMessage[] = [
        {
          id: request.id,
          direction: "in",
          kind: "request",
          text: request.welcome ?? "",
          at: request.at,
          status: "received",
        },
        {
          id: this.newId(),
          direction: "out",
          kind: "accepted",
          text: request.id,
          at: this.now(),
          status: "pending",
        },
      ];
      // A peer who left and asked again keeps the conversation's history.
      const returning = this.contact(request.peer);
      const contact: Contact = returning ?? {
        account: request.peer,
        chatKey: request.peerChatKey,
        name: request.name,
        state: "active",
        messages: [],
        outbound: null,
        expiry: { request: null, response: null },
        acknowledged: null,
      };
      contact.chatKey = request.peerChatKey;
      contact.name = request.name ?? contact.name;
      contact.state = "active";
      contact.peerLeft = false;
      contact.messages.push(...opened);
      this.record.requests = this.record.requests.filter(
        (candidate) => candidate.id !== requestId,
      );
      if (returning === undefined) this.record.contacts.push(contact);
      await this.listenToSessions();
      await this.flush(contact);
      this.changed();
    });
  }

  /** Forget a request here. Nothing is sent, and the requester is not told. */
  decline(requestId: string): void {
    this.record.requests = this.record.requests.filter(
      (candidate) => candidate.id !== requestId,
    );
    this.record.declined.push(requestId);
    this.changed();
  }

  /**
   * Send a chat request to an account, named by its lite label or as a
   * 0x-prefixed account id, with `welcome` as its first message.
   */
  startChat(target: string, welcome: string): Promise<void> {
    return this.userAction("start chat", async () => {
      this.requireWrites();
      const account = await this.resolve(target.trim());
      const peer = bytesToHex(account);
      if (equalBytes(account, this.me))
        throw new Error("That is this identity.");
      if (this.contact(peer) !== undefined)
        throw new Error("There is already a chat with that account.");
      const identity = await lookupChatIdentity(this.deps.directory, account);
      if (identity?.conflict)
        throw new Error(
          `${target} has different chat keys on Asset Hub and on the People chain.`,
        );
      const chatKey = identity?.chatKey ?? null;
      if (chatKey === null)
        throw new Error(`${target} has no chat key on chain.`);
      const request = {
        id: this.newId(),
        timestampMs: BigInt(this.now()),
        content: {
          version: 1 as const,
          welcome: welcome === "" ? null : welcome,
        },
      };
      const now = this.now();
      await this.submit(
        () =>
          this.deps.identity.requestStatement({
            peer: account,
            peerChatKey: chatKey,
            id: request.id,
            timestampMs: request.timestampMs,
            welcome: request.content.welcome,
            expiry: expiryAt(now),
            day: requestDay(now),
          }),
        {
          purpose: "chat request",
          peer: shortHex(peer),
          requestId: request.id,
        },
        "requests",
      );
      this.record.contacts.push({
        account: peer,
        chatKey: bytesToHex(chatKey),
        name: identity?.name ?? null,
        state: "requested",
        messages: [
          {
            id: request.id,
            direction: "out",
            kind: "request",
            text: welcome,
            at: Number(request.timestampMs),
            status: "sent",
          },
        ],
        outbound: null,
        expiry: { request: null, response: null },
        acknowledged: null,
      });
      await this.listenToSessions();
      this.changed();
    });
  }

  private async resolve(target: string): Promise<Uint8Array> {
    if (/^0x[0-9a-fA-F]{64}$/.test(target)) return hexToBytes(target);
    if (target === "") throw new Error("Enter a username or an account id.");
    const owner = await lookupOwner(this.deps.directory, target);
    if (owner === null)
      throw new Error(`No account holds the username ${target}.`);
    return owner;
  }

  /** Send text to an accepted contact. */
  send(account: HexString, text: string): Promise<void> {
    return this.userAction("send", async () => {
      this.requireWrites();
      const contact = this.activeContact(account);
      contact.messages.push({
        id: this.newId(),
        direction: "out",
        kind: "text",
        text,
        at: this.now(),
        status: "pending",
      });
      this.changed();
      await this.flush(contact);
      this.changed();
    });
  }

  /**
   * Set the user's reaction on one of a contact's messages, or take it away
   * with `null`. The user keeps one emoji per message, as the iOS app does:
   * a new one is sent first, then the old one is removed; the same one again
   * removes it.
   */
  react(
    account: HexString,
    messageId: string,
    emoji: string | null,
  ): Promise<void> {
    return this.userAction("react", async () => {
      this.requireWrites();
      const contact = this.activeContact(account);
      const target = contact.messages.find(
        (message) => message.id === messageId,
      );
      if (target === undefined || !reactable(target))
        throw new Error("That message cannot take a reaction.");
      const mine = (target.reactions ?? []).filter(
        (entry) => entry.from === "self",
      );
      const placing =
        emoji !== null && !mine.some((entry) => entry.emoji === emoji)
          ? emoji
          : null;
      const pending = (reaction: string, removed: boolean): PendingMessage => ({
        id: this.newId(),
        at: this.now(),
        content: { kind: "reaction", messageId, emoji: reaction, removed },
      });
      contact.pending = [
        ...(contact.pending ?? []),
        ...(placing === null ? [] : [pending(placing, false)]),
        ...mine.map((entry) => pending(entry.emoji, true)),
      ];
      target.reactions = [
        ...(target.reactions ?? []).filter((entry) => entry.from !== "self"),
        ...(placing === null
          ? []
          : [{ emoji: placing, from: "self" as const }]),
      ];
      this.changed();
      await this.flush(contact);
      this.changed();
    });
  }

  private activeContact(account: HexString): Contact {
    const contact = this.contact(account);
    if (contact === undefined)
      throw new Error("There is no chat with that account.");
    if (contact.blocked) throw new Error("You blocked this person.");
    if (contact.state !== "active")
      throw new Error("The request has not been accepted yet.");
    if (contact.peerLeft) throw new Error("They left this chat.");
    return contact;
  }

  /**
   * Block a contact here. Nothing is sent: the conversation leaves the list,
   * its session is no longer followed, and nothing the person sends is read
   * or acknowledged. Its history stays, for when they are unblocked.
   */
  block(account: HexString): Promise<void> {
    return this.setBlocked(account, true);
  }

  /** Follow a blocked contact again. Messages still in the store are read then. */
  unblock(account: HexString): Promise<void> {
    return this.setBlocked(account, false);
  }

  private setBlocked(account: HexString, blocked: boolean): Promise<void> {
    return this.userAction(blocked ? "block" : "unblock", async () => {
      const contact = this.contact(account);
      if (contact === undefined)
        throw new Error("There is no chat with that account.");
      if (blocked) contact.blocked = true;
      else delete contact.blocked;
      await this.listenToSessions();
      this.changed();
    });
  }

  /**
   * Leave a conversation: tell the peer (kind 13), then stop following it
   * and take it off the list. The conversation is kept in the record, not
   * deleted. Nothing changes when the store does not keep the message.
   */
  leave(account: HexString): Promise<void> {
    return this.userAction("leave", async () => {
      this.requireWrites();
      const contact = this.contact(account);
      if (contact === undefined)
        throw new Error("There is no chat with that account.");
      if (contact.state !== "active")
        throw new Error("The request has not been accepted yet.");
      const left: Outgoing = {
        id: this.newId(),
        at: this.now(),
        content: { kind: "left" },
      };
      if (!(await this.flush(contact, [left]))) {
        this.changed();
        throw new Error(
          `The message that you left was not sent: ${contact.messages.find((message) => message.status === "failed")?.problem ?? "the statement store did not keep it"}`,
        );
      }
      contact.messages.push({
        id: left.id,
        direction: "out",
        kind: "left",
        text: "",
        at: left.at,
        status: "sent",
      });
      this.record.contacts = this.record.contacts.filter(
        (candidate) => candidate !== contact,
      );
      this.record.left.push(contact);
      await this.listenToSessions();
      this.changed();
    });
  }

  /**
   * Send a call signal. An offer becomes the call's line in the
   * conversation, and its id is the call's; the rest go as pending signals,
   * sent again until acknowledged or stale. Resolves to the message id.
   */
  sendCall(account: HexString, signal: OutgoingCallSignal): Promise<string> {
    return this.serial(async () => {
      this.requireWrites();
      const contact = this.activeContact(account);
      const id = this.newId();
      const at = this.now();
      if (signal.kind === "call-offer")
        contact.messages.push({
          id,
          direction: "out",
          kind: "call",
          text: "",
          at,
          status: "pending",
          call: { video: signal.video, setup: bytesToHex(signal.sdp) },
        });
      else {
        if (signal.kind === "call-closed") {
          const call = contact.messages.find(
            (message) =>
              message.kind === "call" && message.id === signal.offerId,
          )?.call;
          if (call !== undefined && call.closedBy === undefined) {
            call.closedBy = "self";
            call.endedAt = at;
          }
        }
        if (signal.kind === "call-answer") {
          const call = contact.messages.find(
            (message) =>
              message.kind === "call" && message.id === signal.offerId,
          )?.call;
          if (call !== undefined) call.answeredAt ??= at;
        }
        contact.pending = [
          ...(contact.pending ?? []),
          {
            id,
            at,
            content:
              signal.kind === "call-closed"
                ? signal
                : signal.kind === "call-answer"
                  ? {
                      kind: "call-answer",
                      offerId: signal.offerId,
                      sdp: bytesToHex(signal.sdp),
                    }
                  : {
                      kind: "call-candidates",
                      offerId: signal.offerId,
                      sdp: bytesToHex(signal.sdp),
                    },
          },
        ];
      }
      this.changed();
      if (!(await this.flush(contact))) {
        this.changed();
        throw new Error("The call signal was not sent.");
      }
      this.changed();
      return id;
    });
  }

  /** Why this engine cannot send files now, or null. */
  fileProblem(): string | null {
    const hop = this.deps.hop;
    if (hop === undefined || hop.nodes().length === 0)
      return "No HOP node is configured for this network, so files cannot be sent or fetched.";
    if (hop.sender === null)
      return "This host has no account to submit files with.";
    return null;
  }

  /**
   * Send a file: show it at once as pending, seal and submit it to a HOP
   * node, then send the message that names it. The upload runs outside the
   * engine's queue, so receiving goes on meanwhile; {@link cancel} or
   * {@link stop} ends it. The bytes are not kept: a failed upload is sent
   * again by picking the file again.
   */
  async sendFile(
    account: HexString,
    file: OutgoingFile,
    caption: string,
  ): Promise<void> {
    const problem = this.fileProblem();
    if (problem !== null) throw new Error(problem);
    if (file.bytes.length > HOP_MAX_FILE_BYTES)
      throw new Error("The file is larger than 128 MiB.");
    const hop = this.deps.hop!;
    const nodes = hop.nodes();
    const node = nodes[Math.floor(Math.random() * nodes.length)];
    const id = this.newId();
    const controller = new AbortController();
    let message!: StoredMessage;
    let run = 0;
    await this.userAction("send file", async () => {
      this.requireWrites();
      const contact = this.activeContact(account);
      message = {
        id,
        direction: "out",
        kind: "text",
        text: caption,
        at: this.now(),
        status: "pending",
        attachments: [
          {
            node,
            kind: file.image === undefined ? "file" : "image",
            mimeType: file.mimeType || "application/octet-stream",
            size: file.bytes.length,
            ...file.image,
            name: file.name,
          },
        ],
      };
      contact.messages.push(message);
      this.transfers.set(id, controller);
      run = this.run;
      this.changed();
    });
    let reference: { identifier: Uint8Array; ticket: Uint8Array };
    try {
      const connection = await hop.connect(node);
      try {
        reference = await hopUpload(
          connection,
          file.bytes,
          hop.sender!,
          { random: this.random, now: this.now },
          { signal: controller.signal },
        );
      } finally {
        connection.close();
      }
    } catch (error) {
      this.transfers.delete(id);
      await this.serial(async () => {
        message.status = "failed";
        message.problem = controller.signal.aborted
          ? "Canceled."
          : `The file was not uploaded: ${errorText(error)}`;
        this.changed();
      });
      this.emit({
        type: "action-failed",
        action: "upload",
        reason: sanitizeReason(errorText(error)),
      });
      throw error;
    }
    this.transfers.delete(id);
    await this.userAction("send file", async () => {
      const attachment = message.attachments![0];
      attachment.identifier = bytesToHex(reference.identifier);
      attachment.ticket = bytesToHex(reference.ticket);
      const contact = this.contact(account);
      if (run !== this.run || contact === undefined) {
        message.status = "failed";
        message.problem = "Chat stopped before the file was sent.";
        this.changed();
        return;
      }
      await this.flush(contact);
      this.changed();
    });
  }

  /** Why a file a message names cannot be fetched here, or null. */
  fetchProblem(attachment: StoredAttachment): string | null {
    if (attachment.identifier === undefined || attachment.ticket === undefined)
      return "Not uploaded.";
    const nodes = this.deps.hop?.nodes() ?? [];
    if (nodes.length === 0)
      return "No HOP node is configured for this network.";
    if (!knownNode(nodes, attachment.node ?? ""))
      return "On a HOP node this host is not configured with.";
    return null;
  }

  /** End a file transfer this host started. */
  cancel(messageId: string): void {
    this.transfers.get(messageId)?.abort();
  }

  /**
   * Fetch and open a file a message names, from the node it names, only when
   * that node is one this host was configured with. Nothing is acknowledged,
   * so the file stays in the pool for the other side's devices.
   */
  async fetchFile(
    account: HexString,
    messageId: string,
    index: number,
    signal?: AbortSignal,
  ): Promise<Uint8Array> {
    const hop = this.deps.hop;
    const attachment = this.contact(account)?.messages.find(
      (message) => message.id === messageId,
    )?.attachments?.[index];
    if (attachment === undefined) throw new Error("That file is not here.");
    const problem = this.fetchProblem(attachment);
    if (problem !== null || hop === undefined)
      throw new Error(`The file is not fetched: ${problem}`);
    const controller = new AbortController();
    const key = `fetch:${messageId}:${index}`;
    this.transfers.get(key)?.abort();
    this.transfers.set(key, controller);
    const abort = () => controller.abort();
    signal?.addEventListener("abort", abort);
    try {
      const connection = await hop.connect(attachment.node!);
      try {
        return await hopDownload(
          connection,
          {
            identifier: hexToBytes(attachment.identifier!),
            ticket: hexToBytes(attachment.ticket!),
          },
          HOP_MAX_FILE_BYTES,
          { signal: controller.signal },
        );
      } finally {
        connection.close();
      }
    } finally {
      signal?.removeEventListener("abort", abort);
      if (this.transfers.get(key) === controller) this.transfers.delete(key);
    }
  }

  /** Send the unacknowledged messages of a contact again. */
  retry(account: HexString): Promise<void> {
    return this.userAction("retry", async () => {
      this.requireWrites();
      const contact = this.contact(account);
      if (contact === undefined) return;
      await this.flush(contact);
      this.changed();
    });
  }
}

/** Whether a message is a bubble a reaction can name. */
export const reactable = (message: StoredMessage): boolean =>
  message.kind === "text" || message.kind === "request";

/** Whether every file of a message has been uploaded, so the message can be sent. */
const uploaded = (message: StoredMessage): boolean =>
  (message.attachments ?? []).every(
    (attachment) => attachment.identifier !== undefined,
  );

function storedAttachment(attachment: Attachment): StoredAttachment {
  const meta = attachment.meta;
  return {
    identifier: bytesToHex(attachment.identifier),
    ticket: bytesToHex(attachment.ticket),
    node: attachment.node,
    kind: meta.kind,
    mimeType: meta.mimeType,
    size: meta.size,
    ...(meta.kind === "image"
      ? { width: meta.width, height: meta.height }
      : {}),
    ...(meta.kind === "video" ? { duration: meta.duration } : {}),
  };
}

function wireAttachment(attachment: StoredAttachment): Attachment {
  const general = { mimeType: attachment.mimeType, size: attachment.size };
  return {
    identifier: hexToBytes(attachment.identifier!),
    ticket: hexToBytes(attachment.ticket!),
    node: attachment.node!,
    meta:
      attachment.kind === "image"
        ? {
            kind: "image",
            ...general,
            width: attachment.width ?? 0,
            height: attachment.height ?? 0,
            thumbnail: null,
          }
        : attachment.kind === "video"
          ? {
              kind: "video",
              ...general,
              duration: attachment.duration ?? 0,
              thumbnail: null,
            }
          : { kind: "file", ...general },
  };
}
