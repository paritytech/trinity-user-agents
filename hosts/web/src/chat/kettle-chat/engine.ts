import type { ContactHandle } from "@parity/truapi";
import type {
  HostMessage,
  IncomingRequest,
  OutgoingView,
  RepliedView,
} from "../../invitations/contract.js";
import { toHex, type RandomBytes } from "../../invitations/sealing.js";
import type {
  Conversation,
  ConversationEngine,
  ConversationMessage,
  ContactState,
  MessageState,
  StartTarget,
} from "../draft/conversations.js";
import type { ConversationDiagnostics } from "../../products/diagnostics.js";
import type { RecordStorage } from "../contact-chat/engine.js";
import type { HostRequests } from "../contact-chat/requests.js";
import type {
  IrohStatus,
  KettleEvent,
  KettleModule,
  KettleNode,
  KettlePorts,
} from "./ports.js";
import type { IrohRelays } from "./route.js";

/**
 * PROTOTYPE. Kettle Chat's engine: one-to-one conversations over Kettle
 * (MLS groups over Statement Store outboxes), with people the product knows
 * only by the contact handles its host gives it.
 *
 * Who does what:
 *
 * - The host names, draws and reaches people, and carries the setup through
 *   its request service, as for Contact Chat.
 * - Each side makes a product account at a random `Raw` index for each
 *   conversation, and runs one Kettle node as that account. The setup
 *   payloads carry only that account: `0x4b06` and the 32 bytes. The request
 *   carries the sender's, the reply the recipient's.
 * - The recipient replies when it accepts, then starts its node, which
 *   publishes its key package once the host lets its account post.
 * - The sender, once its host authenticated the reply and the reply checks,
 *   completes the request at once, so a slow or failed setup never leaves it
 *   open. Then it starts its node, which waits for its account's allowance,
 *   and makes the MLS group by sending its first message: the request's
 *   message, or the first one the user writes. The recipient's node joins
 *   from the Welcome; the engine keeps only the direct group of the two
 *   conversation accounts the authenticated setup named, made by the peer,
 *   and leaves any other.
 * - Kettle keeps the MLS state, the outboxes and the acknowledgements, sealed
 *   in the product's storage. This engine keeps the conversations and their
 *   history.
 *
 * Decline is silent and cancel is the host's, as for Contact Chat. There is
 * no Start over.
 */

export const KETTLE_PROTOCOL = "kettle/6 · MLS 0x0003";
const PAYLOAD_TAG = "0x4b06";
const PEER = "peer";
const DEFAULT_PURPOSE = "Wants to chat with you in Kettle Chat.";

type Status =
  | "sending"
  | "waiting"
  | "paused"
  | "accepting"
  /** The recipient replied, and waits for the sender's Welcome. */
  | "joining"
  /**
   * The sender checked the reply and completed the request; its node waits
   * for its account's allowance, then makes the group.
   */
  | "setting-up"
  | "connected"
  | "not-sent"
  | "cancelled";

interface StoredMessage {
  id: string;
  author: "self" | "peer" | "system";
  text: string;
  at: number;
  state: MessageState;
  /** Kettle's number for this side's record, once queued. */
  number?: number;
  /**
   * Written while the recipient waits for the sender's Welcome: held here,
   * not handed to Kettle, until this side joined the direct group the
   * setup named. Shown as sending.
   */
  held?: boolean;
}

interface StoredConversation {
  id: string;
  contact: string;
  role: "sender" | "recipient";
  status: Status;
  createdAt: number;
  request?: string;
  index?: string;
  account?: string;
  /** The other side's conversation account, from the authenticated setup. */
  peer?: string;
  /** The direct MLS group, once made or joined. */
  group?: string;
  /** The request's message, which the sender sends first, until it does. */
  first?: string;
  allowedUntil?: number;
  provisional?: true;
  messages: StoredMessage[];
}

interface StoredRecord {
  version: 1;
  conversations: StoredConversation[];
}

