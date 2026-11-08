import type { WorkerPairingHostRuntime } from "@parity/truapi-host/web";
import type { HexString } from "@parity/truapi/scale";
import type { ChatContact } from "../../src/chat/provider.js";
import { DEFAULT_REPLY_SUITE } from "../../src/invitations/invitation-service.js";
import type { ReplySuite } from "../../src/invitations/binding.js";
import type {
  ConversationAllowance,
  HostDelivery,
} from "../../src/invitations/ports.js";
import type { NotificationStore } from "../../src/notifications/notifications.js";
import { ask } from "../../src/app/prompt.js";
import {
  fixtureDirectory,
  type UsernameDirectory,
} from "../../src/contacts/username-directory.js";
import type { HandleNames } from "../../src/contacts/contacts.js";
import { FixtureContacts } from "../../fixtures/invitations/fixture-contacts.js";
import { LocalDelivery } from "../../fixtures/invitations/local-delivery.js";
import { pageStore } from "../../fixtures/invitations/shared-store.js";
import type { Network } from "../../src/platform/network.js";
import {
  isNoSubmitRefusal,
  noSubmitNetwork,
  type NoSubmitRecord,
} from "./no-submit.js";
import type { InvitationObserver } from "../../src/invitations/observer.js";
import { sessionChatKeys } from "../../src/invitations/chat-keys.js";
import { cryptoRandom } from "../../src/invitations/sealing.js";
import { WalletIdentityAuth } from "../../src/invitations/wallet-auth.js";
import {
  ownIdentityProblem,
  type IdentityProblem,
} from "../../src/invitations/published-keys.js";
import { startContactChatHost } from "../../src/invitations/contact-chat-host.js";
import {
  CONTACT_CHAT,
  confirmAccept,
  confirmSend,
  NOVA_COSTS,
  contactChatOwner,
  type ChatProductSpec,
  type RunningChat,
  type ContactChatOwner,
  type WalletDelivery,
} from "../../src/invitations/wallet-chat.js";
import * as samples from "../../fixtures/notifications/sample-notifications.js";
import {
  openSimulatedStatementStore,
  simulatedAllowance,
  simulatedStatements,
} from "./kettle-demo.js";
import { novaChat } from "../chat/nova.js";
import * as sr25519 from "@scure/sr25519";
import { hexToBytes } from "@parity/truapi/scale";
import { NovaBoundary } from "../../src/chat/nova-chat/boundary.js";
import { startNovaChatHost } from "../../src/invitations/nova-chat-host.js";
import { deriveHardSecret } from "../../src/invitations/chat-keys.js";
import { lookupChatIdentity } from "../../src/platform/chain-directory.js";
import { kettleChat } from "../chat/kettle.js";
import { backupTestNetwork, type BackupTestNetwork } from "./backup-demo.js";
import type { IrohRelays } from "../../src/chat/kettle-chat/route.js";
import type { ChatProductKey } from "../chat/products.js";
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
 * DEV ONLY. The invitation demo and the More tab's Developer section. Only a
 * dev server on this machine loads this module.
 *
 * The Chat tab (`../chat/products.ts`) chooses, for this tab, which chat
 * products run, the invitation demo's too. The Developer
 * section chooses how the signed-in wallet's host requests travel (the
 * network by default, as in a build) and the reply-key suite of new ones,
 * for every product that uses the request service.
 *
 * The invitation demo is chosen in More: a tab acts as a
 * made-up user (Alice, Bob or Carol) whose contacts, identity credentials
 * are fixtures, and invitations between tabs go through the local simulated
 * delivery. The core signs the demo user in with its public test phrase under
 * a storage scope of its own, so products get a real session, real handles
 * from `contacts.pick` and real prompts. The host names those handles through
 * the handles the core mints for the people it knows; the other users'
 * identity credentials stay fixtures. None of the wallet
 * session's services start: no saved contacts, Pocket or wallet workers, so
 * fixture contacts never reach a wallet's contacts or a transaction it signs.
 *
 * The demo runs the chat products the Chat tab turned on: Contact Chat
 * (`src/chat/contact-chat/`) by default, each a chat worker with its own
 * request service. It runs for the test user as an
 * ordinary product worker. The host's Chat shows its conversations and names
 * each person from the contact handle the product holds, through the host's
 * handle index; the product never gets a name. A chat ends at the
 * connection: Contact Chat sends no text.
 */

