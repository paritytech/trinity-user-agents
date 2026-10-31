import type {
  NotificationClaim,
  WorkerPairingHostRuntime,
} from "@parity/truapi-host/web";
import { bytesToHex, type HexString } from "@parity/truapi/scale";
import {
  CONTACT_CHAT_PATH,
  EXAMPLE_CHAT_PATH,
  type DemoWorkerAnswer,
} from "./local-demo-route.js";
import type { ChatContact } from "../../src/chat/provider.js";
import type { ChatV2ProviderOptions } from "../../src/chat/draft/provider.js";
import {
  parseWorkerManifest,
  type ResolvedWorker,
} from "../../src/products/worker-manifest.js";
import type { FrameTap } from "../../src/products/tap.js";
import {
  InvitationService,
  productPort,
  type SendPrompt,
} from "../../src/invitations/invitation-service.js";
import type { Product } from "../../src/invitations/ports.js";
import { cryptoRandom } from "../../src/invitations/sealing.js";
import type { NotificationStore } from "../../src/notifications/notifications.js";
import type { NotificationActions } from "../../src/notifications/screen.js";
import { openProduct, type OpenProduct } from "../../src/products/product.js";
import { ask } from "../../src/app/prompt.js";
import { loaderUrl, mountScope } from "../../src/products/loader/mount.js";
import {
  fixtureDirectory,
  type UsernameDirectory,
} from "../../src/contacts/username-directory.js";
import type { HandleNames } from "../../src/contacts/contacts.js";
import { FixtureContacts } from "../../fixtures/invitations/fixture-contacts.js";
import type { ChainDirectory } from "../../src/chat/nova/directory.js";
import type { StatementStoreLike } from "../../src/chat/nova/engine.js";
import type { Network } from "../../src/platform/network.js";
import { noSubmitNetwork, type NoSubmitRecord } from "./no-submit.js";
import { StatementStoreDelivery } from "../../src/invitations/statement-delivery.js";
import { DirectoryContacts } from "../../src/invitations/directory-contacts.js";
import { sessionChatKeys } from "../../src/chat/nova-session.js";
import { WalletIdentityAuth } from "../../src/invitations/wallet-auth.js";
import {
  ownIdentityProblem,
  type IdentityProblem,
} from "../../src/invitations/published-keys.js";
import { LocalDelivery } from "../../fixtures/invitations/local-delivery.js";
import {
  sentRequests,
  startContactChatHost,
  type ContactChatHost,
} from "./contact-chat-host.js";
import { pageStore } from "../../fixtures/invitations/shared-store.js";
import * as samples from "../../fixtures/notifications/sample-notifications.js";
import {
  PART_VERSION,
  TEST_USERS,
  chatContactsOf,
  setTabTestUser,
  tabTestUser,
  testUserAccount,
  testUserDirectory,
  testUserEntropy,
  testUserKey,
  testUserKeys,
  testUserScope,
  type TestUser,
} from "../../fixtures/invitations/test-users.js";

/**
 * DEV ONLY. The test accounts the Account tab's picker offers beside saved
 * wallets, the invitation demo, and the More tab's Developer section. Only a
 * dev server on this machine loads this module.
 *
 * A test account is a wallet built into the dev server from a public test
 * phrase. It signs in and runs as any imported wallet does; anyone can sign
 * as it, and the host registers and funds nothing for it.
 *
 * The invitation demo is apart from them, chosen in More: a tab acts as a
 * made-up user (Alice, Bob or Carol) whose contacts, identity credentials
 * are fixtures, and invitations between tabs go through the local simulated
 * delivery. The core signs the demo user in with its public test phrase under
 * a storage scope of its own, so products get a real session, real handles
 * from `contacts.pick` and real prompts. The host names those handles through
 * the handles the core mints for the people it knows; the other users'
 * identity credentials stay fixtures. None of the wallet
 * session's services start: no Nova chat, Pocket or wallet workers, so
 * fixture contacts never reach a wallet's chat or a transaction it signs.
 *
 * The demo has two products, each with its own request service:
 *
 * - Example Chat, an app, opens only when asked: from More, or to accept an
 *   invitation. Its Close ends it.
 * - Contact Chat (`fixtures/products/contact-chat/`), a chat worker, runs
 *   for the test user as an ordinary product worker. The host's Chat shows
 *   its conversations and names each person from the contact handle the
 *   product holds, through the host's handle index; the product never gets a
 *   name. A chat ends at the connection: Contact Chat sends no text.
 */