export interface KettleChatDeps {
  requests: HostRequests;
  kettle: KettleModule;
  /** The product's own account at raw index `index`. */
  account(index: string): Promise<string>;
  /** The ports of the conversation account at `index`. */
  ports(
    index: string,
    allow: () => Promise<string | null>,
    lease: () => { until: number; provisional: boolean; running: boolean },
  ): KettlePorts;
  storage: RecordStorage;
  recordKey: string;
  random: RandomBytes;
  now(): number;
  /** Kettle's short intervals, for tests that run in real time. */
  fast?: boolean;
  /**
   * Run Kettle's log transfer on these iroh relays: each node binds an
   * endpoint on them, or runs without one when it reaches none in time.
   * Absent: no endpoint and no relay connection.
   */
  iroh?: IrohRelays & { onlineTimeoutMs?: number };
  report?(reason: string): void;
}

const CONTACT_STATE: Record<Status, ContactState> = {
  sending: "Outgoing",
  waiting: "Outgoing",
  paused: "Outgoing",
  accepting: "Incoming",
  joining: "Established",
  "setting-up": "Established",
  connected: "Established",
  "not-sent": "Invite",
  cancelled: "Invite",
};

const SUBTITLE: Record<Status, string> = {
  sending: "Sending your request",
  waiting: "Request sent",
  paused: "Request paused",
  accepting: "Accepting the request",
  joining: "Setting up the encrypted chat · waiting for their first message",
  "setting-up": "Setting up the encrypted chat",
  connected: "Connected · end-to-end encrypted (Kettle MLS)",
  "not-sent": "Request not sent",
  cancelled: "Request cancelled",
};

const OUTSTANDING: readonly Status[] = ["sending", "waiting", "paused"];
const OPEN: readonly Status[] = [
  ...OUTSTANDING,
  "accepting",
  "joining",
  "setting-up",
  "connected",
];
const RUNNING: readonly Status[] = [
  "accepting",
  "joining",
  "setting-up",
  "connected",
];
/**
 * Where this side may write. The sender's message written while setting up
 * makes the group once its node runs; the recipient's, written while
 * joining, is held until it joined the sender's group (`held`), since a
 * send to Kettle before then would make a second group of its own.
 */
const WRITABLE: readonly Status[] = ["setting-up", "joining", "connected"];

/** The setup payload that names `account`. */
const payloadOf = (account: string) => `${PAYLOAD_TAG}${account.slice(2)}`;

/** The account a setup payload names, or null when it is not Kettle Chat's. */
function accountOf(payload: string): string | null {
  const lower = payload.toLowerCase();
  return lower.startsWith(PAYLOAD_TAG) && /^0x4b06[0-9a-f]{64}$/.test(lower)
    ? `0x${lower.slice(PAYLOAD_TAG.length)}`
    : null;
}

/**
 * Why an Accept of request `handle`, which names the conversation account
 * `peer`, may not replace `known`, the conversation already kept with its
 * sender, or null when it may.
 *
 * - The same request, while its Accept did not get through, is accepted
 *   again; once replied to, it is not.
 * - A chat that runs or sets up is kept: replacing it would leave the other
 *   side with a group nobody answers in.
 * - When both sent a request, both sides keep the request of the lower
 *   conversation account and accept only that one, so two Accepts never
 *   leave both sides waiting for a Welcome.
 */
function acceptRefusal(
  known: StoredConversation,
  handle: string,
  peer: string,
): string | null {
  if (known.role === "recipient" && known.request === handle)
    return known.status === "accepting"
      ? null
      : "You already accepted this request.";
  if (RUNNING.includes(known.status))
    return "You already have a Kettle chat with this person.";
  if (
    OUTSTANDING.includes(known.status) &&
    known.account !== undefined &&
    known.account < peer
  )
    return "You asked them to chat too. Kettle Chat keeps your request: the chat starts when they accept it.";
  return null;
}

/**
 * Whether a `joined` or `restored` event names the conversation's direct
 * group: direct with its peer, its members only this side's account and the
 * peer's, and made by the peer, or by this side for a group it restores.
 * A group joined from a Welcome has both members; one this side made may not
 * have the peer yet.
 */
function isDirectGroupOf(
  conversation: StoredConversation,
  event: KettleEvent,
): boolean {
  const { account, peer } = conversation;
  if (account === undefined || peer === undefined) return false;
  const joined = event.type === "joined";
  const members = Array.isArray(event.members) ? event.members : [];
  const direct = joined ? event.direct === true : event.direct === peer;
  const committers = joined ? [peer] : [peer, account];
  return (
    direct &&
    committers.includes(String(event.committer)) &&
    members.includes(account) &&
    (!joined || members.includes(peer)) &&
    members.every((member) => member === account || member === peer)
  );
}

