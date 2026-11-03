import type {
  NotificationClaim,
  StatementAccountClaim,
} from "@parity/truapi-host/web";
import { bytesToHex, type HexString } from "@parity/truapi/scale";
import { ask } from "../app/prompt.js";
import type { PromptField } from "../app/reviews.js";
import type { ChatV2ProviderOptions } from "../chat/draft/provider.js";
import {
  CONTACT_CHAT_PATH,
  type ContactChatWorkerAnswer,
} from "../chat/contact-chat/route.js";
import type { HandleNames } from "../contacts/contacts.js";
import type { NotificationStore } from "../notifications/notifications.js";
import type { NotificationActions } from "../notifications/screen.js";
import type { ChainDirectory } from "../platform/chain-directory.js";
import type { StatementStoreLike } from "../platform/statement-store.js";
import {
  parseWorkerManifest,
  type ResolvedWorker,
} from "../products/worker-manifest.js";
import type { ReplySuite } from "./binding.js";
import { sessionChatKeys } from "./chat-keys.js";
import {
  startContactChatHost,
  type ContactChatHost,
} from "./contact-chat-host.js";
import { DirectoryContacts } from "./directory-contacts.js";
import type { AcceptPrompt, SendPrompt } from "./invitation-service.js";
import type { InvitationObserver } from "./observer.js";
import type { ConversationAllowance, HostDelivery, Product } from "./ports.js";
import { ownIdentityProblem } from "./published-keys.js";
import { StatementStoreDelivery } from "./statement-delivery.js";
import { WalletIdentityAuth } from "./wallet-auth.js";

/**
 * Contact Chat for the signed-in wallet: the host's own chat product, run for
 * every wallet that signs in, with requests proved by the wallet's keys and
 * carried on the network's Statement Store.
 *
 * Signing in starts Contact Chat's worker and its request service once the
 * wallet's keys are the ones the core and the chain report. The service then
 * reads the network's day inbox and posts again the wallet's own pending
 * requests. Nothing new leaves the host without the user: a request goes only
 * after the Send prompt, a reply only after Accept, and each says what it
 * claims of the person's allowance. Chat messages are not encrypted.
 *
 * The dev server's tools may choose another delivery for intentional
 * debugging (claims only validated, or requests kept between tabs of this
 * browser), or no Contact Chat at all, for tests of other products.
 */

/**
 * How the wallet's requests travel: `network` claims and posts for real,
 * `validate` uses the network delivery with every claim only validated,
 * `local` keeps requests between tabs of this browser, and `local-validate`
 * does so and only validates each chat account's claim.
 */
export type WalletDelivery =
  | "network"
  | "validate"
  | "local"
  | "local-validate";

/** Contact Chat's product id and name on a network. */
export const contactChat = (networkSuffix: string): Product => ({
  id: `contact-chat.${networkSuffix}`,
  name: "Contact Chat",
});

/** A wallet session the core reports, as Contact Chat's requests need it. */
export interface WalletRequestsSession {
  walletId: string;
  entropy: Uint8Array;
  network: string;
  networkSuffix: string;
  identityAccountId: string | undefined;
  chatPublicKey: string | undefined;
  /** The chain records chat keys are read from. */
  directory: ChainDirectory;
  /** Open the People chain's Statement Store, for the network delivery. */
  openStatementStore(): Promise<StatementStoreLike>;
  /** Claim, or with `submit` false only validate, a notification account for today. */
  claimNotificationAccount(
    target: Uint8Array,
    submit: boolean,
  ): Promise<NotificationClaim>;
  /** Give a product's own account at `Raw(index)` a Statement Store slot for today, or with `submit` false only validate the claim. */
  claimProductStatementAccount(
    productId: string,
    index: Uint8Array,
    submit: boolean,
  ): Promise<StatementAccountClaim>;
}

/** What the host lends a worker frame of one product over one port. */
export interface WorkerPortService {
  connect(productId: string, port: MessagePort): void;
  disconnect(productId: string): void;
}

