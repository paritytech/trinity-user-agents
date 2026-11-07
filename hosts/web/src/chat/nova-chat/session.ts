import type {
  NovaHostMessage,
  NovaPeer,
  NovaResult,
  NovaState,
  OpenedFrame,
  WireContent,
  WireFrame,
  WireSignedStatement,
  WireStatement,
} from "./contract.js";
import type {
  Conversation,
  ConversationEngine,
  ConversationMessage,
  ContactState,
  StartTarget,
} from "../draft/conversations.js";

/**
 * PROTOTYPE. Nova Chat's conversations in its worker: the session part of
 * the official apps' chat protocol, with no key. Per conversation it keeps
 * the history, the messages the peer has not acknowledged and the
 * conversation account's derivation index.
 *
 * As the apps do (Android `RealCommunicationSession`, `StatementExpiry`):
 *
 * - every unacknowledged message of ours travels in one request frame on the
 *   request channel of our device-route topic, replaced with a higher expiry
 *   whenever it changes, under a new request id;
 * - every peer request frame is acknowledged with a response frame on our
 *   response channel, again each time it is read, and its messages are kept
 *   once by message id;
 * - a peer response with code 0 acknowledges the messages of that request.
 *
 * The host seals and opens each frame (`nova-chat/1`, `./contract.ts`); this
 * signs and submits as the conversation's own product account.
 */

/** The host's end of `nova-chat/1`, as the worker uses it. */
export interface NovaHostPort {
  call(call: Record<string, unknown> & { call: string }): Promise<NovaResult>;
  onState(listener: (peer: NovaPeer) => void): void;
}

/** The worker's TrUAPI Statement Store, as Nova Chat uses it. */
export interface NovaStatements {
  /** Follow `topics` until the returned function is called. */
  follow(
    topics: string[],
    deliver: (statements: WireSignedStatement[]) => void,
    problem: (reason: string) => void,
  ): () => void;
  /** Sign `statement` as the product's account at raw index `index`, and submit it. */
  post(index: string, statement: WireStatement): Promise<void>;
}

export interface NovaSessionStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

export interface NovaSessionDeps {
  host: NovaHostPort;
  statements: NovaStatements;
  storage: NovaSessionStorage;
  recordKey: string;
  now(): number;
  newId(): string;
  /**
   * The conversation account's raw derivation index for `handle`, 32 bytes
   * `0x` hex. It must be the same for one handle every time: the host binds
   * a chat to the first index it saw, so a worker that lost its storage
   * finds the same account again rather than a refused new one.
   */
  indexFor(handle: string): string;
  /** Run `task` after `delayMs`; answers a cancel. */
  schedule(task: () => void, delayMs: number): () => void;
  /** Encode a `ChatMessageStatement` (id, timestamp, Text) as `0x` hex. */
  encodeText(id: string, timestamp: bigint, text: string): string;
  report(reason: string): void;
}

interface Entry {
  id: string;
  from: "me" | "peer";
  at: number;
  content: WireContent;
  status: "sending" | "sent" | "acked" | "failed" | "received";
}

interface Kept {
  handle: string;
  state: NovaState;
  /** The conversation account's raw derivation index. */
  index: string;
  history: Entry[];
  /** Our unacknowledged messages, encoded, by message id, in order. */
  pending: { id: string; encoded: string }[];
  /** Our request ids still unanswered, and the message ids each carried. */
  outstanding: { [requestId: string]: string[] };
  /** The last expiry per channel, decimal, so a replacement wins. */
  lastExpiry: { request: string | null; response: string | null };
  /** Until when the host allowed the conversation account to post, Unix ms. */
  allowedUntil: number;
}

/** The host refused a call; `final` when trying again does not change it. */
class HostRefusal extends Error {
  constructor(
    reason: string,
    readonly final: boolean,
  ) {
    super(reason);
  }
}

const refused = (result: { reason: string; final?: true }) =>
  new HostRefusal(result.reason, result.final === true);

/** The first wait after a failed post, doubled on each failure after it. */
export const RETRY_FIRST_MS = 30_000;
/** The longest wait between tries. */
export const RETRY_MAX_MS = 30 * 60_000;

const NEVER = 0xffff_ffffn << 32n;
const PROTOCOL_EPOCH_SECONDS = 1_763_164_800;

/** The apps' expiry: never in the high half, seconds since the protocol epoch as priority, above the last one (Android `StatementExpiry.nextAfter`). */
function nextExpiry(previous: string | null, nowMs: number): string {
  const now =
    NEVER |
    BigInt(Math.max(0, Math.floor(nowMs / 1000) - PROTOCOL_EPOCH_SECONDS));
  const last = previous === null ? null : BigInt(previous);
  return (last !== null && last >= now ? last + 1n : now).toString();
}

