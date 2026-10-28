import type { HexString } from "@parity/truapi/scale";
import {
  reactable,
  type ChatEngine,
  type ChatSnapshot,
} from "./nova/engine.js";
import { HOP_MAX_FILE_BYTES } from "./nova/hop.js";
import { CallManager, type CallManagerDeps } from "./nova/call.js";
import type { TurnCredentials, TurnStatus } from "./nova/turn.js";
import {
  abbreviateAccount,
  type Contact,
  type StoredMessage,
} from "./nova/store.js";
import {
  Listeners,
  pageBefore,
  type ChatContact,
  type ChatMessage,
  type ChatProvider,
  type Composer,
  type CallSupport,
  type CallView,
  type ConversationControls,
  type ConversationSummary,
  type FileTransfer,
  type MessageContent,
  type MessageFile,
  type MessageReaction,
  type ProviderAccount,
  type ProviderEvent,
  type ProviderStatus,
  type RequestInbox,
  type StartChat,
  type TimelinePage,
} from "./provider.js";

/**
 * The host's Nova chat as a chat provider: the person-to-person protocol the
 * host speaks itself over the Statement Store (`src/chat/nova/`). A
 * conversation is a contact, keyed by the peer's identity account. Everything
 * the protocol does stays in the engine, and `nova-session.ts` builds it for a
 * session; this file only translates.
 */

export const NOVA_PROVIDER_ID = "nova";

/** The engine calls the provider uses, so tests can stand in for the engine. */
export type NovaEngine = Pick<
  ChatEngine,
  | "snapshot"
  | "onChange"
  | "start"
  | "stop"
  | "accept"
  | "decline"
  | "startChat"
  | "send"
  | "retry"
  | "react"
  | "block"
  | "unblock"
  | "leave"
  | "fileProblem"
  | "fetchProblem"
  | "sendFile"
  | "cancel"
  | "fetchFile"
  | "sendCall"
  | "onCall"
>;

/** An image's size in pixels, when the browser can read it. */
async function imageSize(
  file: File,
): Promise<{ width: number; height: number } | undefined> {
  if (!/^image\/(png|jpeg|gif|webp)$/.test(file.type)) return undefined;
  try {
    const bitmap = await createImageBitmap(file);
    const size = { width: bitmap.width, height: bitmap.height };
    bitmap.close();
    return size;
  } catch {
    return undefined;
  }
}

const displayName = (entry: {
  name: string | null;
  account?: string;
  peer?: string;
}): string =>
  entry.name ?? abbreviateAccount(entry.account ?? entry.peer ?? "");

function preview(contact: Contact): string {
  if (contact.state === "requested")
    return "Waiting for them to accept your request";
  const last = contact.messages.at(-1);
  if (last === undefined) return "";
  if (last.kind === "accepted")
    return last.direction === "out"
      ? "You accepted the request"
      : "Accepted your request";
  if (last.kind === "left") return "Left the chat";
  if (last.kind === "call" && last.call !== undefined)
    return callLine(last.direction === "out", last.call);
  if (last.attachments !== undefined && last.text === "")
    return `${last.direction === "out" ? "You: " : ""}${last.attachments.length === 1 ? "File" : `${last.attachments.length} files`}`;
  return `${last.direction === "out" ? "You: " : ""}${last.text}`;
}

/** A change to any message of a contact, or to its state, changes this. */
const signature = (contact: Contact): string =>
  JSON.stringify([
    contact.state,
    contact.blocked === true,
    contact.peerLeft === true,
    contact.messages.map((message) => [
      message.id,
      message.status,
      message.reactions ?? [],
      message.call ?? null,
    ]),
  ]);

/** One entry per emoji, in the order they were first placed. */
function reactionsOf(message: StoredMessage): MessageReaction[] {
  const byEmoji = new Map<string, MessageReaction>();
  for (const entry of message.reactions ?? []) {
    const known = byEmoji.get(entry.emoji) ?? {
      emoji: entry.emoji,
      count: 0,
      self: false,
    };
    known.count += 1;
    known.self ||= entry.from === "self";
    byEmoji.set(entry.emoji, known);
  }
  return [...byEmoji.values()];
}

