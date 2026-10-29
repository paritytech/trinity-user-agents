import type { ContactHandle } from "@parity/truapi";
import type {
  HostMessage,
  IncomingRequest,
  OutgoingView,
} from "../../../src/invitations/contract.js";
import {
  fromHex,
  fromUtf8,
  toHex,
  utf8,
  x25519Agree,
  type RandomBytes,
} from "../../../src/invitations/sealing.js";
import type {
  Conversation,
  ConversationEngine,
  ConversationMessage,
  ContactState,
  MessageState,
  StartTarget,
} from "../../../src/chat/draft/conversations.js";
import {
  checkResponse,
  createInvitation,
  readInvitation,
  respond,
  type SenderSecrets,
} from "../example-chat/setup.js";
import {
  openFrame,
  sealFrame,
  sessionKeys,
  type SessionKeys,
} from "./messages.js";
import type { HostRequests } from "./requests.js";

/**
 * DEV ONLY. Contact Chat's engine: one-to-one text conversations with people
 * it knows only by the contact handles its host gives it.
 *
 * Who does what:
 *
 * - The host names, draws and reaches people. A conversation starts when the
 *   user chooses someone in the host's New chat; the host hands this engine
 *   the person's contact handle (`start`), and the engine asks the host's
 *   request service (`src/invitations/`) to send its invitation to that
 *   handle. The host asks the user, seals, authenticates and delivers it,
 *   and hands back only an authenticated reply.
 * - This engine owns the protocol: the setup (`../example-chat/setup.ts`,
 *   a toy), its keys, its check of the reply before it calls `complete`, the
 *   text layer (`./messages.ts`, a fixture), the history, and the two
 *   conversation topics, which the recipient makes.
 * - It never learns a name, account, username, avatar or identity key: a
 *   room's contact and its one participant are the handle, and the host
 *   names them.
 *
 * Decline is silent: a request nobody answers waits until it expires. There
 * is no way yet for the user to cancel one from the host's Chat.
 */

/** How long an invitation may be answered. */
export const INVITATION_TTL_MS = 24 * 60 * 60_000;

/** What the recipient's host shows beside an invitation with no first message. */
export const DEFAULT_PURPOSE = "Wants to chat with you in Contact Chat.";

/** Frames remembered per conversation, to drop a repeat. */
const SEEN_MAX = 4096;

/** The id the one other person of a conversation has, in its rooms. */
export const PEER = "peer";

type Status =
  /** The invitation is with the host, which asks the user before it leaves. */
  | "sending"
  /** Sent; no checked reply yet. */
  | "waiting"
  | "connected"
  | "not-sent"
  | "cancelled"
  | "expired";

interface StoredMessage {
  id: string;
  author: "self" | "peer" | "system";
  text: string;
  at: number;
  state: MessageState;
  /** For the user's own text: the frame, to post again on retry. */
  frame?: string;
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
  /** The sender's setup secrets, until the reply checks. */
  invitation?: SenderSecrets;
  session?: SessionKeys;
  /** The last counter this side sealed. */
  sent: number;
  /** Counters of the frames read, newest last. */
  seen: number[];
  messages: StoredMessage[];
}

interface StoredRecord {
  version: 1;
  conversations: StoredConversation[];
}

/** What the engine needs of the product's TrUAPI storage. */
export type RecordStorage = Pick<Storage, "getItem" | "setItem">;

/** Where the conversation topics are: the Statement Store, or a stand-in for it. */
export interface ConversationTopics {
  post(topic: string, data: string): Promise<void>;
  watch(topic: string, onData: (data: string[]) => void): Promise<() => void>;
}

export interface ContactChatDeps {
  requests: HostRequests;
  topics: ConversationTopics;
  storage: RecordStorage;
  recordKey: string;
  random: RandomBytes;
  now(): number;
  /** Told of what the engine could not do, for the worker's status. */
  report?(reason: string): void;
}

const CONTACT_STATE: Record<Status, ContactState> = {
  sending: "Outgoing",
  waiting: "Outgoing",
  connected: "Established",
  "not-sent": "Invite",
  cancelled: "Invite",
  expired: "Invite",
};

const SUBTITLE: Record<Status, string> = {
  sending: "Sending your request",
  waiting: "Request sent",
  connected: "Contact Chat",
  "not-sent": "Request not sent",
  cancelled: "Request cancelled",
  expired: "Request expired",
};

const OPEN: readonly Status[] = ["sending", "waiting", "connected"];

export class ContactChat implements ConversationEngine {
  private record: StoredRecord;
  private readonly listeners = new Set<() => void>();
  private readonly watching = new Map<string, () => void>();
  /** Replies being checked, so two copies of one are checked once. */
  private readonly checking = new Set<string>();

  constructor(private readonly deps: ContactChatDeps) {
    this.record = this.load();
  }

