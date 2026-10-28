import type { WorkerPairingHostRuntime } from "@parity/truapi-host/web";
import {
  EXAMPLE_CHAT_PATH,
  type DemoWorkerAnswer,
} from "./local-demo-route.js";
import type { ChatContact } from "../../src/chat/provider.js";
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
import {
  FixtureContacts,
  type PickContact,
} from "../../fixtures/invitations/fixture-contacts.js";
import {
  FixtureIdentityAuth,
  publicIdentity,
} from "../../fixtures/invitations/fixture-identity.js";
import { LocalDelivery } from "../../fixtures/invitations/local-delivery.js";
import { pageStore } from "../../fixtures/invitations/shared-store.js";
import * as samples from "../../fixtures/notifications/sample-notifications.js";
import type { Wallet } from "../../src/platform/wallets.js";
import {
  TEST_USERS,
  testAccountWallet,
  chatContactsOf,
  fixtureIdentity,
  setTabTestUser,
  tabTestUser,
  testUserEntropy,
  testUserKey,
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
 * and handle are fixtures, and invitations between tabs go through the local
 * simulated delivery. The core signs the demo user in with its public test
 * phrase under a storage scope of its own, so products get a real session,
 * real handles from `contacts.pick` and real prompts. None of the wallet
 * session's services start, so fixture contacts never reach chat, workers or
 * a transaction. Example Chat, the demo's product, opens only when asked:
 * from More, or to accept an invitation. Its Close ends it.
 */

export interface DevToolsDeps {
  network: { id: string; networkSuffix: string };
  /** The host page's base path. */
  base: string;
  notifications: NotificationStore;
  /** Where the Developer section is drawn in the menu. */
  section: HTMLElement;
  /** The Account tab's fold that says what a test account is; the host shows it while one is picked. */
  about: HTMLElement;
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
  /** The host's contact picker, the one `contacts.pick` opens. */
  pickContact: PickContact;
  log(line: string): void;
}

export interface DevSession {
  /** The invitation demo user this tab acts as, or null for the normal wallet session. */
  testUser: TestUser | null;
  /** The wallets built from the public test phrases, for the account picker. */
  accounts: readonly Wallet[];
  /**
   * Start the invitation demo as `id` in this tab, ending its wallet
   * session, or leave it with null, back to the tab's wallet if one is set.
   * Either reloads the tab.
   */
  useDemo(id: string | null): void;
  /** The Notifications scope of the test user, or null. */
  notificationsScope: string | null;
  /** The test user's contacts, for the host's contact picker, or null. */
  contacts: ChatContact[] | null;
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
}

/** Example Chat's product id and name on a network. */
export const exampleChat = (networkSuffix: string): Product => ({
  id: `example-chat.${networkSuffix}`,
  name: "Example Chat",
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
    accounts: TEST_USERS.map(testAccountWallet),
    useDemo,
    notificationsScope:
      testUser === null
        ? null
        : testUserKey(deps.network.id, testUser.id, "notifications"),
    contacts: testUser === null ? null : chatContactsOf(testUser),
    usernames:
      testUser === null
        ? null
        : fixtureDirectory(
            "the test users",
            TEST_USERS.filter((user) => user.id !== testUser.id).map(
              (user) => ({ account: user.account, username: user.id }),
            ),
          ),
    notificationActions: session?.notificationActions ?? null,
    start: (runtime) => session?.start(runtime) ?? Promise.resolve(),
    exampleChatOpen: () => session?.isOpen() ?? false,
    closeExampleChat: () => session?.close(),
    showProduct: (visible) => session?.show(visible),
  };
}

interface TestUserSession {
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
  const credential = fixtureIdentity(self);
  deps.notifications.setScope(keys.notifications);
  const service = new InvitationService({
    product,
    auth: new FixtureIdentityAuth(credential, cryptoRandom),
    contacts: new FixtureContacts(self, deps.pickContact, deps.log),
    notifications: deps.notifications,
    storage: localStorage,
    keys,
    delivery: new LocalDelivery({
      self: publicIdentity(credential).agreementPublic,
      network: deps.network.id,
      store: pageStore(),
      now,
      schedule,
      random: cryptoRandom,
      log: deps.log,
    }),
    now,
    schedule,
    random: cryptoRandom,
    confirmSend,
    showProduct: () => deps.showProduct(),
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
  });
  window.addEventListener("pagehide", () => {
    service.stop();
    open?.dispose();
  });

  return {
    notificationActions: {
      canAccept: (item) => service.owns(item.id),
      accept: (item) => ensureOpen().then(() => service.requestAccept(item.id)),
      // Example Chat lists what the user accepted; it has no chat conversation to open.
      destination: (item) => (service.owns(item.id) ? product.name : null),
      // An accepted row opens Example Chat, as its Accept did.
      open: (item) => {
        if (item.state.tag !== "accepted" || !service.owns(item.id)) return;
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
      service.start();
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
      for (const key of Object.values(keys)) localStorage.removeItem(key);
      for (const part of ["outbox", "received"] as const)
        localStorage.removeItem(testUserKey(deps.network.id, self.id, part, 1));
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
  deps.about.append(
    el(
      "p",
      "note",
      "A test account signs in with a public recovery phrase and works as any wallet does. Anyone can sign as it, so keep nothing of value on it. Account status shows what the chain records for it.",
    ),
  );

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