/** Contact Chat as the rest of the page uses it, whoever's session it runs for. */
export interface ContactChatOwner {
  /**
   * The people the host may name by handle beyond the wallet's contacts, by
   * account: whoever the user's requests are with. Naming them does not
   * resolve them for a transaction.
   */
  people(): readonly HexString[];
  /** What the Inbox can do with Contact Chat's requests. */
  notificationActions: NotificationActions;
  /** The workers this adds to the session: Contact Chat, while it runs. */
  workerNames(): string[];
  /** Whether `name` is Contact Chat's worker, which the host serves itself. */
  resolvesWorker(name: string): boolean;
  /** Contact Chat's worker, as the host describes the archive it serves. */
  resolveWorker(): Promise<ResolvedWorker>;
  /** Whether `productId` may give its rooms contacts: Contact Chat. */
  isContactChat(productId: string): boolean;
  /** What the host lends Contact Chat's chat provider, or null for another product. */
  chatOptions(productId: string): ChatV2ProviderOptions | null;
  /** The request service, for Contact Chat's worker frame only. */
  requests: WorkerPortService;
}

/** Contact Chat's worker, as the host describes the archive it serves under `base`. */
export async function resolveContactChatWorker(
  base: string,
): Promise<ResolvedWorker> {
  const response = await fetch(`${base}${CONTACT_CHAT_PATH}`, {
    cache: "no-store",
  });
  if (!response.ok)
    throw new Error(
      `This host does not serve Contact Chat: ${response.status} ${await response.text()}`,
    );
  const answer = (await response.json()) as ContactChatWorkerAnswer;
  return {
    record: answer.record,
    cid: answer.cid,
    kind: answer.kind,
    manifest: parseWorkerManifest(answer.manifest),
    gateway: window.location.origin,
  };
}

/** The {@link ContactChatOwner} of one {@link ContactChatHost}, or of none. */
export function contactChatOwner(options: {
  base: string;
  chat: () => ContactChatHost | null;
  /** Calls back whenever the sent requests may have changed, also when the chat itself changes. */
  onSentChange(listener: () => void): () => void;
}): ContactChatOwner {
  const { chat } = options;
  const isChat = (productId: string) => chat()?.product.id === productId;
  return {
    people: () => (chat()?.people() ?? []) as HexString[],
    notificationActions: {
      canAccept: (item) => chat()?.owns(item.id) ?? false,
      accept: (item) => {
        const current = chat();
        return current === null
          ? Promise.reject(new Error("No wallet is signed in."))
          : current.accept(item);
      },
      destination: (item) => (chat()?.owns(item.id) === true ? "Chat" : null),
      open: (item) => {
        const current = chat();
        if (item.state.tag === "accepted" && current?.owns(item.id) === true)
          current.open(item);
      },
      sent: () => chat()?.sent() ?? [],
      cancel: (id) => void chat()?.cancel(id),
      onSentChange: options.onSentChange,
    },
    workerNames: () => {
      const current = chat();
      return current === null ? [] : [current.product.id];
    },
    resolvesWorker: isChat,
    resolveWorker: () => resolveContactChatWorker(options.base),
    isContactChat: isChat,
    chatOptions: (productId) =>
      isChat(productId) ? (chat()?.chatOptions ?? null) : null,
    requests: {
      connect: (productId, port) => {
        const current = chat();
        if (current !== null && isChat(productId)) current.attachChat(port);
        else port.close();
      },
      disconnect: (productId) => {
        if (isChat(productId)) chat()?.detachChat();
      },
    },
  };
}