export interface DevToolsDeps {
  network: { id: string; networkSuffix: string };
  /** The host page's base path. */
  base: string;
  notifications: NotificationStore;
  /** Where the Developer section is drawn in the menu. */
  section: HTMLElement;
  /** The chat products this tab runs, as the Chat tab (`../chat/products.ts`) chose, the one New chat starts in first. */
  chatProducts: readonly ChatProductKey[];
  /** Kettle Chat's iroh relays, as the Chat tab chose, or null: off. */
  kettleIroh: IrohRelays | null;
  /** The app bar marker that names the test user. */
  marker: HTMLElement;
  /** End this tab's wallet session and product, before acting as a test user. */
  leaveWalletSession(): void;
  /** Keep the core's storage under `scope`, apart from every wallet's. */
  useStorage(scope: string): void;
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
  /** Told the request service's and the delivery's facts, for the inspector's Statements view. */
  observe?: InvitationObserver;
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
  /** The test user's Contact Chat, or null for the normal wallet session. */
  testChat: ContactChatOwner | null;
  /** How this tab's wallet requests travel, as the Developer section chose. */
  walletDelivery: {
    mode(): WalletDelivery;
    /** The simulated delivery between tabs of this browser, for the local choices. */
    local(self: string): HostDelivery;
  };
  /** The reply-key suite of this tab's new requests. */
  replySuite(): ReplySuite;
  /**
   * `network`, with every submission refused while the wallet's host
   * requests only validate claims: no extrinsic and no statement leaves this tab.
   * What it refused, and every method it let through, are kept on
   * `window.__truapiNoSubmit` for a test to read.
   */
  guardNetwork(network: Network): Network;
  /** Whether an RPC error is a refusal {@link guardNetwork}'s guard made in this tab. */
  refusedHere(error: { message?: string }): boolean;
  /** Where the test user's picker finds people by username: the other test users, or null. */
  usernames: UsernameDirectory | null;
  /** Once the core runs, sign the test user in and start receiving invitations. */
  start(runtime: WorkerPairingHostRuntime): Promise<void>;
  /** The network a browser test runs for the host's backup, in place of the real one, or null. */
  backupTestNetwork: BackupTestNetwork | null;
}

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

declare global {
  interface Window {
    /** What the no-submit guard of a validate-only tab saw, for a test to read. */
    __truapiNoSubmit?: NoSubmitRecord;
  }
}

/**
 * Where a tab keeps how the wallet's chat requests travel, when a developer
 * chose other than the network: `validate`, `local` or `local-validate`
 * (`src/invitations/wallet-chat.ts`).
 */
const WALLET_DELIVERY_KEY = "truapi-web-signing-host:dev:wallet-delivery";

/**
 * sessionStorage key: the reply-key suite of new host requests in this
 * tab, whichever product sends them. Absent: the request service's default, X-Wing.
 */
const REPLY_SUITE_KEY = "truapi-web-signing-host:dev:reply-suite";

/** The reply-key suite new requests of this tab use; read at each send. */
const devReplySuite = (): ReplySuite =>
  sessionStorage.getItem(REPLY_SUITE_KEY) === "x25519"
    ? "x25519"
    : DEFAULT_REPLY_SUITE;

function walletDelivery(): WalletDelivery {
  const value = sessionStorage.getItem(WALLET_DELIVERY_KEY);
  return value === "local-validate" || value === "validate" || value === "local"
    ? value
    : "network";
}

