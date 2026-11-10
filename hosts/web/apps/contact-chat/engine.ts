import type { ContactHandle } from "@parity/truapi";
import type {
  HostMessage,
  IncomingRequest,
  OutgoingView,
  RepliedView,
} from "../../bridges/host-requests/contract.js";
import { toHex, type RandomBytes } from "../../bridges/primitives/bytes.js";
import type {
  Conversation,
  ConversationEngine,
  ConversationMessage,
  ContactState,
  MessageState,
  StartTarget,
} from "../../bridges/chat-v2/conversations.js";
import type {
  ConversationDiagnostics,
  LogicalCounts,
} from "../../bridges/diagnostics.js";
import type { ChatProtocol, ChatText, Heard } from "./protocol.js";
import type { HostRequests } from "../../bridges/host-requests/product.js";
import type { RecordStorage } from "../shared/product-storage.js";
import {
  RETENTION_SECONDS,
  messageExpiry,
  type Statements,
} from "./transport.js";

/**
 * PROTOTYPE. Contact Chat's engine: one-to-one conversations with people it
 * knows only by the contact handles its host gives it.
 *
 * Who does what:
 *
 * - The host names, draws and reaches people. A conversation starts when the
 *   user chooses someone in the host's New chat; the host hands this engine
 *   the person's contact handle (`start`), and the engine asks the host's
 *   request service (`src/invitations/`) to send its invitation to that
 *   handle. The host asks the user, seals, authenticates and delivers it,
 *   and hands back only an authenticated reply.
 * - The chat protocol (`./protocol.ts`) owns what travels: the setup
 *   payloads, the topics each side reads and posts on, and a message's
 *   bytes. The engine stores what it keeps and never reads it.
 * - This engine owns the conversation: the product's own account at a
 *   random raw derivation index, made for this conversation only and signed
 *   for by the core (`./transport.ts`), the history, and when to ask the host
 *   to allow that account to post (`chatAccount`). It reads and posts through
 *   the product's TrUAPI Statement Store. Each post replaces this side's last
 *   one: the network keeps a short window, and this engine the history.
 * - It never learns a name, account, username, avatar or identity key of a
 *   person: a room's contact and its one participant are the handle, and
 *   the host names them.
 *
 * The recipient reads from the moment it accepts, before its reply leaves,
 * and may write at once. The sender reads once it checked the reply; what the
 * recipient wrote meanwhile is read then if the subscription delivers it, as
 * a full node's replay does, or a peer sends it to the host's light client.
 *
 * Decline is silent: a request nobody answers stays pending until the
 * user cancels it in the host.
 *
 * Start over replaces a connected conversation's connection with a new one,
 * through a new request the other side must accept; it keeps the history and
 * tells the earlier connection nothing (`startOver`).
 *
 * Restarts: the conversation is saved, and the save confirmed kept, before
 * each call to the host that acts on it (request, reply, complete); a save
 * that fails makes no call. So a restart between the two finds it. A sent request the host knows but this
 * engine did not keep the handle of is matched again from the host's replay
 * by its contact; an accepted request whose reply the host took becomes
 * connected from the host's replay of it. A message still sending when the
 * engine stopped is failed, for the user to send again; a message read again
 * after a restart is kept once.
 */

/** What the recipient's host shows beside an invitation with no first message. */
export const DEFAULT_PURPOSE = "Wants to chat with you in Contact Chat.";

/** What the other side's host shows beside a Start over request. */
export const START_OVER_PURPOSE =
  "Started your chat over. Accept this new request to reconnect.";

/** The id of Start over, an entry of a connected conversation's options menu. */
export const START_OVER = "start-over";

/** What a connected conversation's room says under its name. */
export const CONNECTED_SUBTITLE = "Connected · messages are not encrypted";

/**
 * The line earlier versions added to a conversation as a system message when
 * it connected. A stored copy stays in the record and is not shown.
 */
const RETIRED_NOTICE =
  "Messages here are not encrypted: anyone who reads the Statement Store can read them. The network keeps only each side's last 20 messages, for a short time; this app keeps your history.";

/** The id the one other person of a conversation has, in its rooms. */
export const PEER = "peer";

type Status =
  /** The invitation is with the host, which asks the user before it leaves. */
  | "sending"
  /** Sent; no checked reply yet. */
  | "waiting"
  /** Sent, but the host cannot keep it posted now. */
  | "paused"
  /** The user accepted; the reply is with the host. */
  | "accepting"
  | "connected"
  | "not-sent"
  | "cancelled";

const STATUSES: readonly Status[] = [
  "sending",
  "waiting",
  "paused",
  "accepting",
  "connected",
  "not-sent",
  "cancelled",
];

interface StoredMessage {
  id: string;
  author: "self" | "peer" | "system";
  text: string;
  at: number;
  state: MessageState;
}