export interface WalletChatDeps {
  /** The host page's base path. */
  base: string;
  notifications: NotificationStore;
  /** The host's names for the contact handles the core mints. */
  handles: HandleNames;
  /** The accounts the signed-in wallet blocked. */
  blocked(): readonly string[];
  /** The host's names for people, by account, read from its records and the chain. */
  names: {
    name(account: string): string | null;
    onChange(listener: () => void): () => void;
  };
  /** Tell the host's handle index that the people it may name changed. */
  namedChanged(): void;
  /** Bring Chat to the front, at Contact Chat's request. */
  showChat(): void;
  log(line: string): void;
  /** Told the request service's and the delivery's facts, for the inspector's Statements view. */
  observe?: InvitationObserver;
  /** The reply-key suite of each new request. Absent: the request service's default. */
  replySuite?(): ReplySuite;
  /**
   * Another delivery a developer chose on the dev server, or `off` for no
   * Contact Chat, with the simulated delivery between tabs the local choices
   * use. Absent: the network.
   */
  devDelivery?: {
    mode(): WalletDelivery | "off";
    local(self: string): HostDelivery;
  };
}

/** Contact Chat for whichever wallet is signed in. */
export class WalletChat {
  /** Contact Chat as the page uses it, for the wallet signed in now. */
  readonly owner: ContactChatOwner;
  private current: { key: string; chat: ContactChatHost } | null = null;
  private readonly sentListeners = new Set<() => void>();
  private stopSent = () => {};

  constructor(private readonly deps: WalletChatDeps) {
    this.owner = contactChatOwner({
      base: deps.base,
      chat: () => this.current?.chat ?? null,
      onSentChange: (listener) => {
        this.sentListeners.add(listener);
        return () => this.sentListeners.delete(listener);
      },
    });
  }

  /** How the wallet's requests travel in this page, or `off` when Contact Chat does not run. */
  mode(): WalletDelivery | "off" {
    return this.deps.devDelivery?.mode() ?? "network";
  }

  /**
   * Whether the wallet's saved contacts stay out of the picker, and so out
   * of payments: whenever requests reach people the network names.
   */
  hidesWalletContacts(): boolean {
    const mode = this.mode();
    return mode !== "local" && mode !== "off";
  }

  /**
   * Follow the signed-in wallet: Contact Chat runs for it, or ends with null.
   * Call it before the session's workers start, so they see Contact Chat
   * among them.
   */
  setWallet(session: WalletRequestsSession | null): void {
    const key =
      session === null
        ? null
        : `${session.network}:${session.walletId}:${session.identityAccountId ?? ""}:${session.chatPublicKey ?? ""}`;
    if (key === (this.current?.key ?? null)) return;
    this.stopSent();
    this.stopSent = () => {};
    this.current?.chat.stop();
    this.current = null;
    this.sentChanged();
    if (session === null || key === null) return;
    const mode = this.mode();
    if (mode === "off") return;
    const started = this.start(session, mode);
    if (started === null) return;
    this.current = { key, chat: started };
    this.stopSent = started.onSentChange(() => this.sentChanged());
    void started.start().then(() => this.sentChanged());
  }

  private sentChanged(): void {
    for (const listener of [...this.sentListeners]) listener();
  }