export interface DevToolsDeps {
  network: { id: string; networkSuffix: string };
  /** The host page's base path. */
  base: string;
  notifications: NotificationStore;
  /** Where the Developer section is drawn in the menu. */
  section: HTMLElement;
  /** The app bar marker that names the test user. */
  marker: HTMLElement;
  /** Where Example Chat's frame goes, over the product stage. */
  stage: HTMLElement;
  /** End this tab's wallet session and product, before acting as a test user. */
  leaveWalletSession(): void;
  /** Keep the core's storage under `scope`, apart from every wallet's. */
  useStorage(scope: string): void;
  /** What the inspector sees of `productId`'s frames. */
  tap(productId: string): FrameTap | undefined;
  /** Bring the test account's product, in Browse, to the front, and draw the stage again. */
  showProduct(): void;
  /** The host's names for the contact handles the core mints. */
  handles: HandleNames;
  /** What the core reports of the signed-in session's keys, once it is signed in. */
  coreSession(): Promise<{
    identityAccountId?: string;
    chatPublicKey?: string;
  }>;
  /** Bring Chat to the front, at Contact Chat's request. */
  showChat(): void;
  /** The accounts the signed-in wallet blocked. */
  blocked(): readonly string[];
  /** The host's names for people, by account, read from its records and the chain. */
  names: {
    name(account: string): string | null;
    onChange(listener: () => void): () => void;
  };
  /** Tell the host's handle index that the people it may name changed. */
  namedChanged(): void;
  log(line: string): void;
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
}

/** What the host lends a worker frame of one product over one port. */
export interface WorkerPortService {
  connect(productId: string, port: MessagePort): void;
  disconnect(productId: string): void;
}

export interface DevSession {
  /** The invitation demo user this tab acts as, or null for the normal wallet session. */
  testUser: TestUser | null;
  /**
   * Start the invitation demo as `id` in this tab, ending its wallet
   * session, or leave it with null, back to the tab's wallet if one is set.
   * Either reloads the tab.
   */
  useDemo(id: string | null): void;
  /** The Notifications scope of the test user, or null. */
  notificationsScope: string | null;
  /** The host storage scope of the test user's session, its workers' too, or null. */
  scope: string | null;
  /** The test user's contacts, for the host's contact picker, or null. */
  contacts: ChatContact[] | null;
  /**
   * The people the host may name by handle beyond its contacts, by account:
   * everyone a test user's fixture directory knows, or whoever sent the
   * signed-in wallet a request. Naming them does not resolve them for a
   * transaction.
   */
  people(): readonly HexString[];
  /**
   * Follow the signed-in wallet: Contact Chat runs for it, with requests
   * over its own keys. Null ends it. Call it before the session's workers
   * start, so they see Contact Chat among them.
   */
  setWallet(session: WalletRequestsSession | null): void;
  /**
   * Whether this tab holds the wallet's Nova chat off: true while the
   * wallet's Contact Chat requests use the network delivery, so New chat
   * starts Contact Chat, and Nova neither starts nor answers anything. Fixed
   * for the page: changing the delivery reloads it.
   */
  holdsNova(): boolean;
  /**
   * `network`, with every submission refused while the wallet's Contact Chat
   * only validates claims: no extrinsic and no statement leaves this tab.
   * What it refused, and every method it let through, are kept on
   * `window.__truapiNoSubmit` for a test to read.
   */
  guardNetwork(network: Network): Network;
  /** Where the test user's picker finds people by username: the other test users, or null. */
  usernames: UsernameDirectory | null;
  /** What the Notifications screen can do with the test user's invitations, or null. */
  notificationActions: NotificationActions | null;
  /** Once the core runs, sign the test user in and start receiving invitations. */
  start(runtime: WorkerPairingHostRuntime): Promise<void>;
  /** Whether Example Chat is open, so the stage shows it. */
  exampleChatOpen(): boolean;
  /** Close Example Chat: its frame ends and it stops taking invitations until it opens again. */
  closeExampleChat(): void;
  showProduct(visible: boolean): void;
  /** The workers this module adds: Contact Chat, for a test user or a wallet. */
  workerNames(): string[];
  /** Whether `name` is a worker this module resolves. */
  resolvesWorker(name: string): boolean;
  /** Contact Chat's worker, as the dev server describes the archive it built. */
  resolveWorker(): Promise<ResolvedWorker>;
  /** Whether `productId` may give its rooms contacts: Contact Chat. */
  isContactChat(productId: string): boolean;
  /** What the host lends Contact Chat's chat provider, or null for another product. */
  chatOptions(productId: string): ChatV2ProviderOptions | null;
  /** The request service, for Contact Chat's worker frame only. */
  requests: WorkerPortService;
}