interface StoredConversation {
  id: string;
  /** The person's contact handle, as the host gave it. */
  contact: string;
  role: "sender" | "recipient";
  status: Status;
  createdAt: number;
  /** The host's handle for the request, sent or received. */
  request?: string;
  /** The raw derivation index of this side's product account for this conversation, `0x` hex. */
  index?: string;
  /** That account's public key, `0x` hex, as the core reported it. */
  account?: string;
  /** The expiry of this side's last post, as a decimal string: the next one must be higher. */
  lastExpiry?: string;
  /** What the protocol keeps of the sender's invitation, until the reply checks. */
  pending?: unknown;
  /**
   * The protocol's session, kept from before the reply leaves or completes.
   * Product-owned; not exposed in chat or request messages.
   */
  session?: unknown;
  /** Until when the host said the account may post. */
  allowedUntil?: number;
  provisional?: true;
  /** The latest window needs one post after its provisional lease is checked again. */
  pendingWindow?: true;
  /** Where this connection's own messages start in `messages`: earlier ones were posted in an earlier connection. */
  windowStart?: number;
  /**
   * A Start over the user chose, kept until the host holds its request:
   * the new account and invitation, and when it was made. The connection it
   * replaces stays as it is until then.
   */
  restart?: { at: number; index: string; account: string; pending: unknown };
  messages: StoredMessage[];
}

interface StoredRecord {
  version: 3;
  conversations: StoredConversation[];
}

export interface ContactChatDeps {
  requests: HostRequests;
  protocol: ChatProtocol;
  statements: Statements;
  storage: RecordStorage;
  recordKey: string;
  random: RandomBytes;
  now(): number;
  /** Told of what the engine could not do, for the worker's status. */
  report?(reason: string): void;
  /** Run a pending-window check after its lease ends; answers a cancel. */
  schedule?(task: () => void, delayMs: number): () => void;
}

const CONTACT_STATE: Record<Status, ContactState> = {
  sending: "Outgoing",
  waiting: "Outgoing",
  paused: "Outgoing",
  accepting: "Incoming",
  connected: "Established",
  "not-sent": "Invite",
  cancelled: "Invite",
};

const SUBTITLE: Record<Status, string> = {
  sending: "Sending your request",
  waiting: "Request sent",
  paused: "Request paused",
  accepting: "Accepting the request",
  connected: CONNECTED_SUBTITLE,
  "not-sent": "Request not sent",
  cancelled: "Request cancelled",
};

const OPEN: readonly Status[] = [
  "sending",
  "waiting",
  "paused",
  "accepting",
  "connected",
];
/** Sent and not answered yet. */
const OUTSTANDING: readonly Status[] = ["sending", "waiting", "paused"];

export class ContactChat implements ConversationEngine {
  private record: StoredRecord;
  private readonly listeners = new Set<() => void>();
  /** Replies being checked, so two copies of one are checked once. */
  private readonly checking = new Set<string>();
  /** Conversations whose invitation this run handed the host and has no answer for yet. */
  private readonly inviting = new Set<string>();
  /** Conversations whose Start over this run is making, until the host answers. */
  private readonly restarting = new Set<string>();
  /** The topics followed, by conversation: how to stop. */
  private readonly following = new Map<string, () => void>();
  /** An allowance being asked for, by account index, so concurrent sends ask once. */
  private readonly allowing = new Map<string, Promise<string | null>>();
  /** The post in flight, by conversation: the next one waits for it, so expiries rise in order. */
  private readonly posting = new Map<string, Promise<void>>();
  /** Counts since this engine started, by conversation, for the developer tools. */
  private readonly counts = new Map<string, Required<LogicalCounts>>();
  /** What this run's last post carried, by conversation. */
  private readonly lastBatch = new Map<
    string,
    { messages: number; bytes: number }
  >();
  private readonly diagnosticsListeners = new Set<() => void>();
  private readonly renewals = new Map<string, () => void>();
  private stopped = false;

  constructor(private readonly deps: ContactChatDeps) {
    this.record = this.load();
  }

  /** Follow the host's messages, and read every conversation that has a session. */
  run(): void {
    this.deps.requests.onMessage((message) => void this.hostMessage(message));
    for (const conversation of this.record.conversations)
      if (
        conversation.status === "accepting" ||
        conversation.status === "connected"
      ) {
        this.follow(conversation);
        if (conversation.pendingWindow) this.renewWindow(conversation);
      }
  }

  /** Stop reading. The conversations stay as kept. */
  stop(): void {
    this.stopped = true;
    for (const cancel of this.renewals.values()) cancel();
    this.renewals.clear();
    for (const stop of this.following.values()) stop();
    this.following.clear();
  }

  /** Every conversation as stored, for a test to read. */
  stored(): readonly StoredConversation[] {
    return this.record.conversations;
  }