  /**
   * Contact Chat for `session`, over the keys its entropy derives, once they
   * are the keys the core reports for the session; null, with the reason
   * logged, when they are not. People are any identity with a chat key the
   * chain publishes, but the blocked: whoever the user picks to write to,
   * and whoever writes to them. Names come from the host's own records and
   * the chain, never from a request.
   */
  private start(
    session: WalletRequestsSession,
    mode: WalletDelivery,
  ): ContactChatHost | null {
    const { deps } = this;
    const keys = sessionChatKeys({
      entropy: session.entropy,
      networkSuffix: session.networkSuffix,
      identityAccountId: session.identityAccountId as HexString | undefined,
      chatPublicKey: session.chatPublicKey as HexString | undefined,
    });
    if (typeof keys === "string") {
      deps.log(`Contact Chat does not run for this wallet: ${keys}`);
      return null;
    }
    const own = bytesToHex(keys.identity).toLowerCase();
    const contacts = new DirectoryContacts({
      handles: deps.handles,
      directory: session.directory,
      person: (account, published) => {
        const key = account.toLowerCase();
        if (
          key === own ||
          deps.blocked().some((blocked) => blocked.toLowerCase() === key)
        )
          return undefined;
        const name =
          deps.names.name(account) ??
          published ??
          `${account.slice(0, 8)}…${account.slice(-6)}`;
        return { id: key, name, handle: name };
      },
      log: deps.log,
    });
    const list = (part: "sent" | "received") =>
      `truapi-web-signing-host:network:${session.network}:requests:${session.walletId}:contact-chat:${part}:v1`;
    // The network delivery keeps its own lists: its requests name the day
    // inbox as their return route, which the local delivery cannot reach.
    const local = mode === "local" || mode === "local-validate";
    const lists = (part: "sent" | "received") =>
      local ? list(part) : `${list(part)}:day-inbox`;
    deps.log(DELIVERY_LINE[mode]);
    const costs = chatCosts(mode);
    const self = bytesToHex(keys.chatPublic);
    return startContactChatHost({
      product: contactChat(session.networkSuffix),
      keys: { sent: lists("sent"), received: lists("received") },
      auth: new WalletIdentityAuth(keys),
      ...(deps.replySuite === undefined ? {} : { replySuite: deps.replySuite }),
      ...(mode === "local"
        ? {}
        : { accounts: conversationAllowance(session, mode === "network") }),
      delivery:
        local && deps.devDelivery !== undefined
          ? deps.devDelivery.local(self)
          : new StatementStoreDelivery({
              network: session.network,
              entropy: session.entropy,
              openStore: () => session.openStatementStore(),
              claim: (target, submit) =>
                session.claimNotificationAccount(target, submit),
              submit: mode === "network",
              now: () => Date.now(),
              schedule: (task, delayMs) => {
                const timer = setTimeout(task, Math.min(delayMs, 2 ** 31 - 1));
                return () => clearTimeout(timer);
              },
              log: deps.log,
              ...(deps.observe === undefined ? {} : { observe: deps.observe }),
            }),
      contacts,
      handles: deps.handles,
      onNamesChange: (listener) => deps.names.onChange(listener),
      notifications: deps.notifications,
      verify: () => ownIdentityProblem(session.directory, keys),
      confirmSend: (prompt) => confirmSend(prompt, costs),
      ...(costs === null
        ? {}
        : {
            confirmAccept: (prompt: AcceptPrompt) =>
              confirmAccept(prompt, costs),
          }),
      showChat: deps.showChat,
      onSenders: deps.namedChanged,
      log: deps.log,
      ...(deps.observe === undefined ? {} : { observe: deps.observe }),
    });
  }
}

const DELIVERY_LINE: Record<WalletDelivery, string> = {
  network:
    "Contact Chat's requests go on the network's day inbox: each post claims a notification slot.",
  validate:
    "Contact Chat's requests use the network delivery with claims only validated: nothing is submitted.",
  local: "Contact Chat's requests stay between tabs of this browser.",
  "local-validate":
    "Contact Chat's requests stay in this browser, and its chat accounts' claims are only validated: nothing is submitted.",
};

/**
 * How a wallet's Contact Chat gets its conversation accounts an allowance:
 * a general Statement Store slot of the wallet's personhood for the UTC day,
 * which the core claims for Contact Chat's own account at the index it names.
 * A tab that only validates claims asks the runtime whether it would accept
 * the claim and submits nothing, so a message there fails and says so.
 */
function conversationAllowance(
  session: WalletRequestsSession,
  submit: boolean,
): ConversationAllowance {
  return {
    async allow(product, index) {
      const claim = await session.claimProductStatementAccount(
        product,
        index,
        submit,
      );
      if (claim.outcome === "validated")
        throw new Error(
          `This tab only validates claims: the runtime would give this chat's account ${claim.account} slot ${claim.seq} of ${claim.collection}, and nothing was claimed.`,
        );
      return {
        account: claim.account,
        until: (claim.period + 1) * 86_400_000,
      };
    },
  };
}

