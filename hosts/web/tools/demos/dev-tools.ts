import type { WorkerPairingHostRuntime } from "@parity/truapi-host/web";
import type { ContactHandle } from "@parity/truapi";
import type { HexString } from "@parity/truapi/scale";
import {
  CONTACT_CHAT_PATH,
  EXAMPLE_CHAT_PATH,
  type DemoWorkerAnswer,
} from "./local-demo-route.js";
import type { ChatContact } from "../../src/chat/provider.js";
import type {
  ChatV2ProviderOptions,
  HandleContacts,
} from "../../src/chat/draft/provider.js";
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
import { sessionChatKeys } from "../../src/chat/nova-session.js";
import { WalletIdentityAuth } from "../../src/invitations/wallet-auth.js";
import { ownIdentityProblem } from "../../src/invitations/published-keys.js";
import { LocalDelivery } from "../../fixtures/invitations/local-delivery.js";
import { LocalTopics } from "../../fixtures/invitations/local-topics.js";
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
 *   name. Its text goes on conversation topics through the local simulated
 *   topics (`fixtures/invitations/local-topics.ts`).
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
  log(line: string): void;
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
   * Everyone the test user's fixture directory knows, by account, or null:
   * the host may name them by handle, as senders, without resolving them for
   * a transaction.
   */
  people: HexString[] | null;
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
  /** The workers the test user runs: Contact Chat. Empty outside the demo. */
  workerNames(): string[];
  /** Whether `name` is a worker this module resolves. */
  resolvesWorker(name: string): boolean;
  /** Contact Chat's worker, as the dev server describes the archive it built. */
  resolveWorker(): Promise<ResolvedWorker>;
  /** Whether `productId` may give its rooms contacts: Contact Chat, for a test user. */
  isContactChat(productId: string): boolean;
  /** What the host lends Contact Chat's chat provider, or null for another product. */
  chatOptions(productId: string): ChatV2ProviderOptions | null;
  /** The request service, for Contact Chat's worker frame only. */
  requests: WorkerPortService;
  /** The local simulated topics, for Contact Chat's worker frame only. */
  topics: WorkerPortService;
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

function expiresIn(ms: number): string {
  const minutes = Math.round(ms / 60_000);
  if (minutes < 60) return `in ${minutes} min`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `in ${hours} h`;
  const days = Math.round(hours / 24);
  return days === 1 ? "in 1 day" : `in ${days} days`;
}

/** The host's question before a product's invitation leaves. */
function confirmSend(prompt: SendPrompt): Promise<boolean> {
  return ask<boolean>({
    title: "Send invitation",
    from: prompt.product.id,
    fields: [
      { label: "To", value: prompt.to.name },
      { label: "Message", value: prompt.purpose },
      { label: "Expires", value: expiresIn(prompt.ttlMs) },
    ],
    choices: [
      { label: "Cancel", value: false, tone: "cancel" },
      { label: "Send", value: true, tone: "primary" },
    ],
    dismissed: false,
  });
}

/** How long Accept waits for Example Chat to connect after opening it. */
const CONNECT_TIMEOUT_MS = 15_000;

export function startDevTools(deps: DevToolsDeps): DevSession {
  const testUser = tabTestUser(sessionStorage);
  const session = testUser === null ? null : startTestUser(deps, testUser);
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
    people:
      testUser === null
        ? null
        : TEST_USERS.filter((user) => user.id !== testUser.id).map((user) =>
            testUserAccount(user, deps.network.networkSuffix),
          ),
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
    notificationActions: session?.notificationActions ?? null,
    start: (runtime) => session?.start(runtime) ?? Promise.resolve(),
    exampleChatOpen: () => session?.isOpen() ?? false,
    closeExampleChat: () => session?.close(),
    showProduct: (visible) => session?.show(visible),
    workerNames: () => (session === null ? [] : [session.chatProduct.id]),
    resolvesWorker: (name) => session?.chatProduct.id === name,
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
    isContactChat: (productId) => session?.chatProduct.id === productId,
    chatOptions: (productId) =>
      session?.chatProduct.id === productId ? session.chatOptions : null,
    requests: {
      connect: (productId, port) =>
        session?.chatProduct.id === productId
          ? session.attachChat(port)
          : port.close(),
      disconnect: (productId) => {
        if (session?.chatProduct.id === productId) session.detachChat();
      },
    },
    topics: {
      connect: (productId, port) =>
        session?.chatProduct.id === productId
          ? session.serveTopics(port)
          : port.close(),
      disconnect: (productId) => {
        if (session?.chatProduct.id === productId) session.stopTopics();
      },
    },
  };
}