/** Example Chat's product id and name on a network. */
export const exampleChat = (networkSuffix: string): Product => ({
  id: `example-chat.${networkSuffix}`,
  name: "Example Chat",
});

/** Contact Chat's product id and name on a network. */
export const contactChat = (networkSuffix: string): Product => ({
  id: `contact-chat.${networkSuffix}`,
  name: "Contact Chat",
});

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className = "",
  text?: string,
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className !== "") node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function button(label: string, action: string, onClick: () => void) {
  const node = el("button", "", label);
  node.type = "button";
  node.dataset.action = action;
  node.addEventListener("click", onClick);
  return node;
}

/** The host's question before a product's invitation leaves. */
function confirmSend(prompt: SendPrompt): Promise<boolean> {
  return ask<boolean>({
    title: "Send invitation",
    from: prompt.product.id,
    fields: [
      { label: "To", value: prompt.to.name },
      { label: "Message", value: prompt.purpose },
      {
        label: "Waits",
        value: "Until they accept, or you cancel it in your Inbox",
      },
    ],
    choices: [
      { label: "Cancel", value: false, tone: "cancel" },
      { label: "Send", value: true, tone: "primary" },
    ],
    dismissed: false,
  });
}

declare global {
  interface Window {
    /** What the no-submit guard of a validate-only tab saw, for a test to read. */
    __truapiNoSubmit?: NoSubmitRecord;
  }
}

/** The host's question before the wallet's requests go on the network. */
function confirmNetworkDelivery(network: string): Promise<boolean> {
  return ask<boolean>({
    title: "Post requests on the network",
    from: "Developer tools",
    fields: [
      { label: "Network", value: network },
      {
        label: "Each post",
        value:
          "Claims one of your daily notification slots with your personhood, then posts one signed statement to the shared day inbox",
      },
      {
        label: "Runs",
        value:
          "While this tab is open: pending requests are posted again each day",
      },
    ],
    choices: [
      { label: "Cancel", value: false, tone: "cancel" },
      { label: "Use the network", value: true, tone: "primary" },
    ],
    dismissed: false,
  });
}

/** How long Accept waits for Example Chat to connect after opening it. */
const CONNECT_TIMEOUT_MS = 15_000;

/** Where a tab keeps whether Contact Chat runs for its signed-in wallet. */
const WALLET_CHAT_KEY = "truapi-web-signing-host:dev:wallet-contact-chat";

/**
 * Where a tab keeps how the wallet's Contact Chat requests travel: absent for
 * the local simulated delivery, `validate` for the network delivery with
 * every claim only validated, `network` to claim and post for real.
 */
const WALLET_DELIVERY_KEY = "truapi-web-signing-host:dev:wallet-delivery";
type WalletDelivery = "local" | "validate" | "network";

function walletDelivery(): WalletDelivery {
  const value = sessionStorage.getItem(WALLET_DELIVERY_KEY);
  return value === "validate" || value === "network" ? value : "local";
}