/** What a wallet's Contact Chat spends of the person's allowance, and what it exposes, for the host's prompts. */
export interface ChatCosts {
  /** The one muted line both prompts show, which opens the breakdown. */
  summary: string;
  /** What sending a request spends. */
  request: string;
  /** What replying to a request spends. */
  reply: string;
  /** What the chat that follows spends. */
  chat: string;
  /** Who can read the chat's messages, or null when none are sent. */
  messages: string | null;
}

/** The chat's account and its allowance, the same in every delivery that gives one. */
const CHAT_ACCOUNT_COST =
  "1 Statement Store slot each day you send, for this chat's own account.";

const CANCEL = "Cancel it under Sent in your Inbox.";

/** What `mode` spends and exposes, or null when it spends nothing. */
export function chatCosts(mode: WalletDelivery): ChatCosts | null {
  switch (mode) {
    case "local":
      return null;
    case "local-validate":
      return {
        summary: "Uses chat slots, only validated",
        request: "Nothing: the request stays in this browser.",
        reply: "Nothing: the reply stays in this browser.",
        chat: `${CHAT_ACCOUNT_COST} Only validated: nothing is submitted or sent.`,
        messages: null,
      };
    case "validate":
      return {
        summary: "Uses daily slots, only validated",
        request: "1 notification slot, only validated: nothing is submitted.",
        reply: "1 notification slot, only validated: nothing is submitted.",
        chat: `${CHAT_ACCOUNT_COST} Only validated: nothing is submitted.`,
        messages: null,
      };
    case "network":
      return {
        summary: "Uses daily slots · Messages are public",
        request:
          "1 notification slot now, and 1 more each day it is pending while this host is open.",
        reply:
          "1 notification slot now, and 1 more on any later day their request arrives again.",
        chat: CHAT_ACCOUNT_COST,
        messages: "Not encrypted: anyone can read them.",
      };
  }
}

/** The chat's costs, under the request's or reply's own line, folded under the summary. */
function costDetails(
  first: PromptField,
  costs: ChatCosts,
): { summary: string; fields: PromptField[] } {
  return {
    summary: costs.summary,
    fields: [
      first,
      { label: "Chat", value: costs.chat },
      ...(costs.messages === null
        ? []
        : [{ label: "Messages", value: costs.messages }]),
    ],
  };
}

/** The host's question before a product's invitation leaves. */
export function confirmSend(
  prompt: SendPrompt,
  costs: ChatCosts | null = null,
): Promise<boolean> {
  return ask<boolean>({
    title: "Send invitation",
    from: prompt.product.id,
    fields: [
      { label: "To", value: prompt.to.name },
      { label: "Message", value: prompt.purpose },
    ],
    details:
      costs === null
        ? {
            summary: "Details",
            fields: [{ label: "Invitation", value: CANCEL }],
          }
        : costDetails(
            { label: "Invitation", value: `${costs.request} ${CANCEL}` },
            costs,
          ),
    choices: [
      { label: "Cancel", value: false, tone: "cancel" },
      { label: "Send", value: true, tone: "primary" },
    ],
    dismissed: false,
  });
}

/** The host's question before an Accept reaches the product, with what it costs. */
function confirmAccept(
  prompt: AcceptPrompt,
  costs: ChatCosts,
): Promise<boolean> {
  return ask<boolean>({
    title: "Accept chat request",
    from: prompt.product.id,
    fields: [
      { label: "From", value: prompt.from.name },
      { label: "Message", value: prompt.purpose },
    ],
    details: costDetails({ label: "Reply", value: costs.reply }, costs),
    choices: [
      { label: "Cancel", value: false, tone: "cancel" },
      { label: "Accept", value: true, tone: "primary" },
    ],
    dismissed: false,
  });
}
