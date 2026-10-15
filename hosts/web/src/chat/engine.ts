import { bytesToHex, hexToBytes } from "@parity/truapi/scale";
import type { HexString } from "@parity/truapi/scale";
import { equalBytes } from "./codec.js";
import { cryptoRandom, type RandomBytes } from "./crypto.js";
import {
  sanitizeReason,
  shortHex,
  shortId,
  type ChatEventBody,
  type ChatEventSink,
} from "./diagnostics.js";
import {
  lookupChatIdentity,
  lookupOwner,
  type ChainDirectory,
} from "./directory.js";
import type { ChatKeys } from "./keys.js";
import {
  ACKNOWLEDGED,
  checkIdentityProof,
  decodeMessage,
  encodeMessage,
  messageKind,
  openRequest,
  openSession,
  requestChannel,
  requestDayTopic,
  requestSessionTopic,
  requestTopic,
  responseChannel,
  sealRequest,
  sealSession,
  sessionTopic,
  type ChatMessage,
  type SessionData,
} from "./protocol.js";
import {
  decodeStatement,
  encodeSigned,
  expiryAt,
  nextExpiry,
  requestDay,
  verifyStatement,
  type Statement,
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
  keys: ChatKeys;
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
}

/** Why a submission was made, for diagnostics. */
interface SubmitContext {
  purpose: "chat request" | "accept" | "messages" | "ack";
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
  private queue: Promise<void> = Promise.resolve();
  /** Bumped by every start and stop, so work from an earlier run ends quietly. */
  private run = 0;
  private retryTimer: ReturnType<typeof setTimeout> | null = null;
  private failures = 0;
  private readonly listeners = new Set<() => void>();
  private readonly now: () => number;
  private readonly random: RandomBytes;
  private readonly newId: () => string;

  constructor(private readonly deps: ChatEngineDeps) {
    this.record = loadChatRecord(deps.storage, deps.recordKey);
    this.now = deps.now ?? Date.now;
    this.random = deps.random ?? cryptoRandom;
    this.newId = deps.newId ?? (() => crypto.randomUUID());
  }