export function startDevTools(deps: DevToolsDeps): DevSession {
  const testUser = tabTestUser(sessionStorage);
  const networkMode = (): WalletDelivery =>
    testUser === null && sessionStorage.getItem(WALLET_CHAT_KEY) === "on"
      ? walletDelivery()
      : "local";
  const session = testUser === null ? null : startTestUser(deps, testUser);
  let wallet: { key: string; chat: ContactChatHost } | null = null;
  /** Contact Chat's host side now: the test user's, or the wallet's. */
  const chat = (): ContactChatHost | null =>
    session?.chat ?? wallet?.chat ?? null;
  const isChat = (productId: string) => chat()?.product.id === productId;
  /** Who draws the wallet's sent requests; the wallet's Contact Chat changes with the session. */
  const sentListeners = new Set<() => void>();
  let stopWalletSent = () => {};
  const walletSentChanged = () => {
    for (const listener of [...sentListeners]) listener();
  };
  const useDemo = (id: string | null) => {
    if (id !== null) deps.leaveWalletSession();
    setTabTestUser(sessionStorage, id);
    window.location.reload();
  };
  drawSection(deps, testUser, session, useDemo);
  deps.marker.hidden = testUser === null;
  if (testUser !== null) {
    deps.marker.textContent = `Invitation demo · ${testUser.name}`;
    deps.marker.title =
      "A made-up user signed in with a public test phrase. Invitations go only to other tabs of this browser.";
  }
  return {
    testUser,
    useDemo,
    notificationsScope:
      testUser === null
        ? null
        : testUserKey(deps.network.id, testUser.id, "notifications"),
    scope: testUser === null ? null : testUserScope(testUser),
    contacts:
      testUser === null
        ? null
        : chatContactsOf(testUser, deps.network.networkSuffix),
    people: () =>
      testUser !== null
        ? TEST_USERS.filter((user) => user.id !== testUser.id).map((user) =>
            testUserAccount(user, deps.network.networkSuffix),
          )
        : ((wallet?.chat.people() ?? []) as HexString[]),
    holdsNova: () => networkMode() !== "local",
    guardNetwork: (network) => {
      if (networkMode() !== "validate") return network;
      deps.log(
        "This tab submits nothing: every extrinsic and statement submission is refused before it reaches the network.",
      );
      const guarded = noSubmitNetwork(network, deps.log);
      window.__truapiNoSubmit = guarded.record;
      return guarded.network;
    },
    setWallet: (next) => {
      if (testUser !== null || sessionStorage.getItem(WALLET_CHAT_KEY) !== "on")
        return;
      const key =
        next === null
          ? null
          : `${next.network}:${next.walletId}:${next.identityAccountId ?? ""}:${next.chatPublicKey ?? ""}`;
      if (key === (wallet?.key ?? null)) return;
      stopWalletSent();
      stopWalletSent = () => {};
      wallet?.chat.stop();
      wallet = null;
      walletSentChanged();
      if (next === null || key === null) return;
      const started = startWalletChat(deps, next);
      if (started === null) return;
      wallet = { key, chat: started };
      stopWalletSent = started.onSentChange(walletSentChanged);
      void started.start().then(walletSentChanged);
    },
    usernames:
      testUser === null
        ? null
        : fixtureDirectory(
            "the test users",
            TEST_USERS.filter((user) => user.id !== testUser.id).map(
              (user) => ({
                account: testUserAccount(user, deps.network.networkSuffix),
                username: user.id,
              }),
            ),
          ),
    notificationActions: session?.notificationActions ?? {
      canAccept: (item) => wallet?.chat.owns(item.id) ?? false,
      accept: (item) =>
        wallet === null
          ? Promise.reject(new Error("No wallet is signed in."))
          : wallet.chat.accept(item),
      destination: (item) =>
        wallet?.chat.owns(item.id) === true ? "Chat" : null,
      open: (item) => wallet?.chat.open(item),
      sent: () => wallet?.chat.sent() ?? [],
      cancel: (id) => void wallet?.chat.cancel(id),
      onSentChange: (listener) => {
        sentListeners.add(listener);
        return () => sentListeners.delete(listener);
      },
    },
    start: (runtime) => session?.start(runtime) ?? Promise.resolve(),
    exampleChatOpen: () => session?.isOpen() ?? false,
    closeExampleChat: () => session?.close(),
    showProduct: (visible) => session?.show(visible),
    workerNames: () => {
      const current = chat();
      return current === null ? [] : [current.product.id];
    },
    resolvesWorker: isChat,
    resolveWorker: async () => {
      const response = await fetch(`${deps.base}${CONTACT_CHAT_PATH}`, {
        cache: "no-store",
      });
      if (!response.ok)
        throw new Error(
          `The dev server did not build Contact Chat: ${await response.text()}`,
        );
      const answer = (await response.json()) as DemoWorkerAnswer;
      return {
        record: answer.record,
        cid: answer.cid,
        kind: answer.kind,
        manifest: parseWorkerManifest(answer.manifest),
        gateway: window.location.origin,
      };
    },
    isContactChat: isChat,
    chatOptions: (productId) =>
      isChat(productId) ? (chat()?.chatOptions ?? null) : null,
    requests: {
      connect: (productId, port) =>
        isChat(productId) ? chat()!.attachChat(port) : port.close(),
      disconnect: (productId) => {
        if (isChat(productId)) chat()!.detachChat();
      },
    },
  };
}

