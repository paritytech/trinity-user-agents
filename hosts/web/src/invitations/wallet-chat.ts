import { asProductId } from "@parity/truapi";
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
import { sessionChatKeys, type ChatKeys } from "./chat-keys.js";
import {
  startContactChatHost,
  type ContactChatHost,
} from "./contact-chat-host.js";
import { DirectoryContacts } from "./directory-contacts.js";
import { NovaBoundary } from "../chat/nova-chat/boundary.js";
import { deriveHardSecret } from "./chat-keys.js";
import { lookupChatIdentity } from "../platform/chain-directory.js";
import { startNovaChatHost } from "./nova-chat-host.js";
import * as sr25519 from "@scure/sr25519";
import { hexToBytes } from "@parity/truapi/scale";
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
  id: asProductId(`contact-chat.${networkSuffix}`),
  name: "Contact Chat",
});

/**
 * One chat product the host runs for a session: its product id, where its
 * worker archive is served, and what sets it apart. Each runs its own
 * request service, lists and delivery, and its worker its own storage and
 * accounts, which the core keys by the product id.
 */
export interface ChatProductSpec {
  product(networkSuffix: string): Product;
  /** Where the host serves its worker's description, under its base. */
  workerPath: string;
  /** Names its request lists in the host's storage. */
  lists: string;
  /** Whether it encrypts its messages end to end, for the prompts' cost lines. */
  messagesEncrypted: boolean;
  /** Query parameters of its worker's page: the settings the host passes it. */
  workerParams?: Readonly<Record<string, string>>;
  /**
   * Resolves once a conversation account's claim holds at the People chain's
   * finalized head, or rejects saying why not. Absent: the claim's answer is
   * enough.
   */
  allowanceSettled?(claim: StatementAccountClaim): Promise<void>;
  /**
   * How it sets up chats. Absent: the host request service
   * (`host-requests/1`). `nova`: the official Nova apps' own chat requests,
   * through the host's Nova key boundary (`./nova-chat-host.ts`).
   */
  setup?: "nova";
}

/** Contact Chat, the chat product every session runs. */
export const CONTACT_CHAT: ChatProductSpec = {
  product: (networkSuffix) => contactChat(networkSuffix),
  workerPath: CONTACT_CHAT_PATH,
  lists: "contact-chat",
  messagesEncrypted: false,
};

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

/** The host's chat products (Contact Chat, and any other a developer added) as the rest of the page uses them. */
export interface ContactChatOwner {
  /**
   * The people the host may name by handle beyond the wallet's contacts, by
   * account: whoever the user's requests are with. Naming them does not
   * resolve them for a transaction.
   */
  people(): readonly HexString[];
  /** What the Inbox can do with the chat products' requests, each by the product that owns it. */
  notificationActions: NotificationActions;
  /** The workers this adds to the session: each chat product, while it runs. */
  workerNames(): string[];
  /** Whether `name` is one of these workers, which the host serves itself. */
  resolvesWorker(name: string): boolean;
  /** The worker `name`, as the host describes the archive it serves. */
  resolveWorker(name: string): Promise<ResolvedWorker>;
  /** Whether `productId` may give its rooms contacts: one of these products. */
  isContactChat(productId: string): boolean;
  /** What the host lends that product's chat provider, or null for another product. */
  chatOptions(productId: string): ChatV2ProviderOptions | null;
  /** The request service, for each of these products' worker frame only, its own. */
  requests: WorkerPortService;
}

