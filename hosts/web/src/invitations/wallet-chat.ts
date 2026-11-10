import type {
  NotificationClaim,
  StatementAccountClaim,
} from "@parity/truapi-host/web";
import { bytesToHex, type HexString } from "@parity/truapi/scale";
import { ask } from "../app/prompt.js";
import type { PromptField } from "../app/reviews.js";
import type { ChatV2ProviderOptions } from "../chat/draft/provider.js";
import type { HandleNames } from "../contacts/contacts.js";
import type { NotificationStore } from "../notifications/notifications.js";
import type { NotificationActions } from "../notifications/screen.js";
import type { ChainDirectory } from "../platform/chain-directory.js";
import type { StatementStoreLike } from "../platform/statement-store.js";
import {
  parseWorkerManifest,
  type BundledWorkerAnswer,
  type ResolvedWorker,
} from "../products/worker-manifest.js";
import type { ReplySuite } from "./binding.js";
import { sessionChatKeys, type ChatKeys } from "./chat-keys.js";
import {
  startContactChatHost,
  type ContactChatHost,
} from "./contact-chat-host.js";
import { DirectoryContacts } from "./directory-contacts.js";
import type { AcceptPrompt, SendPrompt } from "./invitation-service.js";
import type { InvitationObserver } from "./observer.js";
import type { Product } from "./ports.js";
import { ownIdentityProblem } from "./published-keys.js";
import { WalletIdentityAuth } from "./wallet-auth.js";
import type { AllowanceGuard } from "../platform/allowance-usable.js";
import {
  type AllowanceSeen,
  type ChatCosts,
  type WalletDelivery,
} from "./wallet-delivery.js";

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
 * How requests travel is the {@link WalletDelivery} the page hands it: the
 * network's (`./wallet-delivery.ts`), or one the dev server's tools chose.
 */

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
   * Told where each of its accounts became usable, conversation accounts
   * and Nova Chat's notification accounts alike. Absent: nobody is.
   */
  allowanceSeen?: AllowanceSeen;
  /**
   * How it sets up chats, for a product with a setup of its own, such as
   * Nova Chat (`src/integrations/nova-chat/`). Absent: the host request
   * service (`host-requests/1`).
   */
  start?(context: ChatProductStart): ContactChatHost;
}

/**
 * What the host lends a chat product that sets up its own chats
 * ({@link ChatProductSpec.start}): the wallet's session and keys, the people
 * it knows, and the page's services.
 */
export interface ChatProductStart {
  session: WalletRequestsSession;
  spec: ChatProductSpec;
  contacts: DirectoryContacts;
  keys: ChatKeys;
  deps: WalletChatDeps;
}

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
  /**
   * Wait until a claimed account may post, by the network's posting rule
   * (`../platform/allowance-usable.ts`). Every post after a claim waits on
   * it: conversation accounts, and the notification accounts of host
   * requests and of Nova Chat.
   */
  usableAllowance: AllowanceGuard;
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
  path: string,
): Promise<ResolvedWorker> {
  const response = await fetch(`${base}${path}`, {
    cache: "no-store",
  });
  if (!response.ok)
    throw new Error(
      `This host does not serve Contact Chat: ${response.status} ${await response.text()}`,
    );
  const answer = (await response.json()) as BundledWorkerAnswer;
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
   * The chat products the wallet runs, the one New chat starts in first;
   * none runs for an empty list. The page passes Contact Chat alone
   * (`src/app/chat-products.ts`) unless the developer tools chose others.
   */
  products: readonly ChatProductSpec[];
  /** How the wallet's requests travel, and what that spends. */
  delivery: WalletDelivery;
}

/** Contact Chat for whichever wallet is signed in. */
export class WalletChat {
  /** Contact Chat as the page uses it, for the wallet signed in now. */
  readonly owner: ContactChatOwner;
  private current: { key: string; chats: RunningChat[] } | null = null;
  /** Aborts when the wallet's session ends, which stops its allowance waits. */
  private session = new AbortController();
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

  /** The chat products the wallet runs, the one New chat starts in first. */
  private products(): readonly ChatProductSpec[] {
    return this.deps.products;
  }

  /**
   * Whether the wallet's saved contacts stay out of the picker, and so out
   * of payments: whenever a chat product runs and its requests reach people
   * the network names.
   */
  hidesWalletContacts(): boolean {
    return this.products().length > 0 && this.deps.delivery.hidesWalletContacts;
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
    this.session.abort();
    this.session = new AbortController();
    this.current = null;
    this.sentChanged();
    if (session === null || key === null || this.products().length === 0)
      return;
    const chats = this.start(this.scoped(session, this.session.signal));
    if (chats.length === 0) return;
    this.current = { key, chats };
    const stops = chats.map((chat) =>
      chat.host.onSentChange(() => this.sentChanged()),
    );
    this.stopSent = () => stops.forEach((stop) => stop());
    for (const chat of chats)
      void chat.host.start().then(() => this.sentChanged());
  }

  /** `session` whose allowance waits stop once `signal` aborts. */
  private scoped(
    session: WalletRequestsSession,
    signal: AbortSignal,
  ): WalletRequestsSession {
    return {
      ...session,
      usableAllowance: (claim, request) =>
        session.usableAllowance(claim, {
          ...request,
          signal:
            request?.signal === undefined
              ? signal
              : AbortSignal.any([signal, request.signal]),
        }),
    };
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
  private start(session: WalletRequestsSession): RunningChat[] {
    const { deps } = this;
    const { delivery } = deps;
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
    const lists = (spec: ChatProductSpec, part: "sent" | "received") =>
      `truapi-web-signing-host:network:${session.network}:requests:${session.walletId}:${spec.lists}:${part}:v1${delivery.listSuffix}`;
    deps.log(
      `Host requests for ${this.products()
        .map((spec) => spec.product(session.networkSuffix).name)
        .join(" and ")}: ${delivery.line}`,
    );
    const self = bytesToHex(keys.chatPublic);
    return this.products().map((spec, index) => {
      const costs = delivery.costs(spec.messagesEncrypted);
      const accounts = delivery.allowance(session, spec.allowanceSeen ?? null);
      if (spec.start !== undefined)
        return {
          workerPath: spec.workerPath,
          ...(spec.workerParams === undefined
            ? {}
            : { workerParams: spec.workerParams }),
          startsChats: index === 0,
          host: spec.start({ session, spec, contacts, keys, deps: this.deps }),
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
          ...(accounts === undefined ? {} : { accounts }),
          delivery: delivery.hostDelivery(session, self),
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
}

const CANCEL = "Cancel it under Sent in your Inbox.";

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
    ...(prompt.signal === undefined ? {} : { signal: prompt.signal }),
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
    ...(prompt.signal === undefined ? {} : { signal: prompt.signal }),
  });
}