/**
 * Contact Chat for the signed-in wallet, over the keys its entropy derives,
 * once they are the keys the core reports for the session; null, with the
 * reason logged, when they are not. People are any identity with a chat key
 * the chain publishes, but the blocked: whoever the user picks to write to,
 * and whoever writes to them. Names come from the host's own records and the
 * chain, never from a request.
 */
function startWalletChat(
  deps: DevToolsDeps,
  session: WalletRequestsSession,
): ContactChatHost | null {
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
  const mode = walletDelivery();
  // The network delivery keeps its own lists: its requests name the day
  // inbox as their return route, which the local delivery cannot reach.
  const lists = (part: "sent" | "received") =>
    mode === "local" ? list(part) : `${list(part)}:day-inbox`;
  if (mode !== "local")
    deps.log(
      mode === "network"
        ? "Contact Chat's requests go on the network's day inbox: each post claims a notification slot."
        : "Contact Chat's requests use the network delivery with claims only validated: nothing is submitted.",
    );
  return startContactChatHost({
    product: contactChat(session.networkSuffix),
    keys: { sent: lists("sent"), received: lists("received") },
    network: session.network,
    self: bytesToHex(keys.chatPublic),
    auth: new WalletIdentityAuth(keys),
    ...(mode === "local"
      ? {}
      : {
          delivery: new StatementStoreDelivery({
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
          }),
        }),
    contacts,
    handles: deps.handles,
    onNamesChange: (listener) => deps.names.onChange(listener),
    notifications: deps.notifications,
    verify: () => ownIdentityProblem(session.directory, keys),
    confirmSend,
    showChat: deps.showChat,
    onSenders: deps.namedChanged,
    log: deps.log,
  });
}

interface TestUserSession {
  /** Contact Chat's host side, which runs as the test user's worker. */
  chat: ContactChatHost;
  notificationActions: NotificationActions;
  start(runtime: WorkerPairingHostRuntime): Promise<void>;
  show(visible: boolean): void;
  isOpen(): boolean;
  /** Open Example Chat, or open it again in a new frame, and bring it to the front. */
  open(): Promise<void>;
  /** End Example Chat's frame, and fail an Accept that waits for it. */
  close(): void;
  /** Calls back when Example Chat opens or closes. */
  onChange(listener: () => void): void;
  /** Stop, and remove this test user's lists on this network. */
  reset(): void;
}