/** Whether `mode` keeps the tab from submitting anything. */
const submitsNothing = (mode: WalletDelivery) =>
  mode === "validate" || mode === "local-validate";

/** The simulated delivery between tabs of this browser, for the user whose chat key is `self`. */
function localDelivery(
  deps: DevToolsDeps,
  self: string,
  schedule: (task: () => void, delayMs: number) => () => void,
): HostDelivery {
  return new LocalDelivery({
    self,
    network: deps.network.id,
    store: pageStore(),
    now: () => Date.now(),
    schedule,
    random: cryptoRandom,
    log: deps.log,
  });
}

const timeout = (task: () => void, delayMs: number) => {
  const timer = setTimeout(task, Math.min(delayMs, 2 ** 31 - 1));
  return () => clearTimeout(timer);
};

export function startDevTools(deps: DevToolsDeps): DevSession {
  const { chatProducts } = deps;
  const testUser = tabTestUser(sessionStorage);
  const networkMode = (): WalletDelivery =>
    testUser === null ? walletDelivery() : "local";
  const session =
    testUser === null ? null : startTestUser(deps, testUser, chatProducts);
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
    backupTestNetwork: backupTestNetwork(deps.log),
    notificationsScope:
      testUser === null
        ? null
        : testUserKey(deps.network.id, testUser.id, "notifications"),
    scope: testUser === null ? null : testUserScope(testUser),
    contacts:
      testUser === null
        ? null
        : chatContactsOf(testUser, deps.network.networkSuffix),
    testChat: session?.owner ?? null,
    walletDelivery: {
      mode: walletDelivery,
      local: (self) => localDelivery(deps, self, timeout),
    },
    replySuite: devReplySuite,
    guardNetwork: (network) => {
      if (
        testUser !== null &&
        (chatProducts.includes("kettle-chat") ||
          chatProducts.includes("nova-chat"))
      )
        return simulatedStatements(network, deps.log);
      if (!submitsNothing(networkMode())) return network;
      deps.log(
        "This tab submits nothing: every extrinsic and statement submission is refused before it reaches the network.",
      );
      const guarded = noSubmitNetwork(network, deps.log);
      window.__truapiNoSubmit = guarded.record;
      return guarded.network;
    },
    refusedHere: (error) =>
      submitsNothing(networkMode()) && isNoSubmitRefusal(error),
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
    start: (runtime) => session?.start(runtime) ?? Promise.resolve(),
  };
}

interface TestUserSession {
  /** Contact Chat, which runs as the test user's worker. */
  owner: ContactChatOwner;
  start(runtime: WorkerPairingHostRuntime): Promise<void>;
  /** Stop, and remove this test user's lists on this network. */
  reset(): void;
}

/** A chat product of the invitation demo: its lists, and its conversation accounts' allowance when it posts. */
interface DemoChat {
  spec: ChatProductSpec;
  outbox: keyof typeof PART_VERSION;
  received: keyof typeof PART_VERSION;
  accounts?: ConversationAllowance;
}