export class KettleChat implements ConversationEngine {
  private record: StoredRecord;
  private readonly listeners = new Set<() => void>();
  private readonly diagnosticsListeners = new Set<() => void>();
  /** Running nodes, by conversation. */
  private readonly nodes = new Map<string, Promise<KettleNode | null>>();
  /** The nodes of {@link nodes} that started, by conversation. */
  private readonly started = new Map<string, KettleNode>();
  private readonly allowing = new Map<string, Promise<string | null>>();
  /** The account each received request's Accept replies with, by request handle. */
  private readonly acceptSetups = new Map<string, Promise<string | null>>();
  /** What each node told last, by conversation, for the developer tools. */
  private readonly published = new Map<string, number>();
  private readonly delivered = new Map<string, number>();
  private stopped = false;

  constructor(private readonly deps: KettleChatDeps) {
    this.record = this.load();
  }

  run(): void {
    this.deps.requests.onMessage((message) => void this.hostMessage(message));
    for (const conversation of this.record.conversations)
      if (RUNNING.includes(conversation.status)) void this.node(conversation);
  }

  /** Stop every node that started; one still starting is released once it starts. */
  async stop(): Promise<void> {
    this.stopped = true;
    const started = [...this.started.values()];
    this.nodes.clear();
    this.started.clear();
    for (const node of started) await this.release(node);
  }

  /** Stop `node` and free its WebAssembly object, which nothing calls after. */
  private async release(node: KettleNode | null | undefined): Promise<void> {
    if (node === null || node === undefined) return;
    try {
      await node.stop();
    } catch (error) {
      this.deps.report?.(`stop: ${errorText(error)}`);
    } finally {
      node.free();
    }
  }

  stored(): readonly StoredConversation[] {
    return this.record.conversations;
  }