interface TestUserSession {
  /** Contact Chat, which runs as the test user's worker. */
  chatProduct: Product;
  /** What the host lends Contact Chat's chat provider: names by handle, and starts by handle. */
  chatOptions: ChatV2ProviderOptions;
  /** Serve Contact Chat's request service port, in place of any before it. */
  attachChat(port: MessagePort): void;
  detachChat(): void;
  /** Serve Contact Chat's topic relay port, in place of any before it. */
  serveTopics(port: MessagePort): void;
  stopTopics(): void;
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

  // Contact Chat: its own request service, over the same wallet keys,
  // contacts and delivery, and its own lists.
  const chatProduct = contactChat(deps.network.networkSuffix);
  const chatKeys = {
    sent: testUserKey(deps.network.id, self.id, "contact-chat-outbox"),
    received: testUserKey(deps.network.id, self.id, "contact-chat-received"),
  };
  const chatService = new InvitationService({
    product: chatProduct,
    auth,
    contacts,
    notifications: deps.notifications,
    storage: localStorage,
    keys: chatKeys,
    delivery: delivery(),
    now,
    schedule,
    random: cryptoRandom,
    confirmSend,
    showProduct: () => deps.showChat(),
    log: deps.log,
  });
  const topics = new LocalTopics({
    network: deps.network.id,
    store: pageStore(),
    now,
  });
  let stopTopics = () => {};
  /** Contact Chat's request port, held until the wallet keys are checked. */
  let pendingChat: MessagePort | null = null;
  /**
   * Why the wallet keys may not prove this user, or null: they must be the
   * keys the core's session holds, and the ones published for its identity
   * account.
   */
  const walletProblem = async (): Promise<string | null> => {
    const core = await deps.coreSession();
    const checked = sessionChatKeys({
      entropy: testUserEntropy(self),
      networkSuffix: suffix,
      identityAccountId: core.identityAccountId as HexString | undefined,
      chatPublicKey: core.chatPublicKey as HexString | undefined,
    });
    if (typeof checked === "string") return checked;
    return ownIdentityProblem(directory, walletKeys);
  };
  // Naming needs no chat key: the host draws whom a handle names even when
  // it cannot reach them.
  const handleContacts: HandleContacts = {
    resolve: (handle: ContactHandle) => {
      const account = deps.handles.accountFor(handle.bytes);
      const user = account === null ? undefined : contacts.nameOf(account);
      return account === null || user === undefined
        ? null
        : { account, name: user.name, seed: user.handle };
    },
    handleFor: (account) => {
      if (contacts.nameOf(account) === undefined) return null;
      const bytes = deps.handles.handleFor(account);
      return bytes === null ? null : { bytes };
    },
  };
  const chatOptions: ChatV2ProviderOptions = {
    contacts: handleContacts,
    onNamesChange: (listener) => deps.handles.onChange(listener),
    start: {
      target: (account) => {
        const handle = handleContacts.handleFor(account);
        if (handle === null)
          throw new Error("Contact Chat cannot reach this person.");
        return { handle };
      },
      blocked: () => null,
    },
  };

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
    if (event.key === null || Object.values(keys).includes(event.key))
      service.reload();
    if (event.key === null || Object.values(chatKeys).includes(event.key))
      chatService.reload();
  });
  window.addEventListener("pagehide", () => {
    service.stop();
    chatService.stop();
    stopTopics();
    open?.dispose();
  });

  return {
    chatProduct,
    chatOptions,
    attachChat: (port) => {
      if (verified) chatService.attachProduct(productPort(port));
      else pendingChat = port;
    },
    detachChat: () => {
      pendingChat = null;
      chatService.detachProduct();
    },
    serveTopics: (port) => {
      stopTopics();
      stopTopics = topics.serve(port);
    },
    stopTopics: () => {
      stopTopics();
      stopTopics = () => {};
    },
    notificationActions: {
      canAccept: (item) => service.owns(item.id) || chatService.owns(item.id),
      // Contact Chat runs as a worker: its Accept needs no frame opened.
      accept: (item) =>
        chatService.owns(item.id)
          ? chatService.requestAccept(item.id)
          : ensureOpen().then(() => service.requestAccept(item.id)),
      // Example Chat lists what the user accepted; Contact Chat's conversation is in Chat.
      destination: (item) =>
        chatService.owns(item.id)
          ? "Chat"
          : service.owns(item.id)
            ? product.name
            : null,
      // An accepted row opens Chat for Contact Chat, and Example Chat as its Accept did.
      open: (item) => {
        if (item.state.tag !== "accepted") return;
        if (chatService.owns(item.id)) {
          chatService.requestOpen(item.id);
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
      const problem = await walletProblem();
      if (problem !== null) {
        deps.log(`Invitations stay off: ${problem}`);
        pendingChat?.close();
        pendingChat = null;
        return;
      }
      await contacts.refresh();
      verified = true;
      service.start();
      chatService.start();
      if (pendingChat !== null)
        chatService.attachProduct(productPort(pendingChat));
      pendingChat = null;
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
      chatService.stop();
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

  const rows: HTMLElement[] = [demoLabel, demoSelect];
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