  /** Follow the host's messages and the topics of every connected conversation. */
  async run(): Promise<void> {
    this.deps.requests.onMessage((message) => void this.hostMessage(message));
    await Promise.all(
      this.record.conversations
        .filter((conversation) => conversation.status === "connected")
        .map((conversation) => this.watch(conversation)),
    );
  }

  /** Every conversation as stored, for a test to read. */
  stored(): readonly StoredConversation[] {
    return this.record.conversations;
  }

  onChange(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
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
        composer:
          conversation.status === "connected"
            ? { allowed: true }
            : {
                allowed: false,
                reason:
                  conversation.status === "sending" ||
                  conversation.status === "waiting"
                    ? "Waiting for them to accept"
                    : "Start a new chat to ask again",
              },
      };
    });
  }

  messages(conversationId: string): ConversationMessage[] {
    return (this.find(conversationId)?.messages ?? []).map((message) => ({
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
    if (known !== undefined && OPEN.includes(known.status)) return known.id;
    const conversation: StoredConversation = known ?? {
      id: toHex(this.deps.random(16)),
      contact,
      role: "sender",
      status: "sending",
      createdAt: this.deps.now(),
      sent: 0,
      seen: [],
      messages: [],
    };
    if (known === undefined) this.record.conversations.push(conversation);
    conversation.role = "sender";
    conversation.status = "sending";
    delete conversation.request;
    delete conversation.session;
    const { payload, secrets } = createInvitation(this.deps.random);
    conversation.invitation = secrets;
    this.save();
    void this.invite(conversation, payload, welcome.trim());
    return conversation.id;
  }

  private async invite(
    conversation: StoredConversation,
    payload: string,
    welcome: string,
  ): Promise<void> {
    const result = await this.deps.requests.call({
      call: "request",
      request: {
        contact: { bytes: conversation.contact },
        purpose: welcome === "" ? DEFAULT_PURPOSE : welcome,
        ttlMs: INVITATION_TTL_MS,
        payload,
      },
    });
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
      delete conversation.invitation;
      this.system(
        conversation,
        !result.ok && result.reason === "Dismissed"
          ? "Your chat request was not sent"
          : `Your chat request was not sent: ${result.ok ? "the host gave no request" : result.detail}`,
      );
    }
    this.save();
  }

  async send(conversationId: string, text: string): Promise<string> {
    const conversation = this.find(conversationId);
    if (conversation === undefined) throw new Error("There is no such chat.");
    const session = conversation.session;
    if (conversation.status !== "connected" || session === undefined)
      throw new Error(
        conversation.status === "waiting" || conversation.status === "sending"
          ? "They have not accepted yet."
          : "Start a new chat to ask again.",
      );
    conversation.sent += 1;
    const message: StoredMessage = {
      id: toHex(this.deps.random(16)),
      author: "self",
      text,
      at: this.deps.now(),
      state: "sending",
    };
    message.frame = sealFrame(
      session.send,
      session.out,
      conversation.sent,
      utf8(JSON.stringify({ id: message.id, at: message.at, text })),
    );
    conversation.messages.push(message);
    this.save();
    void this.post(conversation, message);
    return message.id;
  }

  async retry(conversationId: string, messageId: string): Promise<void> {
    const conversation = this.find(conversationId);
    const message = conversation?.messages.find(
      (entry) => entry.id === messageId && entry.author === "self",
    );
    if (conversation === undefined || message === undefined)
      throw new Error("There is no such message.");
    message.state = "sending";
    this.save();
    await this.post(conversation, message);
  }

  private async post(
    conversation: StoredConversation,
    message: StoredMessage,
  ): Promise<void> {
    const session = conversation.session;
    if (session === undefined || message.frame === undefined) return;
    try {
      await this.deps.topics.post(session.out, message.frame);
      message.state = "sent";
    } catch (error) {
      message.state = "failed";
      this.deps.report?.(`post: ${errorText(error)}`);
    }
    this.save();
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

  /** A sent request changed at the host, or every one on attach. */
  private outgoing(view: OutgoingView): void {
    const conversation = this.bySentRequest(view.handle);
    if (conversation === undefined || conversation.status === "connected")
      return;
    if (view.status === "cancelled" || view.status === "expired") {
      if (conversation.status === view.status) return;
      conversation.status = view.status;
      delete conversation.invitation;
      this.system(
        conversation,
        view.status === "expired"
          ? "Your chat request expired"
          : "You cancelled your chat request",
      );
      this.save();
    }
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
    const secrets = conversation.invitation;
    if (conversation.status !== "waiting" || secrets === undefined) return;
    if (this.checking.has(payload)) return;
    const checked = checkResponse(secrets, payload);
    if (!checked.ok) {
      this.deps.report?.(`a reply did not check: ${checked.reason}`);
      return;
    }
    this.checking.add(payload);
    try {
      conversation.session = sessionKeys(
        "sender",
        x25519Agree(
          fromHex(secrets.protocolSecret),
          fromHex(checked.protocolKey),
        ),
        secrets.payload,
        payload,
        checked.topics,
      );
      conversation.status = "connected";
      delete conversation.invitation;
      this.system(conversation, "They accepted your chat request");
      this.save();
      await this.watch(conversation);
      await this.complete(handle);
    } finally {
      this.checking.delete(payload);
    }
  }

  private async complete(handle: string): Promise<void> {
    const result = await this.deps.requests.call({ call: "complete", handle });
    if (!result.ok && result.reason !== "Refused")
      this.deps.report?.(`complete: ${result.detail}`);
  }

  /**
   * The user chose Accept in the host. The engine checks the invitation,
   * makes both topics and its response, and replies through the host. The
   * conversation is kept only once the host took the reply.
   */
  private async accept(request: IncomingRequest): Promise<void> {
    const invitation = readInvitation(request.payload);
    const made =
      invitation === null ? null : respond(request.payload, this.deps.random);
    if (invitation === null || made === null) {
      await this.deps.requests.call({
        call: "refuse",
        handle: request.handle,
        reason: "Contact Chat cannot use this request.",
      });
      return;
    }
    const session = sessionKeys(
      "recipient",
      x25519Agree(
        fromHex(made.protocolSecret),
        fromHex(invitation.protocolKey),
      ),
      request.payload,
      made.response,
      made.topics,
    );
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
    const contact = request.sender.bytes.toLowerCase();
    const known = this.record.conversations.find(
      (conversation) => conversation.contact === contact,
    );
    const conversation: StoredConversation = known ?? {
      id: toHex(this.deps.random(16)),
      contact,
      role: "recipient",
      status: "connected",
      createdAt: this.deps.now(),
      sent: 0,
      seen: [],
      messages: [],
    };
    if (known === undefined) this.record.conversations.push(conversation);
    this.unwatch(conversation);
    conversation.role = "recipient";
    conversation.status = "connected";
    conversation.request = request.handle;
    conversation.session = session;
    conversation.sent = 0;
    conversation.seen = [];
    delete conversation.invitation;
    this.system(conversation, "You accepted the chat request");
    this.save();
    await this.watch(conversation);
  }

  private async watch(conversation: StoredConversation): Promise<void> {
    const session = conversation.session;
    if (session === undefined || this.watching.has(conversation.id)) return;
    const topic = session.in;
    this.watching.set(conversation.id, () => {});
    try {
      const stop = await this.deps.topics.watch(topic, (frames) =>
        this.arrived(conversation.id, topic, frames),
      );
      if (this.watching.has(conversation.id))
        this.watching.set(conversation.id, stop);
      else stop();
    } catch (error) {
      this.watching.delete(conversation.id);
      this.deps.report?.(`watch: ${errorText(error)}`);
    }
  }

  private unwatch(conversation: StoredConversation): void {
    this.watching.get(conversation.id)?.();
    this.watching.delete(conversation.id);
  }

  /** Frames read on a conversation's incoming topic. What does not open is dropped. */
  private arrived(
    conversationId: string,
    topic: string,
    frames: string[],
  ): void {
    const conversation = this.find(conversationId);
    const session = conversation?.session;
    if (conversation === undefined || session?.in !== topic) return;
    let changed = false;
    for (const frame of frames) {
      const opened = openFrame(session.receive, topic, frame);
      if (opened === null) {
        this.deps.report?.("dropped a frame that does not open");
        continue;
      }
      if (conversation.seen.includes(opened.counter)) continue;
      const body = readBody(opened.plaintext);
      if (body === null) {
        this.deps.report?.("dropped a frame that is not text");
        continue;
      }
      conversation.seen.push(opened.counter);
      if (conversation.seen.length > SEEN_MAX)
        conversation.seen.splice(0, conversation.seen.length - SEEN_MAX);
      conversation.messages.push({
        id: body.id,
        author: "peer",
        text: body.text,
        at: body.at,
        state: "received",
      });
      changed = true;
    }
    if (!changed) return;
    conversation.messages.sort((left, right) => left.at - right.at);
    this.save();
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
      if (saved?.version === 1 && Array.isArray(saved.conversations))
        return saved;
    } catch {
      this.deps.report?.("the stored record does not read; starting empty");
    }
    return { version: 1, conversations: [] };
  }

  private save(): void {
    this.deps.storage.setItem(this.deps.recordKey, JSON.stringify(this.record));
    for (const listener of [...this.listeners]) listener();
  }
}

function readBody(
  plaintext: Uint8Array,
): { id: string; at: number; text: string } | null {
  try {
    const body = JSON.parse(fromUtf8(plaintext)) as {
      id?: unknown;
      at?: unknown;
      text?: unknown;
    } | null;
    return typeof body?.id === "string" &&
      typeof body.at === "number" &&
      typeof body.text === "string"
      ? { id: body.id, at: body.at, text: body.text }
      : null;
  } catch {
    return null;
  }
}

const errorText = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);