const CONTACT_STATE: { [state in NovaState]: ContactState } = {
  outgoing: "Outgoing",
  incoming: "Incoming",
  established: "Established",
  blocked: "Blocked",
  gone: "Invite",
};

function textOf(content: WireContent): string {
  switch (content.kind) {
    case "Text":
      return content.text;
    case "RichText":
      return content.text ?? "";
    case "Reply":
      return `↩ ${content.text ?? ""}`;
    case "Edited":
      return `✎ ${content.text ?? ""}`;
    case "Reacted":
      return `Reacted ${content.emoji}`;
    case "ReactionRemoved":
      return `Removed reaction ${content.emoji}`;
    case "LeftChat":
      return "Left the chat.";
    case "Unsupported":
      return "A message this prototype cannot show.";
  }
}

export class NovaSession implements ConversationEngine {
  private records: { [handle: string]: Kept } = {};
  private readonly follows = new Map<string, () => void>();
  private readonly listeners = new Set<() => void>();

  constructor(private readonly deps: NovaSessionDeps) {
    try {
      this.records = JSON.parse(
        deps.storage.getItem(deps.recordKey) ?? "{}",
      ) as { [handle: string]: Kept };
    } catch {
      deps.report("record: it does not read; starting empty");
    }
  }

  /** Learn the host's peers, then follow every established conversation. */
  async run(): Promise<void> {
    this.deps.host.onState((peer) => this.update(peer));
    const listed = await this.deps.host.call({ call: "list" });
    if (!listed.ok) throw new Error(listed.reason);
    for (const peer of listed.value as NovaPeer[]) this.update(peer);
  }

  private save(): void {
    this.deps.storage.setItem(
      this.deps.recordKey,
      JSON.stringify(this.records),
    );
    for (const listener of [...this.listeners]) listener();
  }

  private record(handle: string): Kept {
    return (this.records[handle] ??= {
      handle,
      state: "gone",
      index: this.deps.indexFor(handle),
      history: [],
      pending: [],
      outstanding: {},
      lastExpiry: { request: null, response: null },
      allowedUntil: 0,
    });
  }

  private update(peer: NovaPeer): void {
    const record = this.record(peer.handle);
    const changed = record.state !== peer.state;
    record.state = peer.state;
    this.save();
    if (changed) this.resume(record.handle);
    if (peer.state === "established") {
      if (this.follows.has(peer.handle)) {
        if (changed) void this.flush(record);
      } else void this.follow(record);
    } else {
      this.renewals.get(peer.handle)?.();
      this.renewals.delete(peer.handle);
      this.follows.get(peer.handle)?.();
      this.follows.delete(peer.handle);
    }
  }

  private async follow(record: Kept): Promise<void> {
    if (this.follows.has(record.handle)) return;
    this.follows.set(record.handle, () => {});
    const routes = await this.deps.host.call({
      call: "routes",
      handle: record.handle,
    });
    if (!routes.ok) {
      this.follows.delete(record.handle);
      return this.deps.report(`routes: ${routes.reason}`);
    }
    const { incoming } = routes.value as {
      outgoing: string;
      incoming: string[];
    };
    const stop = this.deps.statements.follow(
      incoming,
      (statements) => {
        for (const statement of statements)
          void this.heard(record.handle, statement);
      },
      (reason) => this.deps.report(`follow: ${reason}`),
    );
    if (this.follows.has(record.handle)) this.follows.set(record.handle, stop);
    else stop();
    if (record.pending.length > 0) void this.flush(record);
  }

  private async heard(
    handle: string,
    statement: WireSignedStatement,
  ): Promise<void> {
    const opened = await this.deps.host.call({
      call: "open",
      handle,
      statement,
    });
    if (!opened.ok) return;
    const record = this.record(handle);
    const frame = opened.value as OpenedFrame;
    if (frame.kind === "Response") {
      const ids = record.outstanding[frame.requestId];
      if (ids === undefined || frame.code !== 0) return;
      delete record.outstanding[frame.requestId];
      record.pending = record.pending.filter((each) => !ids.includes(each.id));
      for (const entry of record.history)
        if (entry.from === "me" && ids.includes(entry.id))
          entry.status = "acked";
      return this.save();
    }
    // Kinds this prototype does not show (the peer's push token, which both
    // apps send once the chat is set up, Seen, calls, files) are acknowledged
    // with the rest, and not listed.
    for (const message of frame.messages)
      if (
        message.content.kind !== "Unsupported" &&
        !record.history.some(
          (entry) => entry.from === "peer" && entry.id === message.id,
        )
      )
        record.history.push({
          id: message.id,
          from: "peer",
          at: Number(message.timestamp),
          content: message.content,
          status: "received",
        });
    this.save();
    await this.post(record, "response", {
      kind: "Response",
      requestId: frame.requestId,
      code: 0,
    }).catch((error: unknown) =>
      this.deps.report(
        `ack: ${error instanceof Error ? error.message : String(error)}`,
      ),
    );
  }