  private get me(): Uint8Array {
    return this.deps.keys.identity;
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
      chatKey: bytesToHex(this.deps.keys.chatPublic),
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
      this.stopRequests = await opened.subscribe(
        { matchAll: [requestTopic(this.me)] },
        (statements) =>
          void this.serial(() => this.receiveRequests(statements)),
      );
      this.emit({
        type: "subscription",
        stream: "requests",
        action: "subscribe",
        topics: 1,
      });
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
          : equalBytes(onChain, this.deps.keys.chatPublic)
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

  private async receiveRequests(statements: Uint8Array[]): Promise<void> {
    for (const encoded of statements) {
      try {
        const statement = decodeStatement(encoded);
        if (!verifyStatement(statement))
          throw new Error("its proof does not verify");
        const request = openRequest(statement.data, this.me, (ephemeral) =>
          this.deps.keys.agree(ephemeral),
        );
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
        if (this.contact(peer)?.state === "active") {
          seen("contact active");
          continue;
        }
        const waiting = this.contact(peer);
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
        });
      }
    }
    this.changed();
  }

  private async describeRequest(
    request: ReturnType<typeof openRequest>,
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
      if (!checkIdentityProof(request, this.deps.keys.agree(chatKey)))
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

  private secretWith(contact: Contact): Uint8Array {
    return this.deps.keys.agree(hexToBytes(contact.chatKey));
  }

  private incomingTopic(contact: Contact): Uint8Array {
    return sessionTopic(
      this.secretWith(contact),
      hexToBytes(contact.account),
      this.me,
    );
  }

  private outgoingTopic(contact: Contact): Uint8Array {
    return sessionTopic(
      this.secretWith(contact),
      this.me,
      hexToBytes(contact.account),
    );
  }

  /** Subscribe to every contact's incoming session, again whenever the contacts change. */
  private async listenToSessions(): Promise<void> {
    const store = this.store;
    if (store === null) return;
    const topics = this.record.contacts.map((contact) =>
      this.incomingTopic(contact),
    );
    const fingerprint = topics.map(bytesToHex).join();
    if (fingerprint === this.sessionTopics) return;
    if (this.stopSessions !== null) {
      this.stopSessions();
      this.emit({
        type: "subscription",
        stream: "sessions",
        action: "unsubscribe",
        topics: this.sessionTopics.split(",").filter(Boolean).length,
      });
    }
    this.stopSessions = null;
    this.sessionTopics = fingerprint;
    if (topics.length === 0) return;
    this.stopSessions = await store.subscribe(
      { matchAny: topics },
      (statements) => void this.serial(() => this.receiveSessions(statements)),
    );
    this.emit({
      type: "subscription",
      stream: "sessions",
      action: "subscribe",
      topics: topics.length,
    });
  }

  private async receiveSessions(statements: Uint8Array[]): Promise<void> {
    for (const encoded of statements) {
      try {
        const statement = decodeStatement(encoded);
        if (!verifyStatement(statement))
          throw new Error("its proof does not verify");
        const topic = statement.topics[0];
        const contact = this.record.contacts.find(
          (candidate) =>
            topic !== undefined &&
            equalBytes(this.incomingTopic(candidate), topic),
        );
        if (contact === undefined) {
          this.emit({
            type: "set-aside",
            stream: "sessions",
            reason: "no contact's session topic matches",
          });
          continue;
        }
        const data = openSession(this.secretWith(contact), statement.data);
        await this.receiveSessionData(contact, data);
      } catch (error) {
        this.skip(`A session statement could not be read: ${errorText(error)}`);
        this.emit({
          type: "set-aside",
          stream: "sessions",
          reason: sanitizeReason(errorText(error)),
        });
      }
    }
    this.changed();
  }

  private async receiveSessionData(
    contact: Contact,
    data: SessionData,
  ): Promise<void> {
    const peer = shortHex(contact.account);
    switch (data.kind) {
      case "multi-device":
        this.emit({ type: "session", peer, data: "multi-device" });
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
        });
        if (
          data.code === ACKNOWLEDGED &&
          contact.outbound?.requestId === data.requestId
        ) {
          for (const message of contact.messages)
            if (contact.outbound.messageIds.includes(message.id))
              message.status = "acknowledged";
          contact.outbound = null;
          if (this.writesAllowed() && this.unacknowledged(contact).length > 0)
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
      case "accepted":
        contact.messages.push({
          ...base,
          kind: "accepted",
          text: "Accepted the chat request",
        });
        return "added";
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

  // -------------------------------------------------------------------------
  // Sending
  // -------------------------------------------------------------------------

  private sign(statement: Statement): Uint8Array {
    return encodeSigned(statement, this.me, (payload) =>
      this.deps.keys.sign(payload),
    );
  }

  private writesAllowed(): boolean {
    return this.deps.writes?.() === true;
  }

  /** Refuse a user action that would publish while writes are off, before it changes anything. */
  private requireWrites(): void {
    if (!this.writesAllowed()) throw new Error(WRITES_OFF);
  }

  private async submit(
    statement: Statement,
    context: SubmitContext,
  ): Promise<void> {
    let reported = false;
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
      });
    };
    try {
      if (!this.writesAllowed()) throw new Error(WRITES_OFF);
      if (this.canSend === false) throw new Error(NO_ALLOWANCE);
      const store = this.store;
      if (store === null)
        throw new Error("Chat is not connected to the People chain.");
      const result = await store.submit(this.sign(statement));
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

  private unacknowledged(contact: Contact): StoredMessage[] {
    return contact.messages.filter(
      (message) =>
        message.direction === "out" &&
        (message.kind === "text" || message.kind === "accepted") &&
        message.status !== "acknowledged",
    );
  }

  /**
   * Put every unacknowledged message in one request on this host's request
   * channel. A newer request replaces the one before it, as the app does.
   */
  private async flush(contact: Contact): Promise<void> {
    const messages = this.unacknowledged(contact);
    if (messages.length === 0) return;
    const requestId = this.newId();
    const encoded = messages.map((message) =>
      encodeMessage({
        id: message.id,
        timestampMs: BigInt(message.at),
        content:
          message.kind === "accepted"
            ? { kind: "accepted", requestId: message.text }
            : { kind: "text", text: message.text },
      }),
    );
    const topic = this.outgoingTopic(contact);
    const expiry = nextExpiry(
      this.lastExpiry(contact.expiry.request),
      this.now(),
    );
    try {
      await this.submit(
        {
          expiry,
          channel: requestChannel(topic),
          topics: [topic],
          data: sealSession(
            this.secretWith(contact),
            { kind: "request", requestId, messages: encoded },
            this.random,
          ),
        },
        {
          purpose: messages.some((message) => message.kind === "accepted")
            ? "accept"
            : "messages",
          peer: shortHex(contact.account),
          requestId,
          messages: messages.length,
        },
      );
      contact.expiry.request = expiry.toString();
      contact.outbound = {
        requestId,
        messageIds: messages.map((message) => message.id),
      };
      for (const message of messages) {
        message.status = "sent";
        delete message.problem;
      }
    } catch (error) {
      for (const message of messages) {
        message.status = "failed";
        message.problem = errorText(error);
      }
    }
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
    const topic = this.outgoingTopic(contact);
    const expiry = nextExpiry(
      this.lastExpiry(contact.expiry.response),
      this.now(),
    );
    try {
      await this.submit(
        {
          expiry,
          channel: responseChannel(topic),
          topics: [topic],
          data: sealSession(
            this.secretWith(contact),
            { kind: "response", requestId, code: ACKNOWLEDGED },
            this.random,
          ),
        },
        { purpose: "ack", peer: shortHex(contact.account), requestId },
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
      const contact: Contact = {
        account: request.peer,
        chatKey: request.peerChatKey,
        name: request.name,
        state: "active",
        messages: [
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
        ],
        outbound: null,
        expiry: { request: null, response: null },
        acknowledged: null,
      };
      this.record.requests = this.record.requests.filter(
        (candidate) => candidate.id !== requestId,
      );
      this.record.contacts.push(contact);
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
      const secret = this.deps.keys.agree(chatKey);
      await this.submit(
        {
          expiry: expiryAt(this.now()),
          channel: null,
          topics: [
            requestDayTopic(account, requestDay(this.now())),
            requestTopic(account),
            requestSessionTopic(secret, this.me, account),
          ],
          data: sealRequest(
            request,
            account,
            chatKey,
            this.me,
            (payload) => this.deps.keys.sign(payload),
            this.random,
          ),
        },
        {
          purpose: "chat request",
          peer: shortHex(peer),
          requestId: request.id,
        },
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
      const contact = this.contact(account);
      if (contact === undefined)
        throw new Error("There is no chat with that account.");
      if (contact.state !== "active")
        throw new Error("The request has not been accepted yet.");
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
