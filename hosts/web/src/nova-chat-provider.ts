import { bytesToHex, hexToBytes } from "@parity/truapi/scale";
import type { HexString } from "@parity/truapi/scale";
import type { ChainProvider } from "@parity/truapi-host";
import type { ChatDiagnostics } from "./chat/diagnostics.js";
import { chainDirectory } from "./chat/directory.js";
import { ChatEngine, type ChatSnapshot } from "./chat/engine.js";
import { chatRefusal } from "./chat/gate.js";
import { ChatKeys } from "./chat/keys.js";
import { StatementStore } from "./chat/statement-store.js";
import {
  chatRecordKey,
  type Contact,
  type StoredMessage,
} from "./chat/store.js";
import {
  Listeners,
  pageBefore,
  type ChatContact,
  type ChatMessage,
  type ChatProvider,
  type Composer,
  type ConversationSummary,
  type MessageContent,
  type ProviderAccount,
  type ProviderEvent,
  type ProviderStatus,
  type RequestInbox,
  type StartChat,
  type TimelinePage,
} from "./chat-provider.js";

/**
 * The host's Nova chat as a chat provider: the person-to-person protocol the
 * host speaks itself over the Statement Store (`src/chat/`). A conversation is
 * a contact, keyed by the peer's identity account. Everything the protocol
 * does stays in the engine; this file only translates.
 */

export const NOVA_PROVIDER_ID = "nova";

/** What chat needs from the signed-in session. All of it is public except the entropy. */
export interface ChatSession {
  walletId: string;
  entropy: Uint8Array;
  network: string;
  networkSuffix: string;
  /** What the core reports for the session, to check the host derives the same keys. */
  identityAccountId: HexString | undefined;
  chatPublicKey: HexString | undefined;
  /** The username the host found for the session on the People chain, to show when Asset Hub names none. */
  username: string | undefined;
  chain: ChainProvider;
  genesis: { assetHub: HexString; people: HexString };
}

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
>;

const errorText = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

const abbreviate = (account: string): string =>
  `${account.slice(0, 8)}…${account.slice(-6)}`;

const displayName = (entry: {
  name: string | null;
  account?: string;
  peer?: string;
}): string => entry.name ?? abbreviate(entry.account ?? entry.peer ?? "");

/**
 * Derive the session's keys, check them against what the core reports, and
 * start the engine. Returns why chat stays off instead, when it does.
 */
export function startNovaChat(
  session: ChatSession,
  deps: { storage: Storage; diagnostics: ChatDiagnostics },
): NovaChatProvider | string {
  const refusal = chatRefusal(session.entropy);
  if (refusal !== null) return refusal;
  let keys: ChatKeys;
  try {
    keys = ChatKeys.fromEntropy(session.entropy, session.networkSuffix);
  } catch (error) {
    return `Chat keys could not be derived: ${errorText(error)}`;
  }
  if (
    session.identityAccountId === undefined ||
    session.chatPublicKey === undefined
  )
    return "The core did not report this session's identity account and chat key, so chat cannot check its keys.";
  const mismatch = [
    session.identityAccountId !== bytesToHex(keys.identity)
      ? "identity account"
      : null,
    session.chatPublicKey !== bytesToHex(keys.chatPublic) ? "chat key" : null,
  ].filter((part) => part !== null);
  if (mismatch.length > 0)
    return `The host derived a different ${mismatch.join(" and ")} than the core. Chat stays off.`;
  const engine = new ChatEngine({
    keys,
    openStore: async () =>
      new StatementStore(
        await session.chain.connect(hexToBytes(session.genesis.people)),
        deps.diagnostics.payloads.connectionTap(),
      ),
    directory: chainDirectory(session.chain, session.genesis),
    storage: deps.storage,
    recordKey: chatRecordKey(session.network, session.walletId),
    writes: () => true,
    diagnostics: deps.diagnostics.record,
    payloads: deps.diagnostics.payloads,
  });
  const provider = new NovaChatProvider(engine, session.username);
  void engine.start();
  return provider;
}

function preview(contact: Contact): string {
  if (contact.state === "requested")
    return "Waiting for them to accept your request";
  const last = contact.messages.at(-1);
  if (last === undefined) return "";
  if (last.kind === "accepted")
    return last.direction === "out"
      ? "You accepted the request"
      : "Accepted your request";
  return `${last.direction === "out" ? "You: " : ""}${last.text}`;
}

/** A change to any message of a contact changes this. */
const signature = (contact: Contact): string =>
  contact.messages.map((message) => `${message.id}:${message.status}`).join();

export class NovaChatProvider implements ChatProvider {
  readonly id = NOVA_PROVIDER_ID;
  readonly label = "Polkadot chat";
  private readonly listeners = new Listeners<ProviderEvent>();
  private readonly seen = new Map<string, string>();
  private readonly stopListening: () => void;

  constructor(
    private readonly engine: NovaEngine,
    private readonly username?: string,
  ) {
    for (const contact of engine.snapshot().contacts)
      this.seen.set(contact.account, signature(contact));
    this.stopListening = engine.onChange(() => this.changed());
  }

  private changed(): void {
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
    return this.snapshot.contacts.map((contact) => ({
      id: contact.account,
      title: displayName(contact),
      subtitle: abbreviate(contact.account),
      icon: null,
      seed: contact.account,
      preview: preview(contact),
      updatedAt: contact.messages.at(-1)?.at ?? 0,
    }));
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
    };
  }

  composer(account: string): Composer {
    const contact = this.contact(account);
    if (contact?.state === "requested")
      return {
        kind: "blocked",
        reason: `Wait for ${displayName(contact)} to accept your message request.`,
      };
    if (this.snapshot.canSend === false)
      return { kind: "blocked", reason: "No statement allowance." };
    return { kind: "text" };
  }

  send(account: string, text: string): Promise<void> {
    return this.engine.send(account as HexString, text);
  }

  /** Nova sends every unacknowledged message of the contact again. */
  retry(account: string): Promise<void> {
    return this.engine.retry(account as HexString);
  }

  readonly start: StartChat = {
    placeholder: "Username, such as name.01, or a 0x account id",
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
   * contact, as on the native hosts, and a declined one is gone.
   */
  contacts(): ChatContact[] {
    return this.snapshot.contacts
      .filter((contact) => contact.state === "active")
      .map((contact) => ({
        account: contact.account,
        name: displayName(contact),
      }));
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
      name: `${snapshot.name ?? this.username ?? "No username"} · ${abbreviate(snapshot.account)}`,
      title: `Identity account ${snapshot.account}\nChat key ${snapshot.chatKey}`,
      notes,
      setAside: snapshot.skipped,
    };
  }

  subscribe(listener: (event: ProviderEvent) => void): () => void {
    return this.listeners.add(listener);
  }

  dispose(): void {
    this.stopListening();
    this.listeners.clear();
    this.engine.stop();
  }
}
