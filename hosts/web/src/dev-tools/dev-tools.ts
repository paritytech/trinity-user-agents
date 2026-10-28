import type { WorkerPairingHostRuntime } from "@parity/truapi-host/web";
import {
  EXAMPLE_CHAT_PATH,
  type DemoWorkerAnswer,
} from "../chat-v2/local-demo-route.js";
import type { ChatContact } from "../chat-provider.js";
import type { FrameTap } from "../frame-tap.js";
import {
  InvitationService,
  productPort,
  type SendPrompt,
} from "../invitations/invitation-service.js";
import type { Product } from "../invitations/ports.js";
import { cryptoRandom } from "../invitations/sealing.js";
import type { NotificationStore } from "../notifications.js";
import type { NotificationActions } from "../notifications-screen.js";
import { openProduct, type OpenProduct } from "../product.js";
import { ask } from "../prompt.js";
import { loaderUrl, mountScope } from "../sandbox/mount.js";
import {
  fixtureDirectory,
  type UsernameDirectory,
} from "../username-directory.js";
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
import {
  TEST_USERS,
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
 * DEV ONLY. The menu's Developer section and the test user mode it controls.
 * Only a dev server on this machine loads this module.
 *
 * In test user mode a tab acts as a made-up user instead of a wallet. The
 * core signs the test user in with a public test phrase, so products get a
 * real session, real handles from `contacts.pick` and real prompts. None of
 * the wallet session's services start: no chat, worker or registration. The
 * host's contact picker lists the test user's contacts, Browse shows Example
 * Chat, and requests between tabs use fixture contacts and identities and the
 * local simulated delivery.
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
  /** Bring the test user's product, in Browse, to the front. */
  showProduct(): void;
  /** The host's contact picker, the one `contacts.pick` opens. */
  pickContact: PickContact;
  log(line: string): void;
}

export interface DevSession {
  /** The test user this tab acts as, or null for the normal wallet session. */
  testUser: TestUser | null;
  /** The Notifications scope of the test user, or null. */
  notificationsScope: string | null;
  /** The test user's contacts, for the host's contact picker, or null. */
  contacts: ChatContact[] | null;
  /** Where the test user's picker finds people by username: the other test users, or null. */
  usernames: UsernameDirectory | null;
  /** What the Notifications screen can do with the test user's invitations, or null. */
  notificationActions: NotificationActions | null;
  /** Once the core runs, sign the test user in and open Example Chat. */
  start(runtime: WorkerPairingHostRuntime): Promise<void>;
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
    from: prompt.product.name,
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

export function startDevTools(deps: DevToolsDeps): DevSession {
  const testUser = tabTestUser(sessionStorage);
  const session = testUser === null ? null : startTestUser(deps, testUser);
  drawSection(deps, testUser, session);
  deps.marker.hidden = testUser === null;
  if (testUser !== null) {
    deps.marker.textContent = `Test user · ${testUser.name}`;
    deps.marker.title =
      "A made-up user signed in with a public test phrase. Invitations go only to other tabs of this browser.";
  }
  return {
    testUser,
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
    showProduct: (visible) => session?.show(visible),
  };
}

interface TestUserSession {
  notificationActions: NotificationActions;
  start(runtime: WorkerPairingHostRuntime): Promise<void>;
  show(visible: boolean): void;
  restart(): Promise<void>;
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

  /** Open Example Chat as any product opened by name: from its archive, through the loader, connected to the core. */
  const openExampleChat = async (): Promise<void> => {
    if (runtime === null) return;
    service.detachProduct();
    open?.dispose();
    open = null;
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
    open = await openProduct(
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
        onInvitationsPort: (port) => service.attachProduct(productPort(port)),
      },
    );
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
      accept: (item) => service.requestAccept(item.id),
      // Example Chat lists what the user accepted; it has no chat conversation to open.
      destination: (item) => (service.owns(item.id) ? product.name : null),
      open: (item) => service.requestOpen(item.id),
    },
    async start(current) {
      runtime = current;
      deps.useStorage(testUserScope(self));
      await current.activateLocalSession(testUserEntropy(self));
      deps.log(`Signed in as test user ${self.name} (${self.handle}).`);
      service.start();
      await openExampleChat();
    },
    show: (visible) => {
      host.hidden = !visible;
    },
    restart: openExampleChat,
    reset: () => {
      service.stop();
      for (const key of Object.values(keys)) localStorage.removeItem(key);
      for (const part of ["outbox", "received"] as const)
        localStorage.removeItem(testUserKey(deps.network.id, self.id, part, 1));
    },
  };
}

function drawSection(
  deps: DevToolsDeps,
  testUser: TestUser | null,
  session: TestUserSession | null,
): void {
  const userSelect = el("select");
  userSelect.id = "test-user";
  userSelect.append(
    new Option("None (use a wallet)", ""),
    ...TEST_USERS.map((user) => new Option(user.name, user.id)),
  );
  userSelect.value = testUser?.id ?? "";
  userSelect.title =
    "Act as a made-up user in this tab. Switching reloads the tab.";
  userSelect.addEventListener("change", () => {
    if (userSelect.value !== "") deps.leaveWalletSession();
    setTabTestUser(sessionStorage, userSelect.value || null);
    window.location.reload();
  });
  const label = el("label", "", "Test user");
  label.htmlFor = userSelect.id;

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

  const rows: HTMLElement[] = [label, userSelect];
  if (session !== null) {
    const userRow = el("div", "row");
    userRow.append(
      button("Restart Example Chat", "restart-example-chat", () => {
        session.restart().then(
          () => deps.log("Example Chat restarted."),
          (error: unknown) =>
            deps.log(`Example Chat did not restart: ${String(error)}`),
        );
      }),
      button(`Reset ${testUser!.name}`, "reset-test-user", () => {
        session.reset();
        window.location.reload();
      }),
    );
    rows.push(userRow);
  }
  const sampleRow = el("div", "row");
  sampleRow.append(addSamples, clearSamples);
  rows.push(sampleRow);

  deps.section.append(...rows);
  deps.section.hidden = false;
}