  onChange(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  onDiagnostics(listener: () => void): () => void {
    this.diagnosticsListeners.add(listener);
    return () => this.diagnosticsListeners.delete(listener);
  }

  diagnostics(): ConversationDiagnostics[] {
    return this.record.conversations.map((conversation) => {
      const own = conversation.messages.filter(
        (message) => message.author === "self",
      );
      return {
        id: conversation.id,
        protocol: KETTLE_PROTOCOL,
        state: conversation.status,
        ...(conversation.request === undefined
          ? {}
          : { request: conversation.request }),
        ...(conversation.account === undefined
          ? {}
          : { account: conversation.account }),
        ...(conversation.peer === undefined
          ? {}
          : { peers: [conversation.peer] }),
        logical: {
          sent: own.filter(
            (message) =>
              message.state !== "sending" && message.state !== "failed",
          ).length,
          received: conversation.messages.filter(
            (message) => message.author === "peer",
          ).length,
          failed: own.filter((message) => message.state === "failed").length,
        },
        facts: [
          {
            label: "Kettle",
            value: `${this.deps.kettle.revision.slice(0, 12)} · protocol ${this.deps.kettle.protocol}`,
          },
          ...(conversation.group === undefined
            ? []
            : [{ label: "MLS group", value: conversation.group }]),
          {
            label: "Log recovery",
            value: irohLine(this.started.get(conversation.id)?.iroh() ?? null),
          },
          {
            label: "Outbox",
            value: `${this.published.get(conversation.id) ?? 0} published · ${this.delivered.get(conversation.id) ?? 0} delivered (this run)`,
          },
        ],
      };
    });
  }

  conversations(): Conversation[] {
    return this.record.conversations.map((conversation) => {
      const handle: ContactHandle = {
        bytes: conversation.contact as ContactHandle["bytes"],
      };
      return {
        id: conversation.id,
        title: "Contact",
        subtitle: this.subtitle(conversation),
        participants: [{ id: PEER, name: "Contact", handle }],
        contact: { handle, state: CONTACT_STATE[conversation.status] },
        composer: WRITABLE.includes(conversation.status)
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

  async start(target: StartTarget, welcome: string): Promise<string> {
    if (!("handle" in target))
      throw new Error("Kettle Chat starts chats by contact handle only.");
    const contact = target.handle.bytes.toLowerCase();
    if (!/^0x[0-9a-f]{64}$/.test(contact))
      throw new Error("That is not a contact handle.");
    const known = this.record.conversations.find(
      (entry) => entry.contact === contact,
    );
    if (
      known !== undefined &&
      OPEN.includes(known.status) &&
      known.status !== "sending"
    )
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
    await this.reset(conversation);
    Object.assign(conversation, {
      role: "sender",
      status: "sending",
      index: toHex(this.deps.random(32)),
    });
    const text = welcome.trim();
    if (text !== "") conversation.first = text;
    this.save();
    void this.invite(conversation, text);
    return conversation.id;
  }

  /**
   * End what a conversation ran, before it starts again. Its history stays.
   * The old account stops being the conversation's before anything is
   * awaited, so an operation of it that finishes meanwhile sees it replaced.
   */
  private async reset(conversation: StoredConversation): Promise<void> {
    const { index, account, request } = conversation;
    if (request !== undefined) this.acceptSetups.delete(request);
    for (const key of [
      "request",
      "index",
      "account",
      "peer",
      "group",
      "first",
      "allowedUntil",
    ] as const)
      delete conversation[key];
    const starting = this.nodes.get(conversation.id);
    const started = this.started.get(conversation.id);
    this.nodes.delete(conversation.id);
    this.started.delete(conversation.id);
    // A node still starting sees itself replaced, and releases itself.
    await starting;
    await this.release(started);
    await this.forget(index, account);
  }

  /**
   * Remove the sealed Kettle state of the account at `index`, which the
   * conversation no longer runs: no node of it runs again.
   */
  private async forget(
    index: string | undefined,
    account: string | undefined,
  ): Promise<void> {
    if (index === undefined || account === undefined) return;
    try {
      await this.deps.kettle.removeState(
        account,
        this.deps.ports(
          index,
          async () => "This chat was replaced.",
          () => ({ until: 0, provisional: false, running: false }),
        ),
      );
    } catch (error) {
      this.deps.report?.(`forget: ${errorText(error)}`);
    }
  }

  /** Withdraw a sent request at the host: its reposts stop. */
  private async cancel(handle: string): Promise<void> {
    const result = await this.deps.requests.call({ call: "cancel", handle });
    if (!result.ok) this.deps.report?.(`cancel: ${result.detail}`);
  }

  private async invite(
    conversation: StoredConversation,
    welcome: string,
  ): Promise<void> {
    const index = conversation.index!;
    // A later start or an Accept replaces the conversation's index while
    // this one waits: what this one learns afterwards is not the chat's.
    const current = () =>
      conversation.index === index && conversation.status === "sending";
    const notSent = (why: string) => {
      if (!current()) return;
      conversation.status = "not-sent";
      this.system(conversation, `Your chat request was not sent: ${why}`);
      this.save();
    };
    let account: string;
    try {
      account = await this.deps.account(index);
    } catch (error) {
      this.deps.report?.(`account: ${errorText(error)}`);
      return notSent("Kettle Chat could not get its account for this chat");
    }
    if (!current()) return;
    conversation.account = account;
    if (!(await this.persist()))
      return notSent("Kettle Chat could not save it");
    const result = await this.deps.requests.call({
      call: "request",
      request: {
        contact: { bytes: conversation.contact },
        purpose: welcome === "" ? DEFAULT_PURPOSE : welcome,
        payload: payloadOf(account),
      },
    });
    if (!current()) {
      if (result.ok && result.request !== undefined)
        await this.cancel(result.request.handle);
      return;
    }
    if (result.ok && result.request !== undefined) {
      conversation.request = result.request.handle;
      conversation.status = "waiting";
      this.system(
        conversation,
        welcome === ""
          ? "You sent a chat request"
          : `You sent a chat request: ${welcome}`,
      );
      this.save();
    } else
      notSent(
        !result.ok && result.reason === "Dismissed"
          ? "you did not send it"
          : result.ok
            ? "the host gave no request"
            : result.detail,
      );
  }

  async send(conversationId: string, text: string): Promise<string> {
    const conversation = this.find(conversationId);
    if (conversation === undefined) throw new Error("There is no such chat.");
    if (!WRITABLE.includes(conversation.status))
      throw new Error("The chat is not connected.");
    const body = text.trim();
    if (body === "") throw new Error("The message is empty.");
    const message: StoredMessage = {
      id: toHex(this.deps.random(16)),
      author: "self",
      text: body,
      at: this.deps.now(),
      state: "sending",
    };
    conversation.messages.push(message);
    this.dispatch(conversation, message);
    return message.id;
  }

  /**
   * Hand `message`, sending, to Kettle, or hold it while the recipient
   * waits for the sender's group: {@link sendHeld} hands it on once this
   * side joined that group.
   */
  private dispatch(
    conversation: StoredConversation,
    message: StoredMessage,
  ): void {
    if (conversation.status === "joining") {
      message.held = true;
      return this.save();
    }
    this.save();
    void this.post(conversation, message);
  }

  /**
   * Hand the held messages to Kettle, in the order written, each once: this
   * side joined the direct group the setup named.
   */
  private sendHeld(conversation: StoredConversation): void {
    const held = conversation.messages.filter((message) => message.held);
    if (held.length === 0) return;
    for (const message of held) delete message.held;
    this.save();
    void (async () => {
      for (const message of held) await this.post(conversation, message);
    })();
  }

  async retry(conversationId: string, messageId: string): Promise<void> {
    const conversation = this.find(conversationId);
    const message = conversation?.messages.find(
      (entry) => entry.id === messageId && entry.author === "self",
    );
    if (conversation === undefined || message === undefined)
      throw new Error("There is no such message.");
    if (message.state !== "failed")
      throw new Error("Only a message that failed is sent again.");
    if (!WRITABLE.includes(conversation.status))
      throw new Error("The chat is not connected.");
    message.state = "sending";
    this.dispatch(conversation, message);
  }

  /** Queue `message` in Kettle: it is sent once its record is published. */
  private async post(
    conversation: StoredConversation,
    message: StoredMessage,
  ): Promise<void> {
    const index = conversation.index;
    const node = await this.node(conversation);
    if (
      node === null ||
      conversation.peer === undefined ||
      conversation.index !== index
    ) {
      message.state = "failed";
      return this.save();
    }
    try {
      const sent = await node.sendText(conversation.peer, message.text);
      message.number = sent.number;
      if (conversation.group !== sent.group) conversation.group = sent.group;
      this.groupReady(conversation);
      if ((this.published.get(conversation.id) ?? 0) >= sent.number)
        message.state = "sent";
    } catch (error) {
      this.deps.report?.(`send: ${errorText(error)}`);
      this.system(
        conversation,
        `Your message was not sent: ${errorText(error)}`,
      );
      message.state = "failed";
    }
    this.save();
  }

  /** Null once the host allowed the conversation's account to post now, or why not. */
  private allow(conversation: StoredConversation): Promise<string | null> {
    if (this.stopped) return Promise.resolve("Kettle Chat stopped.");
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

  /** The conversation's running node, started once its account may post; null when it cannot start. */
  private node(conversation: StoredConversation): Promise<KettleNode | null> {
    const running = this.nodes.get(conversation.id);
    if (running !== undefined) return running;
    const { index, account } = conversation;
    // A reset in between ends this start: the conversation runs another account.
    const replaced = () => this.nodes.get(conversation.id) !== starting;
    const starting = (async (): Promise<KettleNode | null> => {
      if (index === undefined || account === undefined) return null;
      const refused = await this.allow(conversation);
      if (replaced()) return null;
      if (refused !== null) {
        this.system(
          conversation,
          `Kettle Chat cannot run this chat: your host did not let it post. ${refused}`,
        );
        this.save();
        return null;
      }
      try {
        const node = await this.deps.kettle.start(
          account,
          this.deps.ports(
            index,
            () => this.allow(conversation),
            () => ({
              until: conversation.allowedUntil ?? 0,
              provisional: conversation.provisional === true,
              running:
                !replaced() &&
                conversation.index === index &&
                RUNNING.includes(conversation.status),
            }),
          ),
          (event) => this.heard(conversation.id, index, event),
          {
            fast: this.deps.fast === true,
            ...(this.deps.iroh === undefined ? {} : { iroh: this.deps.iroh }),
          },
        );
        if (replaced()) {
          await this.release(node);
          return null;
        }
        this.started.set(conversation.id, node);
        // The node runs; the chat is set up once its direct group is made or
        // joined. Saved, the subtitle says what the sender does next.
        this.save();
        return node;
      } catch (error) {
        this.deps.report?.(`start: ${errorText(error)}`);
        return null;
      }
    })();
    this.nodes.set(conversation.id, starting);
    void starting.then((node) => {
      if (node === null && this.nodes.get(conversation.id) === starting)
        this.nodes.delete(conversation.id);
    });
    return starting;
  }

  /** An event of the node the conversation `conversationId` runs at `index`. */
  private heard(
    conversationId: string,
    index: string,
    event: KettleEvent,
  ): void {
    const conversation = this.find(conversationId);
    if (
      this.stopped ||
      conversation === undefined ||
      conversation.index !== index
    )
      return;
    const group = typeof event.group === "string" ? event.group : undefined;
    switch (event.type) {
      case "created":
        if (group === undefined || event.direct !== conversation.peer) break;
        conversation.group = group;
        this.groupReady(conversation);
        break;
      case "joined":
      case "restored": {
        if (group === undefined) return;
        if (!isDirectGroupOf(conversation, event)) {
          this.deps.report?.(
            `left group ${group}: it is not this chat's direct group (committer ${String(event.committer)}, members ${JSON.stringify(event.members)})`,
          );
          this.leave(conversationId, group);
          return;
        }
        if (event.active === false) return;
        conversation.group = group;
        this.groupReady(conversation);
        if (conversation.status === "connected") this.sendHeld(conversation);
        break;
      }
      case "moved":
        if (event.peer === conversation.peer)
          conversation.group = String(event.to);
        break;
      case "message": {
        if (event.sender !== conversation.peer || group !== conversation.group)
          return;
        const id = String(
          event.id ?? `${group}:${conversation.messages.length}`,
        );
        if (conversation.messages.some((message) => message.id === id)) return;
        conversation.messages.push({
          id,
          author: "peer",
          text: String(event.text),
          at: typeof event.at === "number" ? event.at : this.deps.now(),
          state: "received",
        });
        break;
      }
      case "published":
      case "delivered": {
        const count = Number(event.count);
        (event.type === "published" ? this.published : this.delivered).set(
          conversationId,
          count,
        );
        for (const message of conversation.messages)
          if (
            message.author === "self" &&
            message.number !== undefined &&
            message.number <= count
          )
            if (event.type === "delivered") message.state = "delivered";
            else if (message.state === "sending") message.state = "sent";
        break;
      }
      case "rejected":
      case "error":
        this.deps.report?.(
          `kettle ${event.type}: ${String(event.kind ?? event.reason ?? "")} ${String(event.message ?? "")}`.trim(),
        );
        return this.diagnosticsChanged();
      default:
        return this.diagnosticsChanged();
    }
    this.save();
  }

  /** Leave `group` with the conversation's node; a failure is reported, not thrown. */
  private leave(conversationId: string, group: string): void {
    void this.nodes
      .get(conversationId)
      ?.then((node) => node?.leaveGroup(group))
      .catch((error: unknown) =>
        this.deps.report?.(`leave ${group}: ${errorText(error)}`),
      );
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

  /** A sent request changed at the host. Requests this engine did not make are not its own. */
  private outgoing(view: OutgoingView): void {
    const conversation = this.bySentRequest(view.handle);
    if (
      conversation === undefined ||
      !OUTSTANDING.includes(conversation.status)
    )
      return;
    if (view.status === "cancelled") {
      conversation.status = "cancelled";
      this.system(conversation, "You cancelled your chat request");
    } else if (view.status !== "completed")
      conversation.status = view.status === "paused" ? "paused" : "waiting";
    this.save();
  }

  /**
   * An authenticated reply: it names the recipient's conversation account.
   * The sender keeps it, tells the host the request is complete, and then
   * starts its node, which waits for its account's allowance and makes the
   * group with the first message. The request's message shows as sending
   * from the start; it fails, to be retried, if the node cannot start.
   */
  private async response(handle: string, payload: string): Promise<void> {
    const conversation = this.bySentRequest(handle);
    if (conversation === undefined) return;
    if (WRITABLE.includes(conversation.status)) {
      await this.complete(handle);
      return;
    }
    if (conversation.status !== "waiting" && conversation.status !== "paused")
      return;
    const peer = accountOf(payload);
    if (peer === null) {
      this.deps.report?.("a reply is not Kettle Chat's");
      return;
    }
    conversation.peer = peer;
    conversation.status = "setting-up";
    this.system(
      conversation,
      "They accepted your chat request. Setting up the encrypted chat…",
    );
    const first = conversation.first;
    delete conversation.first;
    const message: StoredMessage | undefined =
      first === undefined
        ? undefined
        : {
            id: toHex(this.deps.random(16)),
            author: "self",
            text: first,
            at: this.deps.now(),
            state: "sending",
          };
    if (message !== undefined) conversation.messages.push(message);
    await this.complete(handle);
    if (message !== undefined) await this.post(conversation, message);
    else await this.node(conversation);
  }

  private async complete(handle: string): Promise<void> {
    if (!(await this.persist())) return;
    const result = await this.deps.requests.call({ call: "complete", handle });
    if (!result.ok && result.reason !== "Refused")
      this.deps.report?.(`complete: ${result.detail}`);
  }

  /**
   * The user chose Accept in the host. The engine makes this side's account,
   * replies with it, then starts its node so its key package is up.
   *
   * A conversation already kept with the sender decides first
   * ({@link acceptRefusal}). When this request replaces one this side sent
   * them, that request is withdrawn at the host, so it is not posted again.
   *
   * The host holds an Accept only briefly, so the user may Accept the same
   * request again while an earlier Accept of it still runs. That one keeps
   * the conversation: both use the one account its setup makes, and each
   * replies with it, so whichever reply the host takes names the account
   * this side runs.
   */
  private async accept(request: IncomingRequest): Promise<void> {
    const { handle } = request;
    const refuse = (reason: string) =>
      this.deps.requests.call({ call: "refuse", handle, reason });
    const peer = accountOf(request.payload);
    if (peer === null) {
      await refuse(
        "Kettle Chat cannot use this request: it was not made by Kettle Chat.",
      );
      return;
    }
    const contact = request.sender.bytes.toLowerCase();
    const known = this.record.conversations.find(
      (entry) => entry.contact === contact,
    );
    const again =
      known?.role === "recipient" &&
      known.request === handle &&
      known.status === "accepting";
    let conversation: StoredConversation;
    let setup: Promise<string | null>;
    if (again) {
      conversation = known;
      setup =
        this.acceptSetups.get(handle) ??
        this.setUpAccept(conversation, handle, refuse, Promise.resolve());
    } else {
      const refusal =
        known === undefined ? null : acceptRefusal(known, handle, peer);
      if (refusal !== null) {
        await refuse(refusal);
        return;
      }
      const withdrawn =
        known?.role === "sender" && OUTSTANDING.includes(known.status)
          ? known.request
          : undefined;
      conversation = known ?? {
        id: toHex(this.deps.random(16)),
        contact,
        role: "recipient",
        status: "accepting",
        createdAt: this.deps.now(),
        messages: [],
      };
      if (known === undefined) this.record.conversations.push(conversation);
      // The reset gives up the old account at once; the conversation is this
      // request's before anything is awaited, so an Accept of it again finds
      // it, and an operation of the old account finds it replaced.
      const ended = this.reset(conversation);
      Object.assign(conversation, {
        role: "recipient",
        status: "accepting",
        request: handle,
        index: toHex(this.deps.random(32)),
        peer,
      });
      if (withdrawn !== undefined)
        this.system(
          conversation,
          "You accepted their chat request, so yours was withdrawn",
        );
      setup = this.setUpAccept(
        conversation,
        handle,
        refuse,
        ended.then(() =>
          withdrawn === undefined ? undefined : this.cancel(withdrawn),
        ),
      );
    }
    const account = await setup;
    if (account === null) return;
    // The host holds an Accept only briefly, and the account's allowance may
    // take longer to become usable: reply first, then start the node, whose key
    // package the sender's node waits for.
    const result = await this.deps.requests.call({
      call: "reply",
      handle,
      payload: payloadOf(account),
      open: true,
    });
    if (!result.ok) {
      this.deps.report?.(`reply: ${result.detail}`);
      return;
    }
    if (conversation.request !== handle || conversation.account !== account)
      return;
    this.joining(conversation);
    if ((await this.node(conversation)) === null) {
      this.system(
        conversation,
        "Kettle Chat could not start the encrypted chat. Reload to try again.",
      );
      this.save();
    }
  }

  /**
   * The account the conversation, accepting request `handle`, replies with:
   * the one it holds, or one it derives and keeps first. Null when it cannot,
   * once it refused the request saying why. Every Accept of the request
   * shares it while it runs, and after it gave an account.
   */
  private setUpAccept(
    conversation: StoredConversation,
    handle: string,
    refuse: (reason: string) => Promise<unknown>,
    ready: Promise<unknown>,
  ): Promise<string | null> {
    const { index } = conversation;
    const owned = () =>
      conversation.request === handle && conversation.index === index;
    const refused = async (reason: string): Promise<null> => {
      await refuse(
        owned() ? reason : "Another request to this person replaced this one.",
      );
      return null;
    };
    const setup = (async (): Promise<string | null> => {
      await ready;
      if (index === undefined) return refused("This chat has no account.");
      let account = conversation.account;
      if (account === undefined)
        try {
          account = await this.deps.account(index);
        } catch (error) {
          this.deps.report?.(`account: ${errorText(error)}`);
          return refused(
            "Kettle Chat could not get its account for this chat.",
          );
        }
      if (!owned()) return refused("");
      conversation.account = account;
      const saved = await this.persist();
      if (!owned()) return refused("");
      if (!saved)
        return refused("Kettle Chat could not save the chat. Try again.");
      return account;
    })();
    this.acceptSetups.set(handle, setup);
    void setup.then((account) => {
      if (account === null && this.acceptSetups.get(handle) === setup)
        this.acceptSetups.delete(handle);
    });
    return setup;
  }

  /** The host took this side's reply. */
  private replied(view: RepliedView): void {
    const conversation = this.record.conversations.find(
      (entry) => entry.role === "recipient" && entry.request === view.handle,
    );
    if (conversation?.status === "accepting") this.joining(conversation);
  }

  /**
   * The direct group the setup named is made or joined: the chat is set up.
   * Only Kettle's group, never the node starting, says so: the sender's node
   * makes the group with its first message, and the recipient's joins it
   * from the Welcome.
   */
  private groupReady(conversation: StoredConversation): void {
    if (conversation.group === undefined) return;
    if (
      conversation.status !== "setting-up" &&
      conversation.status !== "joining"
    )
      return;
    conversation.status = "connected";
    this.system(conversation, "The encrypted chat is set up");
  }

  /**
   * What the chat list says under the chat. A sender whose node runs, with
   * no group and nothing on its way, makes the group by writing.
   */
  private subtitle(conversation: StoredConversation): string {
    if (
      conversation.status === "setting-up" &&
      conversation.group === undefined &&
      this.started.has(conversation.id) &&
      !conversation.messages.some(
        (message) => message.author === "self" && message.state === "sending",
      )
    )
      return "Write a message to start the encrypted chat";
    return SUBTITLE[conversation.status];
  }

  private joining(conversation: StoredConversation): void {
    if (conversation.status !== "accepting") return;
    conversation.status =
      conversation.group === undefined ? "joining" : "connected";
    this.system(conversation, "You accepted the chat request");
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
      if (saved?.version === 1 && Array.isArray(saved.conversations)) {
        for (const conversation of saved.conversations) {
          // Earlier builds said a sender was connected once its node ran,
          // before any group: it still sets up, and its next message makes it.
          if (
            conversation.role === "sender" &&
            conversation.status === "connected" &&
            conversation.group === undefined
          )
            conversation.status = "setting-up";
          for (const message of conversation.messages)
            if (
              message.state === "sending" &&
              message.number === undefined &&
              message.held !== true
            )
              message.state = "failed";
        }
        return saved;
      }
    } catch {
      this.deps.report?.("the stored record does not read; starting empty");
    }
    return { version: 1, conversations: [] };
  }

  private save(): void {
    this.deps.storage.setItem(this.deps.recordKey, JSON.stringify(this.record));
    for (const listener of [...this.listeners]) listener();
    this.diagnosticsChanged();
  }

  private diagnosticsChanged(): void {
    for (const listener of [...this.diagnosticsListeners]) listener();
  }

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

/** What a node runs of the log transfer, for the developer tools. */
function irohLine(status: IrohStatus | null): string {
  if (status === null) return "no node runs";
  switch (status.state) {
    case "off":
      return "off · Statement Store only";
    case "on":
      return `iroh via ${status.relay} (${status.relays === "public" ? "public test relays" : "custom relay"}) · endpoint ${status.endpoint.slice(0, 10)}`;
    case "unavailable":
      return `unavailable, Statement Store only: ${status.reason}${status.relay === undefined ? "" : ` (${status.relay})`}`;
  }
}

const errorText = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);