  /** Each conversation's posts, one at a time, so claims and expiries never race. */
  private readonly queues = new Map<string, Promise<unknown>>();

  /** Run `task` after every post of `handle` queued before it. */
  private serially<T>(handle: string, task: () => Promise<T>): Promise<T> {
    const next = (this.queues.get(handle) ?? Promise.resolve()).then(
      task,
      task,
    );
    this.queues.set(
      handle,
      next.catch(() => {}),
    );
    return next;
  }

  /**
   * Seal `frame` through the host and post it as the conversation account,
   * allowed first when needed. Posts of one conversation run one at a time;
   * a channel's expiry is kept only once a post with it was submitted.
   */
  private post(
    record: Kept,
    channel: "request" | "response",
    frame: WireFrame,
  ): Promise<void> {
    return this.serially(record.handle, () =>
      this.postNow(record, channel, frame),
    );
  }

  private async postNow(
    record: Kept,
    channel: "request" | "response",
    frame: WireFrame,
  ): Promise<void> {
    if (record.allowedUntil <= this.deps.now()) {
      const allowed = await this.deps.host.call({
        call: "conversationAccount",
        handle: record.handle,
        index: record.index,
      });
      if (!allowed.ok) throw refused(allowed);
      record.allowedUntil = (allowed.value as { until: number }).until;
    }
    const expiry = nextExpiry(record.lastExpiry[channel], this.deps.now());
    const sealed = await this.deps.host.call({
      call: "seal",
      handle: record.handle,
      frame,
      expiry,
    });
    if (!sealed.ok) throw refused(sealed);
    await this.deps.statements.post(
      record.index,
      sealed.value as WireStatement,
    );
    record.lastExpiry[channel] = expiry;
    this.save();
  }

  /** Consecutive failed posts per conversation, for the wait before the next try. */
  private readonly failures = new Map<string, number>();
  /** Conversations whose last post the host refused for good: no try until the user or a state change asks. */
  private readonly halted = new Set<string>();
  private readonly flushing = new Set<string>();
  private readonly flushAgain = new Set<string>();
  private stopped = false;

  /** Clear the failures of `handle`: the user asked, or the chat's state changed. */
  private resume(handle: string): void {
    this.failures.delete(handle);
    this.halted.delete(handle);
  }

  /**
   * Post every unacknowledged message of `record` in one new request frame.
   * One flush per conversation runs at a time; one asked for meanwhile runs
   * once after it, with every message then waiting.
   */
  private async flush(record: Kept): Promise<void> {
    if (this.stopped || record.pending.length === 0) return;
    if (this.flushing.has(record.handle)) {
      this.flushAgain.add(record.handle);
      return;
    }
    this.flushing.add(record.handle);
    try {
      await this.flushNow(record);
    } finally {
      this.flushing.delete(record.handle);
      if (this.flushAgain.delete(record.handle)) void this.flush(record);
    }
  }