function startTestUser(deps: DevToolsDeps, self: TestUser): TestUserSession {
  const product = exampleChat(deps.network.networkSuffix);
  const keys = {
    notifications: testUserKey(deps.network.id, self.id, "notifications"),
    sent: testUserKey(deps.network.id, self.id, "outbox"),
    received: testUserKey(deps.network.id, self.id, "received"),
  };
  const now = () => Date.now();
  const schedule = (task: () => void, delayMs: number) => {
    const timer = setTimeout(task, Math.min(delayMs, 2 ** 31 - 1));
    return () => clearTimeout(timer);
  };
  const suffix = deps.network.networkSuffix;
  // The wallet keys of the test user's phrase. Nothing is signed or opened
  // with them until `start` checked them against the core's session and the
  // published chat key.
  const walletKeys = testUserKeys(self, suffix);
  const auth = new WalletIdentityAuth(walletKeys);
  const directory = testUserDirectory(suffix);
  let verified = false;
  deps.notifications.setScope(keys.notifications);
  const contacts = new FixtureContacts(self, deps.handles, {
    networkSuffix: suffix,
    directory,
    log: deps.log,
  });
  const delivery = () =>
    new LocalDelivery({
      self: auth.identityFor().agreementPublic,
      network: deps.network.id,
      store: pageStore(),
      now,
      schedule,
      random: cryptoRandom,
      log: deps.log,
    });
  const service = new InvitationService({
    product,
    auth,
    contacts,
    notifications: deps.notifications,
    storage: localStorage,
    keys,
    delivery: delivery(),
    now,
    schedule,
    random: cryptoRandom,
    confirmSend,
    showProduct: () => deps.showProduct(),
    log: deps.log,
  });

  /**
   * Why the wallet keys may not prove this user, or null: they must be the
   * keys the core's session holds, and the ones published for its identity
   * account.
   */
  const walletProblem = async (): Promise<IdentityProblem | null> => {
    const core = await deps.coreSession();
    const checked = sessionChatKeys({
      entropy: testUserEntropy(self),
      networkSuffix: suffix,
      identityAccountId: core.identityAccountId as HexString | undefined,
      chatPublicKey: core.chatPublicKey as HexString | undefined,
    });
    if (typeof checked === "string") return { reason: checked, again: false };
    return ownIdentityProblem(directory, walletKeys);
  };
  let verifiedOnce: Promise<IdentityProblem | null> = new Promise(() => {});
  // Contact Chat: its own request service, over the same wallet keys,
  // contacts and delivery, and its own lists.
  const chat = startContactChatHost({
    product: contactChat(suffix),
    keys: {
      sent: testUserKey(deps.network.id, self.id, "contact-chat-outbox"),
      received: testUserKey(deps.network.id, self.id, "contact-chat-received"),
    },
    network: deps.network.id,
    self: auth.identityFor().agreementPublic,
    auth,
    contacts,
    handles: deps.handles,
    notifications: deps.notifications,
    // Checked once, for both products, in `start`.
    verify: () => verifiedOnce,
    confirmSend,
    showChat: deps.showChat,
    log: deps.log,
  });
  const host = el("section", "test-product");
  host.setAttribute("aria-label", product.name);
  host.hidden = true;
  deps.stage.append(host);
  let runtime: WorkerPairingHostRuntime | null = null;
  let open: OpenProduct | null = null;
  /** Settles once the open Example Chat has connected for invitations. */
  let connected: Promise<void> = Promise.resolve();
  let onConnected = () => {};
  /** Counts opens and closes, so an open that a close overtook ends itself. */
  let generation = 0;
  const listeners: (() => void)[] = [];
  const changed = () => {
    for (const listener of listeners) listener();
  };

  const closeExampleChat = (): void => {
    generation += 1;
    service.detachProduct();
    open?.dispose();
    open = null;
    onConnected();
  };

  /** Open Example Chat as any product opened by name: from its archive, through the loader, connected to the core. */
  const openExampleChat = async (): Promise<void> => {
    if (runtime === null) throw new Error("The core is not running yet.");
    closeExampleChat();
    const current = generation;
    connected = new Promise((resolve) => (onConnected = resolve));
    const response = await fetch(`${deps.base}${EXAMPLE_CHAT_PATH}`, {
      cache: "no-store",
    });
    if (!response.ok)
      throw new Error(
        `The dev server did not build Example Chat: ${await response.text()}`,
      );
    const archive = (await response.json()) as DemoWorkerAnswer;
    const scope = mountScope(deps.base, {
      walletId: testUserScope(self),
      productId: product.id,
      cid: archive.cid,
      network: deps.network.id,
    });
    const opened = await openProduct(
      runtime,
      loaderUrl({
        base: deps.base,
        origin: window.location.origin,
        scope,
        gateway: window.location.origin,
        kind: archive.kind,
        start: "/",
      }),
      product.id,
      host,
      {
        mounted: true,
        tap: deps.tap(product.id),
        onSandboxStatus: (state, detail) => {
          if (state === "error")
            deps.log(`Example Chat did not start: ${detail}`);
        },
        onInvitationsPort: (port) => {
          if (current !== generation) return;
          if (!verified) return port.close();
          service.attachProduct(productPort(port));
          onConnected();
        },
      },
    );
    if (current !== generation) {
      opened.dispose();
      return;
    }
    open = opened;
    deps.showProduct();
    changed();
  };

  /**
   * Open Example Chat unless it is, and wait until it can take an
   * invitation, at most {@link CONNECT_TIMEOUT_MS}. The user chose Accept, so
   * the product the invitation is for opens for it.
   */
  const ensureOpen = async (): Promise<void> => {
    if (open === null) await openExampleChat();
    let timer: ReturnType<typeof setTimeout> | undefined;
    await Promise.race([
      connected,
      new Promise<void>((resolve) => {
        timer = setTimeout(resolve, CONNECT_TIMEOUT_MS);
      }),
    ]);
    clearTimeout(timer);
  };

  window.addEventListener("storage", (event) => {
    if (
      verified &&
      (event.key === null || Object.values(keys).includes(event.key))
    )
      service.reload();
  });
  window.addEventListener("pagehide", () => {
    service.stop();
    open?.dispose();
  });

  return {
    chat,
    notificationActions: {
      canAccept: (item) => service.owns(item.id) || chat.owns(item.id),
      // Contact Chat runs as a worker: its Accept needs no frame opened.
      accept: (item) =>
        chat.owns(item.id)
          ? chat.accept(item)
          : ensureOpen().then(() => service.requestAccept(item.id)),
      // Example Chat lists what the user accepted; Contact Chat's conversation is in Chat.
      destination: (item) =>
        chat.owns(item.id)
          ? "Chat"
          : service.owns(item.id)
            ? product.name
            : null,
      sent: () =>
        [
          ...sentRequests(service, product, (account) =>
            contacts.nameOf(account),
          ),
          ...chat.sent(),
        ].sort((left, right) => right.createdAt - left.createdAt),
      cancel: (id) => {
        if (!chat.cancel(id)) service.requestCancel(id);
      },
      onSentChange: (listener) => {
        const stops = [
          service.onOutgoing(listener),
          chat.onSentChange(listener),
        ];
        return () => stops.forEach((each) => each());
      },
      // An accepted row opens Chat for Contact Chat, and Example Chat as its Accept did.
      open: (item) => {
        if (item.state.tag !== "accepted") return;
        if (chat.owns(item.id)) {
          chat.open(item);
          return;
        }
        if (!service.owns(item.id)) return;
        void ensureOpen().then(
          () => service.requestOpen(item.id),
          (error: unknown) =>
            deps.log(`Example Chat did not open: ${String(error)}`),
        );
      },
    },
    async start(current) {
      runtime = current;
      deps.useStorage(testUserScope(self));
      await current.activateLocalSession(testUserEntropy(self));
      deps.log(`Signed in to test account ${self.name}.`);
      verifiedOnce = walletProblem();
      const problem = await verifiedOnce;
      if (problem !== null) {
        deps.log(`Invitations stay off: ${problem.reason}`);
        await chat.start();
        return;
      }
      await contacts.refresh();
      verified = true;
      service.start();
      await chat.start();
    },
    show: (visible) => {
      host.hidden = !visible;
    },
    isOpen: () => open !== null,
    open: openExampleChat,
    close: () => {
      if (open === null) return;
      closeExampleChat();
      deps.log("Example Chat closed.");
      deps.showProduct();
      changed();
    },
    onChange: (listener) => listeners.push(listener),
    reset: () => {
      service.stop();
      chat.stop();
      for (const part of Object.keys(
        PART_VERSION,
      ) as (keyof typeof PART_VERSION)[])
        for (let version = 1; version <= PART_VERSION[part]; version += 1)
          localStorage.removeItem(
            testUserKey(deps.network.id, self.id, part, version),
          );
    },
  };
}