/** Contact Chat's worker, as the host describes the archive it serves under `base`. */
export async function resolveContactChatWorker(
  base: string,
  path: string = CONTACT_CHAT_PATH,
): Promise<ResolvedWorker> {
  const response = await fetch(`${base}${path}`, {
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

/** A running chat product: its request service and where its worker is served. */
export interface RunningChat {
  host: ContactChatHost;
  workerPath: string;
  /** Query parameters of its worker's page. */
  workerParams?: Readonly<Record<string, string>>;
  /** Whether New chat starts chats in it. Each other one keeps its chats, but starts none. */
  startsChats: boolean;
}

/** `options` without a start: the provider shows its chats and starts none. */
function withoutStart(options: ChatV2ProviderOptions): ChatV2ProviderOptions {
  const kept = { ...options };
  delete kept.start;
  return kept;
}

/** The {@link ContactChatOwner} of the running chat products, each answering for its own product id. */
export function contactChatOwner(options: {
  base: string;
  chats: () => readonly RunningChat[];
  /** Calls back whenever the sent requests may have changed, also when the chats themselves change. */
  onSentChange(listener: () => void): () => void;
}): ContactChatOwner {
  const byProduct = (productId: string) =>
    options.chats().find((chat) => chat.host.product.id === productId);
  const owning = (id: string) =>
    options.chats().find((chat) => chat.host.owns(id))?.host;
  return {
    people: () =>
      [
        ...new Set(options.chats().flatMap((chat) => chat.host.people())),
      ] as HexString[],
    notificationActions: {
      canAccept: (item) => owning(item.id) !== undefined,
      accept: (item) => {
        const host = owning(item.id);
        return host === undefined
          ? Promise.reject(new Error("No chat product holds this request."))
          : host.accept(item);
      },
      destination: (item) => (owning(item.id) !== undefined ? "Chat" : null),
      open: (item) => {
        if (item.state.tag === "accepted") owning(item.id)?.open(item);
      },
      sent: () =>
        options
          .chats()
          .flatMap((chat) => chat.host.sent())
          .sort((left, right) => right.createdAt - left.createdAt),
      cancel: (id) => {
        for (const chat of options.chats()) if (chat.host.cancel(id)) return;
      },
      onSentChange: options.onSentChange,
    },
    workerNames: () => options.chats().map((chat) => chat.host.product.id),
    resolvesWorker: (name) => byProduct(name) !== undefined,
    resolveWorker: (name) => {
      const chat = byProduct(name);
      return chat === undefined
        ? Promise.reject(
            new Error(`${name} is not a chat product of this host.`),
          )
        : resolveContactChatWorker(options.base, chat.workerPath).then(
            (worker) =>
              chat.workerParams === undefined
                ? worker
                : { ...worker, startParams: chat.workerParams },
          );
    },
    isContactChat: (productId) => byProduct(productId) !== undefined,
    chatOptions: (productId) => {
      const chat = byProduct(productId);
      if (chat === undefined) return null;
      return chat.startsChats
        ? chat.host.chatOptions
        : withoutStart(chat.host.chatOptions);
    },
    requests: {
      connect: (productId, port) => {
        const chat = byProduct(productId);
        if (chat !== undefined) chat.host.attachChat(port);
        else port.close();
      },
      disconnect: (productId) => byProduct(productId)?.host.detachChat(),
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
   * The chat products a developer runs for the wallet on the dev server, the
   * one New chat starts in first; none runs for an empty list. Absent:
   * Contact Chat alone.
   */
  products?: readonly ChatProductSpec[];
  /**
   * Another delivery a developer chose on the dev server, with the simulated
   * delivery between tabs the local choices use. Absent: the network.
   */
  devDelivery?: {
    mode(): WalletDelivery;
    local(self: string): HostDelivery;
  };
}

/** Contact Chat for whichever wallet is signed in. */
export class WalletChat {
  /** Contact Chat as the page uses it, for the wallet signed in now. */
  readonly owner: ContactChatOwner;
  private current: { key: string; chats: RunningChat[] } | null = null;
  private readonly sentListeners = new Set<() => void>();
  private stopSent = () => {};

  constructor(private readonly deps: WalletChatDeps) {
    this.owner = contactChatOwner({
      base: deps.base,
      chats: () => this.current?.chats ?? [],
      onSentChange: (listener) => {
        this.sentListeners.add(listener);
        return () => this.sentListeners.delete(listener);
      },
    });
  }

  /** How the wallet's requests travel in this page. */
  mode(): WalletDelivery {
    return this.deps.devDelivery?.mode() ?? "network";
  }

  /** The chat products the wallet runs, the one New chat starts in first. */
  private products(): readonly ChatProductSpec[] {
    return this.deps.products ?? [CONTACT_CHAT];
  }

  /**
   * Whether the wallet's saved contacts stay out of the picker, and so out
   * of payments: whenever a chat product runs and its requests reach people
   * the network names.
   */
  hidesWalletContacts(): boolean {
    return this.products().length > 0 && this.mode() !== "local";
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
    for (const chat of this.current?.chats ?? []) chat.host.stop();
    this.current = null;
    this.sentChanged();
    if (session === null || key === null || this.products().length === 0)
      return;
    const chats = this.start(session, this.mode());
    if (chats.length === 0) return;
    this.current = { key, chats };
    const stops = chats.map((chat) =>
      chat.host.onSentChange(() => this.sentChanged()),
    );
    this.stopSent = () => stops.forEach((stop) => stop());
    for (const chat of chats)
      void chat.host.start().then(() => this.sentChanged());
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
  ): RunningChat[] {
    const { deps } = this;
    const keys = sessionChatKeys({
      entropy: session.entropy,
      networkSuffix: session.networkSuffix,
      identityAccountId: session.identityAccountId as HexString | undefined,
      chatPublicKey: session.chatPublicKey as HexString | undefined,
    });
    if (typeof keys === "string") {
      deps.log(
        `The request service does not run for this wallet, so no chat product does: ${keys}`,
      );
      return [];
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
    const list = (spec: ChatProductSpec, part: "sent" | "received") =>
      `truapi-web-signing-host:network:${session.network}:requests:${session.walletId}:${spec.lists}:${part}:v1`;
    // The network delivery keeps its own lists: its requests name the day
    // inbox as their return route, which the local delivery cannot reach.
    const local = mode === "local" || mode === "local-validate";
    const lists = (spec: ChatProductSpec, part: "sent" | "received") =>
      local ? list(spec, part) : `${list(spec, part)}:day-inbox`;
    deps.log(
      `Host requests for ${this.products()
        .map((spec) => spec.product(session.networkSuffix).name)
        .join(" and ")}: ${DELIVERY_LINE[mode]}`,
    );
    const self = bytesToHex(keys.chatPublic);
    return this.products().map((spec, index) => {
      const costs = chatCosts(mode, spec.messagesEncrypted);
      if (spec.setup === "nova")
        return {
          workerPath: spec.workerPath,
          startsChats: index === 0,
          host: this.startNova(session, spec, contacts, mode, keys),
        };
      return {
        workerPath: spec.workerPath,
        ...(spec.workerParams === undefined
          ? {}
          : { workerParams: spec.workerParams }),
        startsChats: index === 0,
        host: startContactChatHost({
          product: spec.product(session.networkSuffix),
          keys: {
            sent: lists(spec, "sent"),
            received: lists(spec, "received"),
          },
          auth: new WalletIdentityAuth(keys),
          ...(deps.replySuite === undefined
            ? {}
            : { replySuite: deps.replySuite }),
          ...(mode === "local"
            ? {}
            : {
                accounts: conversationAllowance(
                  session,
                  mode === "network",
                  spec.allowanceSettled,
                ),
              }),
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
                    const timer = setTimeout(
                      task,
                      Math.min(delayMs, 2 ** 31 - 1),
                    );
                    return () => clearTimeout(timer);
                  },
                  log: deps.log,
                  ...(deps.observe === undefined
                    ? {}
                    : { observe: deps.observe }),
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
        }),
      };
    });
  }

  /**
   * Nova Chat for `session`: the official Nova apps' chat requests, sealed
   * and opened by the wallet's keys in the host, read and posted on the
   * network's Statement Store. It needs the network: the local deliveries
   * carry host-requests only, so it does not run there.
   */
  private startNova(
    session: WalletRequestsSession,
    spec: ChatProductSpec,
    contacts: DirectoryContacts,
    mode: WalletDelivery,
    keys: ChatKeys,
  ): ContactChatHost {
    const { deps } = this;
    const product = spec.product(session.networkSuffix);
    const submit = mode === "network";
    const chatKeyOf = async (account: Uint8Array) =>
      (await lookupChatIdentity(session.directory, account))?.chatKey ?? null;
    return startNovaChatHost({
      product,
      storageKey: `truapi-web-signing-host:network:${session.network}:nova-chat:${session.walletId}:v1`,
      boundary: NovaBoundary.fromEntropy(
        session.entropy,
        session.networkSuffix,
        chatKeyOf,
      ),
      handles: deps.handles,
      nameOf: (account) => contacts.nameOf(account),
      lookupName: async (account) =>
        (
          await lookupChatIdentity(
            session.directory,
            hexToBytes(account as HexString),
          ).catch(() => null)
        )?.name ?? null,
      onNamesChange: (listener) => deps.names.onChange(listener),
      blocked: deps.blocked,
      notifications: deps.notifications,
      verify: () =>
        mode === "local" || mode === "local-validate"
          ? Promise.resolve({
              reason:
                "Nova Chat needs the network delivery: the Nova apps read only the network's Statement Store.",
              again: false,
            })
          : ownIdentityProblem(session.directory, keys),
      openStore: () => session.openStatementStore(),
      notificationSigner: (kind, requestId, period) => {
        const secret = deriveHardSecret(session.entropy, [
          kind,
          requestId,
          String(period),
        ]);
        return {
          public: sr25519.getPublicKey(secret),
          sign: (payload) => sr25519.sign(secret, payload),
        };
      },
      claimNotification: async (account, period) => {
        const claim = await session.claimNotificationAccount(account, submit);
        if (claim.period !== period)
          throw new Error("the day changed while the account was claimed");
        if (!submit)
          throw new Error(
            `claims are only validated in this tab: today's claim would take slot ${claim.seq}.`,
          );
        return {
          line: `notification account ${claim.outcome} for day ${claim.period}, slot ${claim.seq}.`,
          seq: claim.seq,
          ...(claim.collection === undefined
            ? {}
            : { collection: claim.collection }),
          ...(claim.blockHash === undefined
            ? {}
            : { blockHash: claim.blockHash }),
        };
      },
      ...(spec.allowanceSettled === undefined
        ? {}
        : {
            // The same finality guard as the conversation accounts': a
            // notification account's allowance is read the same way.
            notificationSettled: (account, claim) =>
              spec.allowanceSettled!({
                account: bytesToHex(account),
                period: claim.period,
                outcome: "registered",
                seq: claim.seq,
                collection: claim.collection ?? "People",
                ...(claim.blockHash === undefined
                  ? {}
                  : { blockHash: claim.blockHash }),
              }),
          }),
      accounts: conversationAllowance(session, submit, spec.allowanceSettled),
      confirmSend: (prompt) => confirmSend(prompt, NOVA_COSTS),
      confirmAccept: (prompt) => confirmAccept(prompt, NOVA_COSTS),
      showChat: deps.showChat,
      onSenders: deps.namedChanged,
      log: deps.log,
    });
  }
}

/** How the request service's requests and replies travel, whichever product sent them. */
const DELIVERY_LINE: Record<WalletDelivery, string> = {
  network:
    "requests and replies go on the network's day inbox, and each post claims a notification slot.",
  validate:
    "requests and replies use the network delivery with claims only validated, and nothing is submitted.",
  local: "requests and replies stay between tabs of this browser.",
  "local-validate":
    "requests and replies stay in this browser, conversation accounts' claims are only validated, and nothing is submitted.",
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
  settled?: (claim: StatementAccountClaim) => Promise<void>,
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
      await settled?.(claim);
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

/** What Nova Chat spends and exposes, for its prompts. Each request and acceptance is posted once. */
export const NOVA_COSTS: ChatCosts = {
  summary: "Uses daily slots · Messages are end-to-end encrypted",
  request: "1 notification slot, once. It is not posted again on a later day.",
  reply: "1 notification slot, once, for the acceptance.",
  chat: CHAT_ACCOUNT_COST,
  messages:
    "End-to-end encrypted, as the Nova apps do. Who posts, to which topics, when and how much is public.",
};

/** What `mode` spends and exposes, or null when it spends nothing. */
export function chatCosts(
  mode: WalletDelivery,
  encrypted = false,
): ChatCosts | null {
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
        summary: encrypted
          ? "Uses daily slots · Messages are end-to-end encrypted"
          : "Uses daily slots · Messages are public",
        request:
          "1 notification slot now, and 1 more each day it is pending while this host is open.",
        reply:
          "1 notification slot now, and 1 more on any later day their request arrives again.",
        chat: CHAT_ACCOUNT_COST,
        messages: encrypted
          ? "End-to-end encrypted. Who posts, to which topics, when and how much is public."
          : "Not encrypted: anyone can read them.",
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
export function confirmAccept(
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