  onChange(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  /**
   * What the developer tools may see of each conversation: the protocol, the
   * host request, the accounts and topics it uses, and counts since this
   * engine started. Nothing anyone wrote, and nothing secret.
   */
  diagnostics(): ConversationDiagnostics[] {
    const { protocol } = this.deps;
    return this.record.conversations.map((conversation) => {
      const routes =
        conversation.session === undefined
          ? undefined
          : protocol.routes(conversation.session);
      const batch = this.lastBatch.get(conversation.id);
      return {
        id: conversation.id,
        protocol: protocol.name,
        state: conversation.status,
        ...(conversation.request === undefined
          ? {}
          : { request: conversation.request }),
        ...(conversation.account === undefined
          ? {}
          : { account: conversation.account }),
        ...(routes === undefined
          ? {}
          : {
              peers: routes.peers,
              listen: protocol.listen(conversation.session),
              post: routes.post,
            }),
        logical: this.countsFor(conversation.id),
        ...(batch === undefined || protocol.window === undefined
          ? {}
          : {
              facts: [
                {
                  label: "Published message batch",
                  value: `${batch.messages} of ${protocol.window.messages} messages · ${batch.bytes} of ${protocol.window.bytes} B`,
                },
                {
                  label: "Requested expiry",
                  value: `${RETENTION_SECONDS / 3600} h per post; peers may evict it sooner`,
                },
              ],
            }),
      };
    });
  }

  /** Calls back whenever {@link diagnostics} may answer differently. */
  onDiagnostics(listener: () => void): () => void {
    this.diagnosticsListeners.add(listener);
    return () => this.diagnosticsListeners.delete(listener);
  }

  private countsFor(conversationId: string): Required<LogicalCounts> {
    let counts = this.counts.get(conversationId);
    if (counts === undefined) {
      counts = { sent: 0, received: 0, duplicates: 0, failed: 0, posts: 0 };
      this.counts.set(conversationId, counts);
    }
    return counts;
  }

  private count(
    conversationId: string,
    key: keyof LogicalCounts,
    by: number,
  ): void {
    if (by === 0) return;
    this.countsFor(conversationId)[key] += by;
    for (const listener of [...this.diagnosticsListeners]) listener();
  }

  conversations(): Conversation[] {
    return this.record.conversations.map((conversation) => {
      const handle: ContactHandle = {
        bytes: conversation.contact as ContactHandle["bytes"],
      };
      return {
        id: conversation.id,
        title: "Contact",
        subtitle: SUBTITLE[conversation.status],
        participants: [{ id: PEER, name: "Contact", handle }],
        contact: { handle, state: CONTACT_STATE[conversation.status] },
        ...(conversation.status === "connected" &&
        !this.restarting.has(conversation.id)
          ? { actions: [{ id: START_OVER, label: "Start over" }] }
          : {}),
        composer:
          conversation.status === "connected"
            ? { allowed: true }
            : {
                allowed: false,
                reason: OUTSTANDING.includes(conversation.status)
                  ? "Waiting for them to accept"
                  : conversation.status === "accepting"
                    ? "Accepting their request"
                    : "Start a new chat to ask again",
              },
      };
    });
  }

  messages(conversationId: string): ConversationMessage[] {
    return (this.find(conversationId)?.messages ?? [])
      .filter(
        (message) =>
          message.author !== "system" || message.text !== RETIRED_NOTICE,
      )
      .map((message) => ({
        id: message.id,
        author:
          message.author === "self"
            ? { kind: "self" }
            : message.author === "peer"
              ? { kind: "participant", id: PEER }
              : { kind: "system" },
        text: message.text,
        at: message.at,
        state: message.state,
      }));
  }

  /** The user chose an entry the conversation's menu offers: only Start over, while connected. */
  async act(conversationId: string, actionId: string): Promise<void> {
    const conversation = this.find(conversationId);
    const offered = this.conversations()
      .find((entry) => entry.id === conversationId)
      ?.actions?.some((action) => action.id === actionId);
    if (offered === true && actionId === START_OVER)
      return this.startOver(conversationId);
    // A Start over chosen from a menu drawn before the chat changed: the room says why nothing happened.
    if (conversation !== undefined && actionId === START_OVER) {
      this.system(
        conversation,
        this.restarting.has(conversation.id)
          ? "Start over is already waiting for your answer."
          : "Start over is offered only while the chat is connected.",
      );
      this.save();
      return;
    }
    throw new Error("This option is not offered any more.");
  }

  /**
   * The user chose Start over in a connected conversation: a new
   * conversation account and invitation go to the host, which asks the user
   * before the request leaves. Until the host holds it, the connection stays
   * as it was; a request the user dismisses or the host refuses changes
   * nothing. Once the host holds it, the earlier connection ends here: this
   * side stops reading it, keeps its history, and waits for the other side to
   * accept the new request. The other side is not told anything else.
   */
  async startOver(conversationId: string): Promise<void> {
    const conversation = this.find(conversationId);
    if (conversation?.status !== "connected")
      throw new Error("Only a connected chat can start over.");
    if (this.restarting.has(conversation.id))
      throw new Error("Start over is already waiting for your answer.");
    // The menu drops Start over while this one is being made, so it is not chosen twice.
    this.restarting.add(conversation.id);
    this.changed();
    try {
      await this.restart(conversation);
    } finally {
      this.restarting.delete(conversation.id);
      this.changed();
    }
  }

  private async restart(conversation: StoredConversation): Promise<void> {
    const session = conversation.session;
    const notRestarted = (why: string) => {
      delete conversation.restart;
      this.system(
        conversation,
        `Start over did not happen: ${why}. This chat is as it was.`,
      );
      this.save();
    };
    const index = toHex(this.deps.random(32));
    let account: string;
    try {
      account = await this.deps.statements.account(index);
    } catch (error) {
      this.deps.report?.(`account: ${errorText(error)}`);
      return notRestarted(
        "Contact Chat could not get an account for the new chat",
      );
    }
    if (conversation.status !== "connected" || conversation.session !== session)
      return;
    const { payload, pending } = this.deps.protocol.invite(
      account,
      this.deps.random,
    );
    const candidate = { at: this.deps.now(), index, account, pending };
    conversation.restart = candidate;
    if (!(await this.persist()))
      return notRestarted("Contact Chat could not save it");
    // Accepting their request while the save waited replaced this Start over: nothing is sent.
    if (conversation.restart !== candidate) return;
    const result = await this.deps.requests.call({
      call: "request",
      request: {
        contact: { bytes: conversation.contact },
        purpose: START_OVER_PURPOSE,
        payload,
      },
    });
    if (conversation.restart !== candidate) {
      // Accepting their request replaced the conversation meanwhile: nothing waits for this one.
      if (
        result.ok &&
        result.request !== undefined &&
        conversation.request !== result.request.handle
      )
        await this.deps.requests.call({
          call: "cancel",
          handle: result.request.handle,
        });
      return;
    }
    if (result.ok && result.request !== undefined)
      return this.restarted(conversation, result.request.handle);
    if (!result.ok && result.reason === "Dismissed") {
      delete conversation.restart;
      this.save();
      return;
    }
    notRestarted(result.ok ? "the host gave no request" : result.detail);
  }

  /**
   * The host holds the Start over request `handle`: the earlier connection
   * ends here, and the conversation waits for its new request to be accepted.
   * A message of the earlier connection still sending is failed, and is not
   * sent again in the new one.
   */
  private restarted(conversation: StoredConversation, handle: string): void {
    const candidate = conversation.restart!;
    this.endConnection(conversation);
    conversation.role = "sender";
    conversation.status = "waiting";
    conversation.request = handle;
    conversation.index = candidate.index;
    conversation.account = candidate.account;
    conversation.pending = candidate.pending;
    delete conversation.restart;
    delete conversation.session;
    this.system(
      conversation,
      "You started over. Your earlier connection ended here; they must accept your new chat request.",
    );
    this.save();
  }

  /**
   * The connection `conversation` had ends here: this side stops reading it,
   * its own messages still sending in it are failed, and a new window starts
   * after the history. A message before that window is kept, but neither
   * posted nor retried again in a later connection.
   */
  private endConnection(conversation: StoredConversation): void {
    this.unfollow(conversation);
    this.renewals.get(conversation.id)?.();
    this.renewals.delete(conversation.id);
    delete conversation.pendingWindow;
    delete conversation.provisional;
    for (const message of conversation.messages)
      if (message.author === "self" && message.state === "sending")
        message.state = "failed";
    conversation.windowStart = conversation.messages.length;
    this.lastBatch.delete(conversation.id);
    delete conversation.allowedUntil;
    delete conversation.lastExpiry;
  }

  /**
   * The user chose someone in the host's New chat. A conversation with them
   * that is open is opened; otherwise a new invitation goes to the host,
   * which asks the user before it leaves. Resolves at once with the room.
   */
  async start(target: StartTarget, welcome: string): Promise<string> {
    if (!("handle" in target))
      throw new Error("Contact Chat starts chats by contact handle only.");
    const contact = target.handle.bytes.toLowerCase();
    if (!/^0x[0-9a-f]{64}$/.test(contact))
      throw new Error("That is not a contact handle.");
    const known = this.record.conversations.find(
      (conversation) => conversation.contact === contact,
    );
    // One still sending from a run that ended never got the host's answer: ask again.
    const lost = known?.status === "sending" && !this.inviting.has(known.id);
    if (known !== undefined && OPEN.includes(known.status) && !lost)
      return known.id;
    const conversation: StoredConversation = known ?? {
      id: toHex(this.deps.random(16)),
      contact,
      role: "sender",
      status: "sending",
      createdAt: this.deps.now(),
      messages: [],
    };
    if (known === undefined) this.record.conversations.push(conversation);
    this.endConnection(conversation);
    conversation.role = "sender";
    conversation.status = "sending";
    delete conversation.request;
    delete conversation.session;
    delete conversation.account;
    delete conversation.restart;
    // A new invitation is a new conversation: it gets an account of its own.
    conversation.index = toHex(this.deps.random(32));
    delete conversation.pending;
    this.save();
    void this.invite(conversation, welcome.trim());
    return conversation.id;
  }

  private async invite(
    conversation: StoredConversation,
    welcome: string,
  ): Promise<void> {
    this.inviting.add(conversation.id);
    const notSent = (why: string) => {
      this.inviting.delete(conversation.id);
      if (conversation.status !== "sending") return;
      conversation.status = "not-sent";
      delete conversation.pending;
      this.system(conversation, `Your chat request was not sent: ${why}`);
      this.save();
    };
    let account: string;
    try {
      account = await this.deps.statements.account(conversation.index!);
    } catch (error) {
      this.deps.report?.(`account: ${errorText(error)}`);
      return notSent("Contact Chat could not get its account for this chat");
    }
    if (conversation.status !== "sending") {
      this.inviting.delete(conversation.id);
      return;
    }
    conversation.account = account;
    const { payload, pending } = this.deps.protocol.invite(
      account,
      this.deps.random,
    );
    conversation.pending = pending;
    if (!(await this.persist()))
      return notSent("Contact Chat could not save it");
    const result = await this.deps.requests.call({
      call: "request",
      request: {
        contact: { bytes: conversation.contact },
        purpose: welcome === "" ? DEFAULT_PURPOSE : welcome,
        payload,
      },
    });
    this.inviting.delete(conversation.id);
    if (conversation.status !== "sending") return;
    if (result.ok && result.request !== undefined) {
      conversation.request = result.request.handle;
      conversation.status = "waiting";
      this.system(
        conversation,
        welcome === ""
          ? "You sent a chat request"
          : `You sent a chat request: ${welcome}`,
      );
    } else {
      conversation.status = "not-sent";
      delete conversation.pending;
      this.system(
        conversation,
        !result.ok && result.reason === "Dismissed"
          ? "Your chat request was not sent"
          : `Your chat request was not sent: ${result.ok ? "the host gave no request" : result.detail}`,
      );
    }
    this.save();
  }

  /**
   * Send text in a connected conversation. Resolves with the message's id
   * once it is kept; it is then posted, and marked sent or failed.
   */
  async send(conversationId: string, text: string): Promise<string> {
    const conversation = this.find(conversationId);
    if (conversation === undefined) throw new Error("There is no such chat.");
    if (conversation.status !== "connected")
      throw new Error(
        OUTSTANDING.includes(conversation.status)
          ? "They have not accepted yet."
          : conversation.status === "accepting"
            ? "The chat is not connected yet."
            : "Start a new chat to ask again.",
      );
    const body = text.trim();
    if (body === "") throw new Error("The message is empty.");
    const message: ChatText = {
      id: toHex(this.deps.random(16)),
      text: body,
      at: this.deps.now(),
    };
    // The protocol refuses what it cannot carry before anything is kept.
    this.deps.protocol.post(conversation.session, [message]);
    conversation.messages.push({
      ...message,
      author: "self",
      state: "sending",
    });
    this.save();
    void this.deliver(conversation);
    return message.id;
  }

  /** Send a failed message of this side's again. */
  async retry(conversationId: string, messageId: string): Promise<void> {
    const conversation = this.find(conversationId);
    const message = conversation?.messages.find(
      (entry) => entry.id === messageId && entry.author === "self",
    );
    if (conversation === undefined || message === undefined)
      throw new Error("There is no such message.");
    if (message.state !== "failed" && !conversation.pendingWindow)
      throw new Error("Only a failed or unconfirmed message is sent again.");
    if (conversation.status !== "connected")
      throw new Error("The chat is not connected.");
    if (
      conversation.messages.indexOf(message) < (conversation.windowStart ?? 0)
    )
      throw new Error("This message was written in an earlier connection.");
    message.state = "sending";
    this.save();
    void this.deliver(conversation);
  }

  /**
   * Post this side's latest messages, as its conversation account, once the
   * host allowed it, after any post still in flight. Each post replaces the
   * last one: the messages it carries are sent; those still sending that it
   * could not post are failed.
   */
  private deliver(conversation: StoredConversation): Promise<void> {
    const previous = this.posting.get(conversation.id) ?? Promise.resolve();
    const next = previous.then(() => this.postWindow(conversation));
    this.posting.set(conversation.id, next);
    void next.finally(() => {
      if (this.posting.get(conversation.id) === next)
        this.posting.delete(conversation.id);
    });
    return next;
  }

  private async postWindow(conversation: StoredConversation): Promise<void> {
    // A Start over may replace the connection while this post waits: it ends on the one it began with.
    const { session, index } = conversation;
    const own = conversation.messages
      .slice(conversation.windowStart ?? 0)
      .filter((message) => message.author === "self");
    const recovering =
      conversation.pendingWindow === true &&
      (conversation.allowedUntil ?? 0) <= this.deps.now();
    if (
      this.stopped ||
      (!recovering && !own.some((message) => message.state === "sending"))
    )
      return;
    const current = () =>
      !this.stopped &&
      conversation.index === index &&
      conversation.session === session;
    const { post, carried } = this.deps.protocol.post(
      session,
      own.map(({ id, text, at }) => ({ id, text, at })),
    );
    const fail = (problem: string) => {
      if (!current()) return;
      delete conversation.pendingWindow;
      this.deps.report?.(`send: ${problem}`);
      let failed = 0;
      for (const message of own)
        if (
          message.state === "sending" ||
          (recovering && carried.includes(message.id))
        ) {
          message.state = "failed";
          failed += 1;
        }
      this.count(conversation.id, "failed", failed);
      this.save();
    };
    if (index === undefined)
      return fail("This chat has no account to post with.");
    const refused = await this.allow(conversation);
    if (!current()) return;
    if (refused !== null) {
      this.system(
        conversation,
        `Your message was not sent: your host did not let this chat post. ${refused}`,
      );
      return fail(refused);
    }
    const expiry = messageExpiry(
      conversation.lastExpiry === undefined
        ? null
        : BigInt(conversation.lastExpiry),
      this.deps.now(),
    );
    try {
      await this.deps.statements.post(index, post, expiry);
    } catch (error) {
      if (!current()) return;
      // The allowance may have lapsed: the next send asks the host again.
      delete conversation.allowedUntil;
      const reason = errorText(error);
      this.system(conversation, `Your message was not sent: ${reason}`);
      return fail(reason);
    }
    if (!current()) return;
    conversation.lastExpiry = expiry.toString();
    let sent = 0;
    let failed = 0;
    for (const message of own)
      if (carried.includes(message.id)) {
        if (message.state !== "sent") sent += 1;
        message.state = "sent";
      } else if (message.state === "sending") {
        message.state = "failed";
        failed += 1;
      }
    this.lastBatch.set(conversation.id, {
      messages: carried.length,
      bytes: (post.data.length - 2) / 2,
    });
    this.count(conversation.id, "posts", 1);
    this.count(conversation.id, "sent", sent);
    this.count(conversation.id, "failed", failed);
    if (conversation.provisional && !recovering) {
      conversation.pendingWindow = true;
      this.renewWindow(conversation);
    } else delete conversation.pendingWindow;
    this.save();
  }

  private renewWindow(conversation: StoredConversation): void {
    this.renewals.get(conversation.id)?.();
    const index = conversation.index;
    const schedule =
      this.deps.schedule ??
      ((task: () => void, delay: number) => {
        const timer = setTimeout(task, delay);
        return () => clearTimeout(timer);
      });
    this.renewals.set(
      conversation.id,
      schedule(
        () => {
          this.renewals.delete(conversation.id);
          if (
            !this.stopped &&
            conversation.index === index &&
            conversation.status === "connected"
          )
            void this.deliver(conversation);
        },
        Math.max(0, (conversation.allowedUntil ?? 0) - this.deps.now()) + 1,
      ),
    );
  }

  /** Null once the host allowed the conversation's account to post now, or why it did not. */
  private allow(conversation: StoredConversation): Promise<string | null> {
    if ((conversation.allowedUntil ?? 0) > this.deps.now())
      return Promise.resolve(null);
    const { request, index } = conversation;
    if (request === undefined || index === undefined)
      return Promise.resolve("This chat has no request to post under.");
    const asking = this.allowing.get(index);
    if (asking !== undefined) return asking;
    const ask = (async (): Promise<string | null> => {
      const result = await this.deps.requests.call({
        call: "chatAccount",
        handle: request,
        index,
      });
      if (!result.ok) return result.detail;
      if (result.allowance === undefined) return "The host gave no allowance.";
      if (!this.stopped && conversation.index === index) {
        conversation.allowedUntil = result.allowance.until;
        conversation.provisional = result.allowance.provisional;
        this.save();
      }
      return null;
    })().finally(() => this.allowing.delete(index));
    this.allowing.set(index, ask);
    return ask;
  }

  /** Read what the other side posts in `conversation`, as the subscription delivers it. */
  private follow(conversation: StoredConversation): void {
    if (
      this.following.has(conversation.id) ||
      conversation.session === undefined
    )
      return;
    const topics = this.deps.protocol.listen(conversation.session);
    this.following.set(
      conversation.id,
      this.deps.statements.follow(
        topics,
        (heard) => this.heard(conversation.id, heard),
        (reason) => this.deps.report?.(`read: ${reason}`),
      ),
    );
  }

  private unfollow(conversation: StoredConversation): void {
    this.following.get(conversation.id)?.();
    this.following.delete(conversation.id);
  }

  /** Statements on a conversation's topics: keep each new message of the other side's once. */
  private heard(conversationId: string, heard: Heard[]): void {
    const conversation = this.find(conversationId);
    if (conversation?.session === undefined) return;
    const known = new Set(
      conversation.messages
        .filter((message) => message.author === "peer")
        .map((message) => message.id),
    );
    const fresh: ChatText[] = [];
    let duplicates = 0;
    for (const statement of heard)
      for (const message of this.deps.protocol.read(
        conversation.session,
        statement,
      )) {
        if (known.has(message.id)) {
          duplicates += 1;
          continue;
        }
        known.add(message.id);
        fresh.push(message);
      }
    this.count(conversationId, "duplicates", duplicates);
    this.count(conversationId, "received", fresh.length);
    if (fresh.length === 0) return;
    for (const message of fresh.sort((left, right) => left.at - right.at))
      conversation.messages.push({
        id: message.id,
        author: "peer",
        text: message.text,
        at: message.at,
        state: "received",
      });
    this.save();
  }

  /**
   * Withdraw this side's request through the host's `cancel`, as a product
   * may without its user going to the host. The other person is not told.
   */
  async cancel(conversationId: string): Promise<void> {
    const conversation = this.find(conversationId);
    const handle = conversation?.request;
    if (
      conversation?.role !== "sender" ||
      handle === undefined ||
      !OUTSTANDING.includes(conversation.status)
    )
      throw new Error("There is no request to cancel.");
    const result = await this.deps.requests.call({ call: "cancel", handle });
    if (!result.ok) throw new Error(result.detail);
    if (result.request !== undefined) this.outgoing(result.request);
  }

  private async hostMessage(message: HostMessage): Promise<void> {
    switch (message.kind) {
      case "outgoing":
        return this.outgoing(message.request);
      case "response":
        return this.response(message.handle, message.payload);
      case "acceptIntent":
        return this.accept(message.request);
      case "openIntent":
        await this.deps.requests.call({ call: "open", handle: message.handle });
        return;
      case "replied":
        return this.replied(message.request);
      case "result":
        return;
    }
  }

  private bySentRequest(handle: string): StoredConversation | undefined {
    return this.record.conversations.find(
      (conversation) =>
        conversation.role === "sender" && conversation.request === handle,
    );
  }

  /**
   * A sent request changed at the host, or every one on attach. One this
   * engine lost the handle of, in a restart while the host sent it, is the
   * outstanding request to the same contact.
   */
  private outgoing(view: OutgoingView): void {
    const conversation = this.bySentRequest(view.handle) ?? this.adopt(view);
    if (
      conversation === undefined ||
      !OUTSTANDING.includes(conversation.status)
    )
      return;
    if (view.status === "cancelled") {
      conversation.status = "cancelled";
      delete conversation.pending;
      this.system(conversation, "You cancelled your chat request");
      this.save();
      return;
    }
    const next = view.status === "paused" ? "paused" : "waiting";
    if (view.status === "completed" || conversation.status === next) return;
    conversation.status = next;
    this.save();
  }

  /**
   * The conversation that asked the host for `view`, a request it holds,
   * when this engine stopped before it kept the handle: one still sending to
   * that contact, or one connected whose Start over the host took. A Start
   * over is matched only by a request made after it.
   */
  private adopt(view: OutgoingView): StoredConversation | undefined {
    if (view.status !== "waiting" && view.status !== "paused") return;
    const contact = view.contact.bytes.toLowerCase();
    const restarted = this.record.conversations.find(
      (entry) =>
        entry.contact === contact &&
        entry.status === "connected" &&
        entry.restart !== undefined &&
        view.createdAt >= entry.restart.at,
    );
    if (restarted !== undefined) {
      this.restarted(restarted, view.handle);
      return restarted;
    }
    const conversation = this.record.conversations.find(
      (entry) =>
        entry.role === "sender" &&
        entry.contact === contact &&
        entry.request === undefined &&
        entry.pending !== undefined &&
        (entry.status === "sending" || entry.status === "not-sent"),
    );
    if (conversation === undefined) return;
    conversation.request = view.handle;
    conversation.status = "waiting";
    this.system(conversation, "You sent a chat request");
    this.save();
    return conversation;
  }

  /** The host took this side's reply to a request: an accepting conversation is connected. */
  private async replied(view: RepliedView): Promise<void> {
    const conversation = this.record.conversations.find(
      (entry) =>
        entry.role === "recipient" &&
        entry.request === view.handle &&
        entry.status === "accepting",
    );
    if (conversation === undefined) return;
    this.connected(conversation, "You accepted the chat request");
  }

  /** `conversation` is connected: say so. */
  private connected(conversation: StoredConversation, line: string): void {
    conversation.status = "connected";
    this.system(conversation, line);
    this.save();
    this.follow(conversation);
  }

  /**
   * An authenticated reply from the host. The engine checks it itself, keeps
   * the connection, and only then tells the host it is complete.
   */
  private async response(handle: string, payload: string): Promise<void> {
    const conversation = this.bySentRequest(handle);
    if (conversation === undefined) return;
    if (conversation.status === "connected") {
      // The host told it again, as after a restart: the request may still be open there.
      await this.complete(handle);
      return;
    }
    if (
      (conversation.status !== "waiting" && conversation.status !== "paused") ||
      conversation.pending === undefined
    )
      return;
    if (this.checking.has(payload)) return;
    const opened = this.deps.protocol.open(conversation.pending, payload);
    if (!opened.ok) {
      this.deps.report?.(`a reply did not check: ${opened.reason}`);
      return;
    }
    this.checking.add(payload);
    try {
      conversation.session = opened.value;
      delete conversation.pending;
      this.connected(conversation, "They accepted your chat request");
      await this.complete(handle);
    } finally {
      this.checking.delete(payload);
    }
  }

  /** Tell the host the request is complete, once the connection it made is kept. */
  private async complete(handle: string): Promise<void> {
    if (!(await this.persist())) return;
    const result = await this.deps.requests.call({ call: "complete", handle });
    if (!result.ok && result.reason !== "Refused")
      this.deps.report?.(`complete: ${result.detail}`);
  }

  /**
   * The user chose Accept in the host. The protocol answers the invitation
   * with the product's account at a new index for this conversation; the engine keeps the
   * conversation as accepting, starts reading, and replies through the host. It is connected once the host
   * took the reply; a reply the host refuses leaves it accepting, and Accept
   * can be chosen again.
   */
  private async accept(request: IncomingRequest): Promise<void> {
    const index = toHex(this.deps.random(32));
    let account: string;
    try {
      account = await this.deps.statements.account(index);
    } catch (error) {
      this.deps.report?.(`account: ${errorText(error)}`);
      await this.deps.requests.call({
        call: "refuse",
        handle: request.handle,
        reason: "Contact Chat could not get its account for this chat.",
      });
      return;
    }
    const made = this.deps.protocol.respond(
      request.payload,
      account,
      this.deps.random,
    );
    if (made === null) {
      await this.deps.requests.call({
        call: "refuse",
        handle: request.handle,
        reason: "Contact Chat cannot use this request.",
      });
      return;
    }
    const contact = request.sender.bytes.toLowerCase();
    const known = this.record.conversations.find(
      (conversation) => conversation.contact === contact,
    );
    const conversation: StoredConversation = known ?? {
      id: toHex(this.deps.random(16)),
      contact,
      role: "recipient",
      status: "accepting",
      createdAt: this.deps.now(),
      messages: [],
    };
    if (known === undefined) this.record.conversations.push(conversation);
    this.endConnection(conversation);
    conversation.role = "recipient";
    conversation.status = "accepting";
    conversation.request = request.handle;
    conversation.index = index;
    conversation.account = account;
    conversation.session = made.session;
    delete conversation.pending;
    delete conversation.restart;
    // Kept before the reply leaves, so a restart in between still has it.
    this.save();
    if (!(await this.persist())) {
      await this.deps.requests.call({
        call: "refuse",
        handle: request.handle,
        reason: "Contact Chat could not save the chat. Try again.",
      });
      return;
    }
    // Reading starts before the reply leaves, so nothing posted after it is missed.
    this.follow(conversation);
    const result = await this.deps.requests.call({
      call: "reply",
      handle: request.handle,
      payload: made.response,
      open: true,
    });
    if (!result.ok) {
      this.deps.report?.(`reply: ${result.detail}`);
      return;
    }
    if (conversation.status !== "accepting") return;
    this.connected(conversation, "You accepted the chat request");
  }

  private system(conversation: StoredConversation, text: string): void {
    conversation.messages.push({
      id: toHex(this.deps.random(16)),
      author: "system",
      text,
      at: this.deps.now(),
      state: "received",
    });
  }

  private find(conversationId: string): StoredConversation | undefined {
    return this.record.conversations.find(
      (conversation) => conversation.id === conversationId,
    );
  }

  private load(): StoredRecord {
    try {
      const saved = JSON.parse(
        this.deps.storage.getItem(this.deps.recordKey) ?? "null",
      ) as StoredRecord | null;
      if (saved?.version === 3 && Array.isArray(saved.conversations)) {
        for (const conversation of saved.conversations) {
          // A status this version does not know is a request no longer sent.
          if (!STATUSES.includes(conversation.status))
            conversation.status = "not-sent";
          // A message still sending when the engine stopped did not reach the store as far as it knows.
          for (const message of conversation.messages)
            if (message.state === "sending") message.state = "failed";
        }
        return saved;
      }
    } catch {
      this.deps.report?.("the stored record does not read; starting empty");
    }
    return { version: 3, conversations: [] };
  }

  private save(): void {
    this.deps.storage.setItem(this.deps.recordKey, JSON.stringify(this.record));
    this.changed();
  }

  /** Tell the listeners what the engine answers may have changed. */
  private changed(): void {
    for (const listener of [...this.listeners]) listener();
    for (const listener of [...this.diagnosticsListeners]) listener();
  }

  /** Save, and wait until storage confirms it kept every write. False, reported, when it did not. */
  private async persist(): Promise<boolean> {
    this.save();
    try {
      await this.deps.storage.flush();
      return true;
    } catch (error) {
      this.deps.report?.(`save: ${errorText(error)}`);
      return false;
    }
  }
}

const errorText = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);