/**
 * What clearing a test account's invitations removes, asked before it does.
 * Wallets and the core session are not touched.
 */
function confirmClear(user: TestUser): Promise<boolean> {
  return ask<boolean>({
    title: `Clear ${user.name}'s invitations?`,
    fields: [
      {
        label: "Removes",
        value: `${user.name}'s sent and received invitations and their Inbox items on this network, in this browser`,
      },
      {
        label: "Keeps",
        value:
          "Wallets, the other test accounts and Example Chat's own records",
      },
    ],
    choices: [
      { label: "Cancel", value: false, tone: "cancel" },
      { label: "Clear", value: true, tone: "destructive" },
    ],
    dismissed: false,
  });
}

function drawSection(
  deps: DevToolsDeps,
  testUser: TestUser | null,
  session: TestUserSession | null,
  useDemo: (id: string | null) => void,
): void {
  const demoSelect = el("select");
  demoSelect.id = "invitation-demo";
  demoSelect.append(
    new Option("Off", ""),
    ...TEST_USERS.map((user) => new Option(user.name, user.id)),
  );
  demoSelect.value = testUser?.id ?? "";
  demoSelect.title =
    "Act as a made-up user with fixture contacts, and send invitations between tabs of this browser. Switching reloads the tab.";
  demoSelect.addEventListener("change", () =>
    useDemo(demoSelect.value || null),
  );
  const demoLabel = el("label", "", "Invitation demo");
  demoLabel.htmlFor = demoSelect.id;

  const store = deps.notifications;
  const addSamples = button(
    "Add sample notifications",
    "notifications-demo-seed",
    () => samples.addSamples(store),
  );
  const clearSamples = button("Clear samples", "notifications-demo-clear", () =>
    samples.clearSamples(store),
  );
  const renderSamples = () => {
    addSamples.disabled = !store.scoped();
    clearSamples.disabled = !store.scoped();
  };
  store.onChange(renderSamples);
  renderSamples();

  const walletChat = el("input");
  walletChat.type = "checkbox";
  walletChat.id = "wallet-contact-chat";
  walletChat.checked = sessionStorage.getItem(WALLET_CHAT_KEY) === "on";
  walletChat.disabled = testUser !== null;
  walletChat.title =
    "Run Contact Chat for the signed-in wallet, with requests proved by its own keys and its contacts' published chat keys. Requests travel as Contact Chat delivery says: between tabs of this browser, or on the network. Switching reloads the tab.";
  walletChat.addEventListener("change", () => {
    if (walletChat.checked) sessionStorage.setItem(WALLET_CHAT_KEY, "on");
    else sessionStorage.removeItem(WALLET_CHAT_KEY);
    window.location.reload();
  });
  const walletChatLabel = el("label", "", "Contact Chat for this wallet");
  walletChatLabel.htmlFor = walletChat.id;
  const walletChatRow = el("div", "row");
  walletChatRow.append(walletChat, walletChatLabel);

  const deliverySelect = el("select");
  deliverySelect.id = "wallet-delivery";
  deliverySelect.append(
    new Option("This browser only", "local"),
    new Option("Network, validate claims only", "validate"),
    new Option("Network: claim and post", "network"),
  );
  deliverySelect.value = walletDelivery();
  deliverySelect.disabled = testUser !== null;
  deliverySelect.title =
    "How the wallet's Contact Chat requests travel. The network delivery posts to the network's shared day inbox, each post signed by a notification account the core claims with your personhood. Validate only asks the runtime whether each claim would pass and submits nothing. Switching reloads the tab.";
  deliverySelect.addEventListener("change", () => {
    const next = deliverySelect.value as WalletDelivery;
    const apply = () => {
      if (next === "local") sessionStorage.removeItem(WALLET_DELIVERY_KEY);
      else sessionStorage.setItem(WALLET_DELIVERY_KEY, next);
      window.location.reload();
    };
    if (next !== "network") return apply();
    void confirmNetworkDelivery(deps.network.id).then((confirmed) => {
      if (confirmed) apply();
      else deliverySelect.value = walletDelivery();
    });
  });
  const deliveryLabel = el("label", "", "Contact Chat delivery");
  deliveryLabel.htmlFor = deliverySelect.id;

  const rows: HTMLElement[] = [
    demoLabel,
    demoSelect,
    walletChatRow,
    deliveryLabel,
    deliverySelect,
  ];
  if (session !== null) {
    const exampleRow = el("div", "row");
    const openExample = button("", "open-example-chat", () => {
      const reopening = session.isOpen();
      session.open().then(
        () =>
          deps.log(
            reopening ? "Example Chat restarted." : "Example Chat opened.",
          ),
        (error: unknown) =>
          deps.log(`Example Chat did not open: ${String(error)}`),
      );
    });
    const renderExample = () => {
      openExample.textContent = session.isOpen()
        ? "Restart Example Chat"
        : "Open Example Chat";
      openExample.dataset.action = session.isOpen()
        ? "restart-example-chat"
        : "open-example-chat";
    };
    renderExample();
    session.onChange(renderExample);
    exampleRow.append(openExample);
    rows.push(exampleRow);
    const clearRow = el("div", "row");
    clearRow.append(
      button(
        `Clear ${testUser!.name}'s invitations`,
        "clear-test-invitations",
        () => {
          void confirmClear(testUser!).then((confirmed) => {
            if (!confirmed) return;
            session.reset();
            window.location.reload();
          });
        },
      ),
    );
    rows.push(clearRow);
  }
  const sampleRow = el("div", "row");
  sampleRow.append(addSamples, clearSamples);
  rows.push(sampleRow);

  deps.section.append(...rows);
  deps.section.hidden = false;
}