  private async flushNow(record: Kept): Promise<void> {
    const requestId = this.deps.newId();
    const ids = record.pending.map((each) => each.id);
    record.outstanding = { [requestId]: ids };
    try {
      await this.post(record, "request", {
        kind: "Request",
        requestId,
        messages: record.pending.map((each) => each.encoded),
      });
      for (const entry of record.history)
        if (ids.includes(entry.id) && entry.status !== "acked")
          entry.status = "sent";
      this.failures.delete(record.handle);
    } catch (error) {
      for (const entry of record.history)
        if (ids.includes(entry.id) && entry.status !== "acked")
          entry.status = "failed";
      if (error instanceof HostRefusal && error.final)
        this.halted.add(record.handle);
      else
        this.failures.set(
          record.handle,
          (this.failures.get(record.handle) ?? 0) + 1,
        );
      this.deps.report(
        `send: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
    this.save();
    this.renewLater(record);
  }

  private readonly renewals = new Map<string, () => void>();

  /**
   * While messages wait for the peer's acknowledgement, post them again
   * once the conversation account's allowance has ended (the end of its UTC
   * day), with a new allowance: the node drops a statement whose account has
   * none. Only while the chat is followed.
   */
  private renewLater(record: Kept): void {
    this.renewals.get(record.handle)?.();
    this.renewals.delete(record.handle);
    if (
      this.stopped ||
      record.pending.length === 0 ||
      record.state !== "established" ||
      !this.follows.has(record.handle) ||
      this.halted.has(record.handle)
    )
      return;
    // After a failure, wait 30 s, doubled each time up to 30 min; else until
    // the allowance ends.
    const failures = this.failures.get(record.handle) ?? 0;
    const delay =
      failures > 0
        ? Math.min(RETRY_FIRST_MS * 2 ** (failures - 1), RETRY_MAX_MS)
        : Math.max(record.allowedUntil - this.deps.now(), 0) + 1_000;
    this.renewals.set(
      record.handle,
      this.deps.schedule(() => {
        this.renewals.delete(record.handle);
        void this.flush(record);
      }, delay),
    );
  }

  // ---- ConversationEngine --------------------------------------------------------------

  conversations(): Conversation[] {
    return Object.values(this.records)
      .filter((record) => record.state !== "gone" || record.history.length > 0)
      .map((record) => ({
        id: record.handle,
        title: "Nova chat",
        participants: [
          {
            id: "peer",
            name: "Contact",
            handle: { bytes: record.handle as `0x${string}` },
          },
        ],
        contact: {
          handle: { bytes: record.handle as `0x${string}` },
          state: CONTACT_STATE[record.state],
        },
        composer:
          record.state === "established"
            ? { allowed: true as const }
            : {
                allowed: false as const,
                reason:
                  record.state === "outgoing"
                    ? "Waiting for them to accept your request."
                    : record.state === "incoming"
                      ? "Accept their request in your Inbox first."
                      : record.state === "blocked"
                        ? "You blocked this person."
                        : "There is no chat with this person.",
              },
      }));
  }

  messages(conversationId: string): ConversationMessage[] {
    return (this.records[conversationId]?.history ?? []).map((entry) => ({
      id: entry.id,
      author:
        entry.from === "me"
          ? { kind: "self" as const }
          : entry.content.kind === "LeftChat"
            ? { kind: "system" as const }
            : { kind: "participant" as const, id: "peer" },
      text: textOf(entry.content),
      at: entry.at,
      state:
        entry.status === "acked"
          ? ("delivered" as const)
          : entry.status === "received"
            ? ("received" as const)
            : entry.status,
    }));
  }

  async send(conversationId: string, text: string): Promise<string> {
    const record = this.records[conversationId];
    if (record?.state !== "established")
      throw new Error("This chat is not established.");
    const id = this.deps.newId();
    const at = this.deps.now();
    record.history.push({
      id,
      from: "me",
      at,
      content: { kind: "Text", text },
      status: "sending",
    });
    record.pending.push({
      id,
      encoded: this.deps.encodeText(id, BigInt(at), text),
    });
    this.save();
    this.resume(record.handle);
    void this.flush(record);
    return id;
  }

  async retry(conversationId: string, messageId: string): Promise<void> {
    const record = this.records[conversationId];
    if (
      record === undefined ||
      !record.pending.some((each) => each.id === messageId)
    )
      return;
    this.resume(record.handle);
    await this.flush(record);
  }

  async start(target: StartTarget, welcome: string): Promise<string> {
    if (!("handle" in target))
      throw new Error("Nova Chat starts chats only with a contact handle.");
    const handle = target.handle.bytes.toLowerCase();
    const result = await this.deps.host.call({
      call: "request",
      handle,
      welcome: welcome.trim() === "" ? null : welcome,
    });
    if (!result.ok) throw new Error(result.reason);
    const record = this.record(handle);
    if (record.state === "gone") record.state = "outgoing";
    this.save();
    return handle;
  }

  onChange(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  /** The whole record, as JSON, for a test to read. */
  snapshot(): string {
    return JSON.stringify(this.records);
  }

  stop(): void {
    this.stopped = true;
    for (const stop of this.follows.values()) stop();
    this.follows.clear();
    for (const cancel of this.renewals.values()) cancel();
    this.renewals.clear();
  }
}

/** The host's end of `nova-chat/1` over `port`. */
export function novaHostOver(port: MessagePort): NovaHostPort {
  let nextId = 1;
  const calls = new Map<number, (result: NovaResult) => void>();
  const early: NovaPeer[] = [];
  let listener: ((peer: NovaPeer) => void) | null = null;
  port.onmessage = (event: MessageEvent<NovaHostMessage>) => {
    const message = event.data;
    if (message?.kind === "result") {
      calls.get(message.id)?.(message.result);
      calls.delete(message.id);
    } else if (message?.kind === "state") {
      if (listener === null) early.push(message.peer);
      else listener(message.peer);
    }
  };
  port.start();
  return {
    call(call) {
      const id = nextId++;
      return new Promise((resolve) => {
        calls.set(id, resolve);
        port.postMessage({ ...call, id });
      });
    },
    onState(next) {
      listener = next;
      for (const peer of early.splice(0)) next(peer);
    },
  };
}