function startTestUser(
  deps: DevToolsDeps,
  self: TestUser,
  chatProducts: readonly ChatProductKey[],
): TestUserSession {
  const suffix = deps.network.networkSuffix;
  // The wallet keys of the test user's phrase. Nothing is signed or opened
  // with them until `start` checked them against the core's session and the
  // published chat key.
  const walletKeys = testUserKeys(self, suffix);
  const auth = new WalletIdentityAuth(walletKeys);
  const directory = testUserDirectory(suffix);
  deps.notifications.setScope(
    testUserKey(deps.network.id, self.id, "notifications"),
  );
  const contacts = new FixtureContacts(self, deps.handles, {
    networkSuffix: suffix,
    directory,
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
  let verified: Promise<IdentityProblem | null> = new Promise(() => {});
  let core: WorkerPairingHostRuntime | null = null;
  const chatOf = (demo: DemoChat, index: number): RunningChat => ({
    workerPath: demo.spec.workerPath,
    ...(demo.spec.workerParams === undefined
      ? {}
      : { workerParams: demo.spec.workerParams }),
    startsChats: index === 0,
    host: startContactChatHost({
      product: demo.spec.product(suffix),
      keys: {
        sent: testUserKey(deps.network.id, self.id, demo.outbox),
        received: testUserKey(deps.network.id, self.id, demo.received),
      },
      auth,
      replySuite: devReplySuite,
      delivery: localDelivery(
        deps,
        auth.identityFor().agreementPublic,
        timeout,
      ),
      ...(demo.accounts === undefined ? {} : { accounts: demo.accounts }),
      contacts,
      handles: deps.handles,
      notifications: deps.notifications,
      // Checked in `start`, once the core runs the test user's session.
      verify: () => verified,
      confirmSend,
      showChat: deps.showChat,
      log: deps.log,
      ...(deps.observe === undefined ? {} : { observe: deps.observe }),
    }),
  });
  const demoOf = (key: ChatProductKey): DemoChat =>
    key === "contact-chat"
      ? {
          spec: CONTACT_CHAT,
          outbox: "contact-chat-outbox",
          received: "contact-chat-received",
        }
      : {
          spec: kettleChat(undefined, deps.kettleIroh),
          outbox: "kettle-chat-outbox",
          received: "kettle-chat-received",
          // Only Kettle Chat posts here, to the simulated Statement Store.
          accounts: simulatedAllowance(() => core, deps.log),
        };
  /**
   * Nova Chat for the test user: the official apps' requests and frames,
   * sealed and opened with the test user's keys, on the simulated Statement
   * Store the tabs share. Simulated: each notification claim and each
   * conversation account's allowance.
   */
  const novaOf = (index: number): RunningChat => {
    const spec = novaChat();
    const entropy = testUserEntropy(self);
    const chatKeyOf = async (account: Uint8Array) =>
      (await lookupChatIdentity(directory, account))?.chatKey ?? null;
    return {
      workerPath: spec.workerPath,
      startsChats: index === 0,
      host: startNovaChatHost({
        product: spec.product(suffix),
        storageKey: testUserKey(deps.network.id, self.id, "nova-chat"),
        boundary: NovaBoundary.fromEntropy(entropy, suffix, chatKeyOf),
        handles: deps.handles,
        nameOf: (account) => contacts.nameOf(account),
        lookupName: async (account) =>
          (
            await lookupChatIdentity(
              directory,
              hexToBytes(account as HexString),
            ).catch(() => null)
          )?.name ?? null,
        blocked: () => [],
        notifications: deps.notifications,
        verify: () => verified,
        openStore: openSimulatedStatementStore,
        notificationSigner: (kind, requestId, period) => {
          const secret = deriveHardSecret(entropy, [
            kind,
            requestId,
            String(period),
          ]);
          return {
            public: sr25519.getPublicKey(secret),
            sign: (payload) => sr25519.sign(secret, payload),
          };
        },
        claimNotification: async () => ({
          line: "simulated: nothing was claimed in the invitation demo.",
          seq: 0,
        }),
        accounts: simulatedAllowance(() => core, deps.log),
        confirmSend: (prompt) => confirmSend(prompt, NOVA_COSTS),
        confirmAccept: (prompt) => confirmAccept(prompt, NOVA_COSTS),
        showChat: deps.showChat,
        log: deps.log,
      }),
    };
  };
  const chats = chatProducts.map((key, index) =>
    key === "nova-chat" ? novaOf(index) : chatOf(demoOf(key), index),
  );
  const owner = contactChatOwner({
    base: deps.base,
    chats: () => chats,
    onSentChange: (listener) => {
      const stops = chats.map((chat) => chat.host.onSentChange(listener));
      return () => stops.forEach((stop) => stop());
    },
  });
  return {
    owner: {
      ...owner,
      // Everyone the test user's fixture directory knows.
      people: () =>
        TEST_USERS.filter((user) => user.id !== self.id).map((user) =>
          testUserAccount(user, suffix),
        ),
    },
    async start(runtime) {
      core = runtime;
      deps.useStorage(testUserScope(self));
      await runtime.activateLocalSession(testUserEntropy(self));
      deps.log(`Signed in to test account ${self.name}.`);
      verified = walletProblem();
      const problem = await verified;
      if (problem !== null) deps.log(`Invitations stay off: ${problem.reason}`);
      else await contacts.refresh();
      for (const chat of chats) await chat.host.start();
    },
    reset: () => {
      for (const chat of chats) chat.host.stop();
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
          "Wallets, the other test accounts and Contact Chat's own records",
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

  const rows: HTMLElement[] = [
    demoLabel,
    demoSelect,
    ...requestControls(deps, testUser),
  ];
  if (session !== null) {
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

/**
 * The request service's controls: how the wallet's host requests travel, and
 * the reply key of new ones. Every product that sets up through the request
 * service uses them, a chat product or any other.
 */
function requestControls(
  deps: DevToolsDeps,
  testUser: TestUser | null,
): HTMLElement[] {
  const deliverySelect = el("select");
  deliverySelect.id = "wallet-delivery";
  deliverySelect.append(
    new Option("Network: claim and post (default)", "network"),
    new Option("Network, validate claims only", "validate"),
    new Option("This browser only", "local"),
    new Option("This browser, validate chat claims only", "local-validate"),
  );
  deliverySelect.value = walletDelivery();
  deliverySelect.disabled = testUser !== null;
  deliverySelect.title =
    "How the wallet's host requests travel in this tab, for every product that uses the request service. The network delivery, the default and the only one a build has, posts to the network's shared day inbox, each post signed by a notification account the core claims with your personhood. Validate only asks the runtime whether each claim would pass and submits nothing; in this browser, requests stay between tabs, and with chat claims validated each chat account's claim is validated. Switching reloads the tab.";
  deliverySelect.addEventListener("change", () => {
    const next = deliverySelect.value as WalletDelivery;
    if (next === "network") sessionStorage.removeItem(WALLET_DELIVERY_KEY);
    else sessionStorage.setItem(WALLET_DELIVERY_KEY, next);
    window.location.reload();
  });
  const deliveryLabel = el("label", "", "Request delivery");
  deliveryLabel.htmlFor = deliverySelect.id;

  const suiteSelect = el("select");
  suiteSelect.id = "reply-key-suite";
  suiteSelect.append(
    new Option("X-Wing: hybrid post-quantum reply (default)", "xwing"),
    new Option("X25519: classical reply", "x25519"),
  );
  suiteSelect.value = devReplySuite();
  suiteSelect.title =
    "The kind of reply key your host makes for each new host request, whichever product sends it. The key's length tells the recipient's host how to seal its reply, and your host opens it the same way. Changing this changes only requests sent after it. The request itself and the identity proof stay classical either way.";
  suiteSelect.addEventListener("change", () => {
    const next = suiteSelect.value as ReplySuite;
    if (next === DEFAULT_REPLY_SUITE)
      sessionStorage.removeItem(REPLY_SUITE_KEY);
    else sessionStorage.setItem(REPLY_SUITE_KEY, next);
    deps.log(
      `New host requests in this tab use ${next === "xwing" ? "an X-Wing" : "an X25519"} reply key; requests already sent keep theirs.`,
    );
  });
  const suiteLabel = el("label", "", "Reply key for new requests");
  suiteLabel.htmlFor = suiteSelect.id;

  return [deliveryLabel, deliverySelect, suiteLabel, suiteSelect];
}