const duration = (ms: number): string => {
  const seconds = Math.max(0, Math.round(ms / 1000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
};

/** A call's line in the conversation, read from who offered, answered and ended it. */
export function callLine(
  out: boolean,
  call: NonNullable<StoredMessage["call"]>,
): string {
  const kind = call.video ? "video call" : "audio call";
  const named = `${out ? "Outgoing" : "Incoming"} ${kind}`;
  if (call.answeredAt !== undefined)
    return call.endedAt === undefined
      ? named
      : `${named} · ${duration(call.endedAt - call.answeredAt)}`;
  if (call.closedBy === undefined) return named;
  if (out)
    return call.closedBy === "self" ? `Canceled ${kind}` : `Declined ${kind}`;
  return call.closedBy === "self" ? `Declined ${kind}` : `Missed ${kind}`;
}

/** What a call says about its relay, or null when there is nothing to say. Never the credentials. */
export function relayNote(status: TurnStatus | null): string | null {
  if (status === null)
    return "No relay: only the configured ICE servers are used.";
  switch (status.state) {
    case "ready":
      return null;
    case "idle":
    case "fetching":
      return "Getting relay credentials…";
    case "unavailable":
      return `No relay: ${status.problem.reason} The call uses only direct paths.`;
  }
}

/** Whether the user can still write to a contact. */
const writable = (contact: Contact): boolean =>
  contact.state === "active" &&
  contact.blocked !== true &&
  contact.peerLeft !== true;

export class NovaChatProvider implements ChatProvider {
  readonly id = NOVA_PROVIDER_ID;
  readonly label = "Polkadot chat";
  private readonly listeners = new Listeners<ProviderEvent>();
  private readonly seen = new Map<string, string>();
  private readonly stopListening: () => void;
  private readonly callManager: CallManager | null;
  readonly calls?: CallSupport;

  constructor(
    private readonly engine: NovaEngine,
    private readonly username?: string,
    calls?: Omit<CallManagerDeps, "send">,
    private readonly turn: TurnCredentials | null = null,
  ) {
    for (const contact of engine.snapshot().contacts)
      this.seen.set(contact.account, signature(contact));
    const stops = [engine.onChange(() => this.changed())];
    this.callManager =
      calls === undefined
        ? null
        : new CallManager({
            ...calls,
            send: (account, signal) => engine.sendCall(account, signal),
          });
    if (this.callManager !== null) {
      const manager = this.callManager;
      stops.push(engine.onCall((signal) => manager.receive(signal)));
      this.calls = this.callSupport(manager);
    }
    this.stopListening = () => {
      for (const stop of stops) stop();
    };
  }

  private callSupport(manager: CallManager): CallSupport {
    const session = () => manager.current()?.session;
    return {
      blocked: (account) => {
        const contact = this.contact(account);
        if (contact === undefined || !writable(contact))
          return "This chat cannot take calls now.";
        if (typeof RTCPeerConnection === "undefined")
          return "This browser has no WebRTC.";
        if (this.snapshot.canSend === false) return "No statement allowance.";
        return manager.current()?.session.ended === false
          ? "A call is already on."
          : null;
      },
      start: (account, video) => manager.start(account as HexString, video),
      current: () => {
        const held = manager.current();
        if (held === null) return null;
        const contact = this.contact(held.account);
        const call = held.session;
        const view: CallView = {
          conversationId: held.account,
          title:
            contact === undefined
              ? abbreviateAccount(held.account)
              : displayName(contact),
          video: call.video,
          direction: call.direction,
          state: call.state,
          problem: call.problem,
          muted: call.muted,
          cameraOn: call.cameraOn,
          remoteCamera: call.remoteCamera,
          local: call.local,
          remote: call.remote,
          relay: relayNote(this.turn?.status() ?? null),
        };
        return view;
      },
      accept: async () => session()?.accept(),
      hangUp: () => session()?.hangUp(),
      setMuted: (muted) => session()?.setMuted(muted),
      setCamera: (on) => session()?.setCamera(on),
      dismiss: () => manager.clear(),
      subscribe: (listener) => {
        const stops = [
          manager.subscribe(listener),
          ...(this.turn === null ? [] : [this.turn.onChange(listener)]),
        ];
        return () => {
          for (const stop of stops) stop();
        };
      },
    };
  }

  /** An outgoing call rings once the peer acknowledged its offer. */
  private followCall(): void {
    const held = this.callManager?.current();
    if (held === null || held === undefined) return;
    const offer = this.contact(held.account)?.messages.find(
      (message) => message.id === held.session.offerId,
    );
    if (offer?.status === "acknowledged") held.session.delivered();
  }

  private changed(): void {
    this.followCall();
    for (const contact of this.engine.snapshot().contacts) {
      const now = signature(contact);
      if (this.seen.get(contact.account) === now) continue;
      this.seen.set(contact.account, now);
      this.listeners.emit({
        type: "messages",
        conversationId: contact.account,
      });
    }
    this.listeners.emit({ type: "conversations" });
    this.listeners.emit({ type: "status" });
  }

  private get snapshot(): ChatSnapshot {
    return this.engine.snapshot();
  }

  private contact(account: string): Contact | undefined {
    return this.snapshot.contacts.find(
      (candidate) => candidate.account === account,
    );
  }

  status(): ProviderStatus {
    const status = this.snapshot.status;
    switch (status.state) {
      case "listening":
        return { state: "ready" };
      case "starting":
        return { state: "starting" };
      case "stopped":
        return { state: "offline", reason: "Stopped" };
      case "failed":
        return { state: "failed", reason: status.reason };
    }
  }

  epoch(): number {
    return 0;
  }

  conversations(): ConversationSummary[] {
    return this.snapshot.contacts
      .filter((contact) => contact.blocked !== true)
      .map((contact) => this.summary(contact));
  }

  blocked(): ConversationSummary[] {
    return this.snapshot.contacts
      .filter((contact) => contact.blocked === true)
      .map((contact) => this.summary(contact));
  }

  private summary(contact: Contact): ConversationSummary {
    return {
      id: contact.account,
      title: displayName(contact),
      subtitle: abbreviateAccount(contact.account),
      icon: null,
      seed: contact.account,
      preview: preview(contact),
      updatedAt: contact.messages.at(-1)?.at ?? 0,
    };
  }

  async timeline(account: string, before?: string): Promise<TimelinePage> {
    const contact = this.contact(account);
    if (contact === undefined) return { messages: [] };
    const page = pageBefore(contact.messages, before);
    return {
      messages: page.items.map((message) => this.message(contact, message)),
      before: page.before,
    };
  }

  private message(contact: Contact, message: StoredMessage): ChatMessage {
    const out = message.direction === "out";
    let content: MessageContent;
    if (message.kind === "unsupported")
      content = { tag: "System", text: message.text };
    else if (message.attachments !== undefined)
      content = {
        tag: "Attachments",
        text: message.text === "" ? null : message.text,
        files: message.attachments.map((attachment) => this.file(attachment)),
      };
    else if (message.kind === "call" && message.call !== undefined)
      content = { tag: "System", text: callLine(out, message.call) };
    else if (message.kind === "left")
      content = {
        tag: "System",
        text: out
          ? "You left the chat"
          : `${displayName(contact)} left the chat`,
      };
    else if (message.kind === "accepted")
      content = {
        tag: "System",
        text: out
          ? `You accepted the chat request${message.status === "failed" ? ", but it was not sent" : ""}`
          : `${displayName(contact)} accepted your chat request`,
      };
    else
      content = {
        tag: "Text",
        text: message.text === "" ? "(no message)" : message.text,
        ...(message.kind === "request"
          ? { label: out ? "Your chat request" : "Chat request" }
          : {}),
      };
    return {
      id: message.id,
      at: message.at,
      sender: out
        ? { id: this.snapshot.account, label: "You", self: true }
        : { id: contact.account, label: displayName(contact), self: false },
      status: message.status === "acknowledged" ? "delivered" : message.status,
      ...(message.problem === undefined ? {} : { problem: message.problem }),
      content,
      ...(message.reactions === undefined || message.reactions.length === 0
        ? {}
        : { reactions: reactionsOf(message) }),
      ...(writable(contact) && reactable(message) ? { reactable: true } : {}),
    };
  }

  private file(
    attachment: NonNullable<StoredMessage["attachments"]>[number],
  ): MessageFile {
    return {
      name: attachment.name ?? null,
      kind: attachment.kind,
      mimeType: attachment.mimeType,
      sizeBytes: attachment.size,
      ...(attachment.width === undefined
        ? {}
        : { width: attachment.width, height: attachment.height }),
      unavailable: this.engine.fetchProblem(attachment),
    };
  }

  readonly files: FileTransfer = {
    maxBytes: HOP_MAX_FILE_BYTES,
    blocked: (account) => {
      const contact = this.contact(account);
      if (contact === undefined || !writable(contact))
        return "This chat cannot take files now.";
      return this.engine.fileProblem();
    },
    send: async (account, file, caption) =>
      this.engine.sendFile(
        account as HexString,
        {
          bytes: new Uint8Array(await file.arrayBuffer()),
          name: file.name,
          mimeType: file.type,
          image: await imageSize(file),
        },
        caption,
      ),
    cancel: (_account, messageId) => this.engine.cancel(messageId),
    open: async (account, messageId, index, signal) => {
      const attachment = this.contact(account)?.messages.find(
        (message) => message.id === messageId,
      )?.attachments?.[index];
      const bytes = await this.engine.fetchFile(
        account as HexString,
        messageId,
        index,
        signal,
      );
      return new Blob([bytes as BlobPart], {
        type: attachment?.mimeType || "application/octet-stream",
      });
    },
  };

  composer(account: string): Composer {
    const contact = this.contact(account);
    if (contact?.state === "requested")
      return {
        kind: "blocked",
        reason: `Wait for ${displayName(contact)} to accept your message request.`,
      };
    if (contact?.blocked)
      return { kind: "blocked", reason: "You blocked this person." };
    if (contact?.peerLeft)
      return {
        kind: "blocked",
        reason: `${displayName(contact)} left this chat. They can send a new request to talk again.`,
      };
    if (this.snapshot.canSend === false)
      return { kind: "blocked", reason: "No statement allowance." };
    return { kind: "text" };
  }

  send(account: string, text: string): Promise<void> {
    return this.engine.send(account as HexString, text);
  }

  react(account: string, messageId: string, emoji: string | null) {
    return this.engine.react(account as HexString, messageId, emoji);
  }

  controls(account: string): ConversationControls {
    const contact = this.contact(account);
    if (contact === undefined) return {};
    return {
      block: true,
      leave: contact.state === "active" && contact.blocked !== true,
    };
  }

  block(account: string): Promise<void> {
    return this.engine.block(account as HexString);
  }

  unblock(account: string): Promise<void> {
    return this.engine.unblock(account as HexString);
  }

  leave(account: string): Promise<void> {
    return this.engine.leave(account as HexString);
  }

  /** Nova sends every unacknowledged message of the contact again. */
  retry(account: string): Promise<void> {
    return this.engine.retry(account as HexString);
  }

  readonly start: StartChat = {
    blocked: () => {
      if (this.snapshot.status.state !== "listening") return "Not connected.";
      return this.snapshot.canSend === false ? "No statement allowance." : null;
    },
    start: async (target, welcome) => {
      const before = new Set(
        this.snapshot.contacts.map((contact) => contact.account),
      );
      await this.engine.startChat(target, welcome);
      const contacts = this.snapshot.contacts;
      const added =
        contacts.find((contact) => !before.has(contact.account)) ??
        contacts.at(-1);
      if (added === undefined) throw new Error("The chat was not started.");
      return added.account;
    },
  };

  readonly requests: RequestInbox = {
    list: () =>
      this.snapshot.requests.map((request) => ({
        id: request.id,
        title: displayName(request),
        seed: request.peer,
        preview: request.welcome ?? "Wants to chat",
        at: request.at,
        problem: request.problem,
      })),
    accept: async (requestId) => {
      const request = this.snapshot.requests.find(
        (entry) => entry.id === requestId,
      );
      if (request === undefined) throw new Error("That request is gone.");
      await this.engine.accept(requestId);
      return request.peer;
    },
    decline: (requestId) => this.engine.decline(requestId),
  };

  /**
   * Accepted contacts. A request either side has not accepted yet is not a
   * contact, as on the native hosts, and a declined one is gone. A blocked
   * person is not offered, and a conversation the user left is gone.
   */
  contacts(): ChatContact[] {
    return this.snapshot.contacts
      .filter((contact) => contact.state === "active" && !contact.blocked)
      .map((contact) => {
        const last = contact.messages.at(-1)?.at;
        return {
          account: contact.account,
          name: displayName(contact),
          ...(last === undefined ? {} : { lastActiveAt: last }),
        };
      });
  }

  account(): ProviderAccount {
    const snapshot = this.snapshot;
    const notes: string[] = [];
    if (snapshot.chainKey === "unknown") notes.push("Checking chain…");
    if (snapshot.chainKey === "missing")
      notes.push(
        "No chat key on chain: no one can send this identity a request.",
      );
    if (snapshot.chainKey === "differs")
      notes.push("The chat key on chain is not this wallet's.");
    if (snapshot.chainKey === "conflict")
      notes.push("Different chat keys on Asset Hub and the People chain.");
    if (snapshot.chainKey === "unreadable")
      notes.push("The chat key on chain could not be read.");
    if (snapshot.canSend === false)
      notes.push(
        "No statement allowance: this identity cannot answer or send.",
      );
    return {
      name: `${snapshot.name ?? this.username ?? "No username"} · ${abbreviateAccount(snapshot.account)}`,
      title: `Identity account ${snapshot.account}\nChat key ${snapshot.chatKey}`,
      notes,
      setAside: snapshot.skipped,
    };
  }

  subscribe(listener: (event: ProviderEvent) => void): () => void {
    return this.listeners.add(listener);
  }

  dispose(): void {
    this.callManager?.dispose();
    this.turn?.reset();
    this.stopListening();
    this.listeners.clear();
    this.engine.stop();
  }
}
