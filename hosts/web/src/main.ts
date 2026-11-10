import { createWebWorkerPairingHostRuntime } from "@parity/truapi-host/web";
import type { WorkerPairingHostRuntime } from "@parity/truapi-host/web";
import type { AuthState } from "@parity/truapi-host";
import { TRUAPI_CODEC_VERSION, TRUAPI_WIRE_SCHEMA_HASH } from "@parity/truapi";
import {
  effectiveProductIdFor,
  completeAddress,
  displayAddress,
  implicitSuffix,
  parseTypedAddress,
  type Address,
} from "./app/address.js";
import {
  accountStatusRows,
  abbreviate,
  productAllowanceRows,
  type ChainState,
  type ProductAllowanceView,
} from "./app/account-status.js";
import { allocationRecord } from "./platform/allowance-ledger.js";
import {
  permissionRows,
  queriesFor,
  type PermissionsView,
} from "./app/permissions-status.js";
import { bindRecentsMenu } from "./app/address-recents.js";
import { bindChrome } from "./app/chrome.js";
import { copyInPage } from "./app/copy.js";
import { messagePortBridge } from "../bridges/chat-v2/contract.js";
import { isProductHello } from "../bridges/host-requests/contract.js";
import { isNovaHello } from "../bridges/nova-chat/contract.js";
import { ChatV2Host } from "./chat/draft/host.js";
import { demoAllowed } from "../tools/demos/local-demo-route.js";
import { createHostCallbacks } from "./app/callbacks.js";
import { CONTACT_CHAT } from "./app/chat-products.js";
import { fetchPreimage, gatewayPreimages } from "./products/preimage.js";
import { connectNetwork, type Network } from "./platform/network.js";
import {
  CHAIN_ROLES,
  NETWORKS,
  chainBackends,
  networkConfig,
  type ChainBackend,
  type ChainRole,
  type NetworkConfig,
} from "./platform/network-config.js";
import { DEFAULT_NETWORK } from "./platform/network-scope.js";
import {
  chosenNetwork,
  rememberNetwork,
  savedChainBackends,
  tabProductKey,
  tabWalletKey,
  type NetworkChoice,
} from "./platform/network-choice.js";
import { ask, setPromptArea, setRequesterDescriber } from "./app/prompt.js";
import { describeRequester } from "./app/requester.js";
import { CHAIN_LABELS, bindChainBackends } from "./app/chain-backends.js";
import { RecentProducts, type RecentEntry } from "./app/recents.js";
import { BrowseLanding } from "./app/browse-landing.js";
import {
  readPeopleChain,
  type AccountReading,
  type AccountToRead,
} from "./platform/people-chain.js";
import {
  readBalance,
  readProductAccount,
  type ProductChainState,
} from "./app/product-chain.js";
import { StageLoading } from "./products/loading.js";
import {
  DESTINATIONS,
  DESTINATION_LABELS,
  destinationOf,
  isProductDestination,
  loadAddresses,
} from "./app/shell.js";
import { defaultAddresses } from "./app/dock-defaults.js";
import { drawPocket } from "./pocket/screen.js";
import { cardKey, pocketPlatform } from "./pocket/pocket.js";
import { PocketFaces } from "./pocket/faces.js";
import { bindProductCard } from "./pocket/card-view.js";
import { bulletinImageUrl } from "./products/render.js";
import { ChatScreen } from "./chat/screen.js";
import { bindWorkersMenu } from "./products/workers-menu.js";
import { NotificationStore } from "./notifications/notifications.js";
import { NotificationsScreen } from "./notifications/screen.js";
import { WalletChat } from "./invitations/wallet-chat.js";
import { ChatTabs, type ChatTab } from "./chat/tabs.js";
import { AppsShelf } from "./app/apps-shelf.js";
import { ChatInbox } from "./chat/inbox.js";
import {
  HostContacts,
  chooseContact,
  type PickedPerson,
} from "./contacts/contacts.js";
import { HostNames } from "./contacts/host-names.js";
import { WalletContacts } from "./contacts/wallet-contacts.js";
import {
  exactUsernameDirectory,
  fixtureDirectory,
  routeDirectory,
} from "./contacts/username-directory.js";
import { chainDirectory, lookupOwner } from "./platform/chain-directory.js";
import { StatementStore } from "./platform/statement-store.js";
import { bytesToHex, hexToBytes, type HexString } from "@parity/truapi/scale";
import { ProductChat } from "./chat/product-chat.js";
import { ProductWorkers } from "./products/product-workers.js";
import { resolveWorker } from "./products/worker-manifest.js";
import { productDetailsText, productStatusText } from "./products/status.js";
import { hostBase } from "./products/loader/mount.js";
import { HostStorage, localStorageChanges } from "./platform/storage.js";
import { SerialQueue } from "./app/serial-queue.js";
import { versionSections, type VersionRow } from "./app/versions.js";
import { SearchCard } from "./app/search-card.js";
import { bindDeviceMode } from "./app/device-mode.js";
import { followKeyboard } from "./app/keyboard-viewport.js";
import { SearchPeople } from "./contacts/search-people.js";
import {
  WalletPublicInfoStore,
  sessionUsernameFrom,
  usernameFrom,
  walletLabel,
} from "./app/wallet-label.js";
import { walletPanel, type WalletMode } from "./app/wallet-panel.js";
import { DEV_ACCOUNTS, devAccountWallet } from "./platform/dev-accounts.js";
import { WalletStore, type Wallet } from "./platform/wallets.js";
import {
  ProductNavigation,
  SANDBOX_FAILED,
  scopeOf,
} from "./app/navigation.js";
import { SessionMembership, WalletSession } from "./app/session.js";
import type { ConnectedAccount, TabSession } from "./app/tab-session.js";
import { chatRefusal } from "./invitations/gate.js";
import { networkDelivery } from "./invitations/wallet-delivery.js";
import {
  HostBackup,
  backupOn,
  type BackupNetwork,
  type BackupSession,
} from "./backup/host-backup.js";
import {
  readGrants,
  untilFinalizedAllowance,
} from "./platform/allowance-finality.js";
import { mountTools } from "../tools/mount.js";
import { ProductLoadLog } from "../tools/products/log.js";
import { ActivityLog } from "../tools/activity/log.js";
import { StatementLedger } from "../tools/statements/ledger.js";
import { peopleTransport } from "../tools/statements/transport.js";
import type { PeopleAccess } from "./platform/allowance-finality.js";
import {
  peopleAllowanceGuard,
  postingRule,
  type PostingRule,
} from "./platform/allowance-usable.js";
import {
  observeStatements,
  type TrafficSource,
} from "./platform/statement-observer.js";
import { WorkerConsoleLog } from "../tools/workers/console-log.js";
import type { ActivityCategory } from "./platform/activity.js";

/** The host's activity, which every part of the host reports to and the inspector shows. */
const activity = new ActivityLog();
/** The People chain the Statements view reads on demand, once the network is up. */
let peopleAccess: PeopleAccess | null = null;
/** This host's directory on its origin, from the build's base. Mounted products live under it. */
const BASE = hostBase(import.meta.env.BASE_URL, window.location.href);

/** The networks the compact menu offers, from the one catalog this host serves. */
const OFFERED_NETWORKS: NetworkChoice[] = Object.values(NETWORKS).map(
  (network) => ({ id: network.id, label: network.displayName }),
);

/**
 * The network this tab runs on, chosen before the core starts.
 *
 * The choice is per tab and applied by reloading, so the core, the wallet
 * session and every open product are built for one network and never switched
 * under a live page.
 */
const activeNetwork: NetworkConfig = networkConfig(
  chosenNetwork(sessionStorage, OFFERED_NETWORKS, DEFAULT_NETWORK),
);

/**
 * The backend each chain runs on in this tab, chosen before the core starts
 * and applied by reloading, like the network.
 */
const runningBackends: Record<ChainRole, ChainBackend> = chainBackends(
  activeNetwork,
  savedChainBackends(sessionStorage, activeNetwork.id),
);

/** When a Statement Store account may post after its claim, on the People chain this tab reaches. */
function currentPostingRule(): PostingRule {
  return peopleAccess === null
    ? { wait: "finalized", why: "the People chain is not connected" }
    : chainAccess?.postingReady?.() !== true
      ? {
          wait: "finalized",
          why: "the core's People connection is not verified for best-block posting",
        }
      : postingRule(activeNetwork, runningBackends.people, peopleAccess.genesis);
}

/**
 * Every post after a claim waits on this: conversation accounts, and the
 * notification accounts of host requests and Nova Chat. The backup does not
 * ({@link backupNetwork}): it always waits for the finalized head.
 */
const usableAllowance = peopleAllowanceGuard({
  people: () => peopleAccess,
  rule: currentPostingRule,
  log: (line) => linesFor("host")(line),
});

/**
 * What the inspector's Statements view knows: Statement Store traffic, claims
 * and reports, read as People's running backend gives them. The dev tools'
 * no-submit guard says which errors it made.
 */
const statements = new StatementLedger(
  peopleTransport(runningBackends.people),
  {},
  Date.now,
  (error): boolean => devSession?.refusedHere(error) ?? false,
);

function element<T extends HTMLElement>(id: string): T {
  const found = document.getElementById(id);
  if (!found) throw new Error(`missing #${id}`);
  return found as T;
}

const coreStatus = element("core-status");
const sessionLine = element("session");
const walletSelect = element<HTMLSelectElement>("wallet");
const signInButton = element<HTMLButtonElement>("sign-in");
const signOutButton = element<HTMLButtonElement>("sign-out");
const forgetButton = element<HTMLButtonElement>("forget");
/** What a test account is, shown while the picker points at one. */
const testAccountsAbout = element<HTMLDetailsElement>("test-accounts-about");
const walletNameInput = element<HTMLInputElement>("wallet-name");
const mnemonicInput = element<HTMLTextAreaElement>("mnemonic");
const importForm = element<HTMLFormElement>("import-form");
const importDetails = element<HTMLDetailsElement>("import");
const addWalletButton = element<HTMLButtonElement>("add-wallet");
const walletActions = element("wallet-actions");
const productForm = element<HTMLFormElement>("product-form");
const addressForm = element<HTMLFormElement>("address-form");
const addressInput = element<HTMLInputElement>("address");
const addressId = element("address-id");
const addressHintTyped = element("address-hint-typed");
const addressHintSuffix = element("address-hint-suffix");
const recentsList = element<HTMLUListElement>("address-recents");
const resetDataButton = element<HTMLButtonElement>("reset-data");
const addressGo = element<HTMLButtonElement>("address-go");
const productIdInput = element<HTMLInputElement>("product-id");
/** The Product tab's own address field. It mirrors the Pill card's. */
const productAddress = element<HTMLInputElement>("product-address");
const productRecents = element<HTMLUListElement>("product-recents");
const productIdSummary = element("product-id-summary");
const productDot = element("product-dot");
const appProblem = element("app-problem");
const productIdReset = element<HTMLButtonElement>("product-id-reset");
const productIdEffective = element("product-id-effective");
const openButton = element<HTMLButtonElement>("open");
const closeButton = element<HTMLButtonElement>("close-product");
const productStatus = element("product-status");
const productMore = element<HTMLDetailsElement>("product-more");
const productDetails = element("product-details");
const productFrame = element("product-frame");
// Prompts are drawn over the app, as the phone app draws its sheets.
setPromptArea(productFrame);
// A prompt names the product by its id; the host adds where it opened it from.
setRequesterDescriber((productId) =>
  describeRequester(productId, [
    ...navigation.runningAs(productId),
    ...(productWorkers.provider(productId) === null
      ? []
      : [{ kind: "Worker" as const, address: null }]),
  ]),
);
const productSlot = element("product-slot");
/** The host screen a minimized product goes back to: the last one shown. */
let lastHost: "chat" | "pocket" = "chat";
const appChrome = element("app-chrome");
const appMinimize = element<HTMLButtonElement>("app-minimize");
const appClose = element<HTMLButtonElement>("app-close");
const appMore = element<HTMLButtonElement>("app-more");
const appMoreMenu = element("app-more-menu");
const pocketScreen = element("pocket-screen");
/** What Browse shows with no product in front, drawn by {@link browseLanding}. */
const browseScreen = element("browse-screen");
/** What Pocket says about a card it could not open. */
const pocketStatus = document.createElement("p");
pocketStatus.className = "pocket-status";
pocketStatus.setAttribute("role", "status");
/** What the product workers write to their consoles, for the inspector's Workers view. */
const workerConsole = new WorkerConsoleLog();
/** What each mounted product's load resolved and fetched, for the inspector's Products view. */
const productLoads = new ProductLoadLog();
/** Rooms that product workers keep, and the workers that keep them. */
const productChat = new ProductChat(localStorage);
/**
 * The host end of the Chat Extension v2 draft bridge. It runs no call media:
 * no product here has a call engine.
 */
const chatV2 = new ChatV2Host({
  media: () => null,
  contactRooms: (productId) => contactChat.isContactChat(productId),
  log: linesFor("workers"),
});
/** The host's own names for the people of a chat product's contact rooms. */
const hostNames = new HostNames();
/** The signed-in wallet's saved contacts and blocked people. */
const walletContacts = new WalletContacts(localStorage);
const workerFrames = document.createElement("div");
workerFrames.id = "worker-frames";
workerFrames.hidden = true;
document.body.append(workerFrames);
const productWorkers = new ProductWorkers({
  runtime: () => runtime,
  resolve: (name, network) =>
    contactChat.resolvesWorker(name)
      ? contactChat.resolveWorker(name)
      : resolveWorker(name, network),
  defaults: () => contactChat.workerNames(),
  container: workerFrames,
  base: BASE,
  storage: localStorage,
  tap: (channel) =>
    runtime === null
      ? undefined
      : tools.tap(channel, runtime.coreWireSchemaHash),
  activity: activity.record,
  observer: workerConsole,
  diagnostics: statements.diagnostics,
  loads: productLoads.record,
  chatV2: {
    connect: (productId, port) =>
      chatV2.connect(productId, messagePortBridge(port)),
    disconnect: (productId) => chatV2.disconnect(productId),
  },
  // Only the chat products' workers are served these: host requests, and
  // Nova Chat's own port, which its integration serves.
  invitations: {
    accepts: (data, ports) =>
      isProductHello(data, ports) || isNovaHello(data, ports),
    connect: (productId, port) => contactChat.requests.connect(productId, port),
    disconnect: (productId) => contactChat.requests.disconnect(productId),
  },
});
/** The faces of Pocket's cards, drawn by the workers of the products behind them. */
const pocketFaces = new PocketFaces({
  workers: productWorkers,
  storage: localStorage,
  log: linesFor("workers"),
});
/** The chat providers of the signed-in session: product rooms, and chat products over the v2 draft bridge. */
const chatInbox = new ChatInbox({
  rooms: {
    chat: productChat,
    channel: (productId) => productWorkers.provider(productId),
    workerState: (productId) => productWorkers.state(productId),
    onWorkersChange: (listener) => productWorkers.onChange(listener),
    archiveImage: (productId, path) =>
      productWorkers.archiveImage(productId, path),
    gateway: () => activeNetwork.dotns.contentGateway,
    log: linesFor("workers"),
  },
  workerNames: () => productWorkers.names(),
  log: linesFor("host"),
  chatV2,
  chatV2Options: (productId) => contactChat.chatOptions(productId) ?? {},
});
/** Which of Chat's tabs shows: the conversations or the Inbox. */
let chatTab: ChatTab = "chats";
/**
 * The Inbox: Chat's Inbox tab and the Apps shelf's Inbox open this one screen
 * over this one store. Contact Chat's requests arrive, and are accepted or
 * cancelled, here.
 */
const chatTabs = new ChatTabs(showChatTab);
const notifications = (() => {
  const store = new NotificationStore(localStorage);
  // Attached from the start, so its count reads the same on any screen.
  element("chat-screen").append(chatTabs.root);
  const screen = new NotificationsScreen(
    element("notifications-screen"),
    store,
    (unseen) => chatTabs.render(chatTab, unseen),
    chatTabs.root,
  );
  return { store, screen };
})();
const chatScreen = new ChatScreen(
  element("chat-screen"),
  chatInbox,
  async () => {
    const person = await hostContacts.choose({ title: "New chat" }, () =>
      chatInbox.roomPeople(),
    );
    if (person !== null) hostNames.remember(person.account, person.name);
    return person;
  },
  chatTabs.root,
  {
    list: () => walletContacts.blockedPeople(),
    blockable: () => walletContacts.blockable(),
    // The host asks for itself, over its own contacts only: no one is looked up by username.
    choose: () =>
      chooseContact(
        { title: "Block a contact" },
        walletContacts.blockable(),
        fixtureDirectory("your contacts", []),
      ),
    block: (account) => walletContacts.block(account),
    unblock: (account) => walletContacts.unblock(account),
    onChange: (listener) => walletContacts.onChange(listener),
  },
);
bindWorkersMenu({
  form: element<HTMLFormElement>("worker-form"),
  input: element<HTMLInputElement>("worker-name"),
  add: element<HTMLButtonElement>("worker-add"),
  list: element<HTMLUListElement>("worker-list"),
  status: element("worker-status"),
  workers: productWorkers,
  inspect: (name) => {
    chrome.show("inspector");
    tools.inspectWorker(name);
  },
  signedIn: () => chatInbox.signedIn(),
  onSessionChange: (listener) =>
    chatInbox.onChange((change) => {
      if (change.type === "session") listener();
    }),
});
/**
 * The menu's Developer section: the invitation demo, sample data, and this
 * tab's wallet delivery and reply-key choices. Only a dev server on this
 * machine loads it, so a build has none of its code.
 */
/** Whether the dev server's tools run here: on this machine only. */
const devTools =
  import.meta.env.DEV && demoAllowed(true, window.location.hostname);
// The Chat tab, and the dev server's tools, load at once: the page binds the
// developer tools only after them. Each condition names its build constant
// in place, so a build drops a module whose constant is false.
const [chatTools, demoTools] = await Promise.all([
  import.meta.env.DEV || __KETTLE_CHAT_BUILD__
    ? import("../tools/chat/products.js")
    : null,
  import.meta.env.DEV && devTools
    ? import("../tools/demos/dev-tools.js")
    : null,
]);
/**
 * The Chat tab and the chat products it turned on: on the dev server, and in
 * a build made with Kettle Chat (`WEB_SIGNING_KETTLE_CHAT=1`). Elsewhere,
 * Contact Chat alone runs and the tab stays hidden.
 */
const chatModality =
  devTools || __KETTLE_CHAT_BUILD__
    ? chatTools!.startChatModality({
        panel: element("dev-chat"),
        tab: document.querySelector<HTMLElement>('[data-dev-tab="chat"]')!,
        networkSuffix: activeNetwork.networkSuffix,
        log: linesFor("host"),
      })
    : null;
const devSession =
  import.meta.env.DEV && devTools
    ? demoTools!.startDevTools({
        chatProducts: chatModality!.keys,
        kettleIroh: chatModality!.kettleIroh,
        network: activeNetwork,
        base: BASE,
        notifications: notifications.store,
        section: element("developer"),
        marker: element("test-user-marker"),
        leaveWalletSession: () => {
          sessionStorage.removeItem(tabWalletKey(activeNetwork.id));
          sessionStorage.removeItem(tabProductKey(activeNetwork.id));
        },
        useStorage: (scope) => storage.useWallet(scope, activeNetwork.id),
        showChat: () => void navigation.selectDestination("chat"),
        handles: {
          accountFor: (handle): HexString | null =>
            hostContacts.accountFor(handle),
          handleFor: (account): HexString | null =>
            hostContacts.handleFor(account),
          onChange: (listener): (() => void) => hostContacts.onChange(listener),
        },
        coreSession: () =>
          lastAuthState.tag === "Connected"
            ? Promise.resolve(lastAuthState.value)
            : new Promise((resolve) => connectedWaiters.push(resolve)),
        log: linesFor("host"),
        observe: statements.invitations,
        serial,
        syncProductsOnly: (scope) => sessionMembers.syncProductsOnly(scope),
      })
    : null;
/** Contact Chat for the signed-in wallet: the host's own chat, on the network unless the dev tools chose otherwise. */
const walletChat = new WalletChat({
  base: BASE,
  notifications: notifications.store,
  handles: {
    accountFor: (handle): HexString | null => hostContacts.accountFor(handle),
    handleFor: (account): HexString | null => hostContacts.handleFor(account),
    onChange: (listener): (() => void) => hostContacts.onChange(listener),
  },
  blocked: () => blockedPeople(),
  names: hostNames,
  namedChanged: () => hostContacts.namesChanged(),
  showChat: () => void navigation.selectDestination("chat"),
  log: linesFor("host"),
  observe: statements.invitations,
  delivery:
    devSession?.walletDelivery ??
    networkDelivery({
      log: linesFor("host"),
      observe: statements.invitations,
    }),
  ...(devSession === null ? {} : { replySuite: devSession.replySuite }),
  products:
    chatModality === null
      ? [CONTACT_CHAT]
      : chatModality.walletProducts(devSession?.probe),
});
/** Contact Chat in this tab: the invitation demo's test user's, or the signed-in wallet's. */
const contactChat = devSession?.testChat ?? walletChat.owner;
notifications.screen.setActions(contactChat.notificationActions);
chatModality?.follow({
  owner: contactChat,
  workers: productWorkers,
  inspect: (name) => {
    chrome.show("inspector");
    tools.inspectWorker(name);
  },
});
/** The signed-in wallet, as the tab acts as it. */
const walletTab: TabSession = {
  activeId: () => walletSession.active?.id ?? null,
  signIn: (wallet) => void walletSession.signIn(wallet),
  signOut: () => void walletSession.signOut(),
  name: (account) =>
    account.fullUsername ??
    account.liteUsername ??
    walletSession.active?.name ??
    "wallet",
  evidence: () => ({ id: walletSession.active?.id ?? "", who: "a wallet" }),
  sync: syncWalletSession,
  resume: async () => {
    const wallet = findWallet(
      sessionStorage.getItem(tabWalletKey(activeNetwork.id)) ?? "",
    );
    if (wallet === undefined) return;
    walletSelect.value = wallet.id;
    await walletSession.signIn(wallet);
  },
  autoOpens: true,
  emptyStageNote:
    "Pick Chat, Pocket or Browse, or open a product with the search button.",
  noChat: null,
};
/** Who this tab acts as: the signed-in wallet, or the invitation demo's made-up user on the dev server. */
const tabSession: TabSession = devSession?.tabSession ?? walletTab;
if (tabSession.noChat !== null) chatScreen.setNoChat(tabSession.noChat);
/** The wallets built into the host from public test phrases. */
const testAccounts = DEV_ACCOUNTS.map(devAccountWallet);
/** A wallet by id: an imported one, or a test account. */
function findWallet(id: string): Wallet | undefined {
  return wallets.find(id) ?? testAccounts.find((wallet) => wallet.id === id);
}
const isTestAccount = (id: string): boolean =>
  testAccounts.some((wallet) => wallet.id === id);
/** The people the signed-in wallet blocked, as its saved record keeps them. None without a wallet. */
function blockedPeople(): HexString[] {
  return walletContacts.blocked();
}
/** The account `username` names on this network's chains, or null. */
async function usernameOwner(username: string): Promise<HexString | null> {
  const access = chainAccess;
  if (access === null) throw new Error("the chain is not connected");
  const owner = await lookupOwner(
    chainDirectory(access.chain, access.genesis),
    username,
  );
  return owner === null ? null : (bytesToHex(owner) as HexString);
}
/**
 * Where the contact picker finds people by username: the other test users for
 * a test user; else this network's username index, through the dev server's
 * route on this machine; else, as in a build, the chain by exact username.
 */
const usernameDirectory =
  devSession?.usernames ??
  (import.meta.env.DEV &&
  demoAllowed(true, window.location.hostname) &&
  activeNetwork.identityBackend !== null
    ? routeDirectory({
        base: BASE,
        network: activeNetwork.id,
        owner: usernameOwner,
      })
    : exactUsernameDirectory(usernameOwner));
/**
 * The contact picker, over the wallet's saved contacts or the test user's:
 * the one products open with `contacts.pick` and Chat's New chat opens.
 */
const hostContacts = new HostContacts({
  list: () => devSession?.contacts ?? walletContacts.contacts(),
  onChange: (listener) => walletContacts.onChange(listener),
  choose: (asker, contacts, blocked) =>
    chooseContact(asker, contacts, usernameDirectory, blocked),
  blocked: blockedPeople,
  onSessionChange: (listener) =>
    chatInbox.onChange((change) => {
      if (change.type === "session") listener();
    }),
  removed: () => runtime?.notifyContactsChanged(),
  mint: async (accounts) =>
    (await runtime?.contactHandles(accounts.map(hexToBytes)))?.map(
      (handle) => bytesToHex(handle) as HexString,
    ),
  named: (): readonly HexString[] => contactChat.people(),
  log: linesFor("host"),
});
/** Everything that belongs to the signed-in session, handed each change in order. */
const sessionMembers = new SessionMembership({
  network: activeNetwork,
  pocketFaces,
  productWorkers,
  productChat,
  notifications: notifications.store,
  signedOutNotifications: devSession?.notificationsScope ?? null,
  hostNames,
  walletContacts,
  hidesWalletContacts: () => walletChat.hidesWalletContacts(),
  chatV2,
  chatInbox,
});
const dockButtons = DESTINATIONS.map((destination) => ({
  destination,
  button: document.querySelector<HTMLButtonElement>(
    `#dock [data-destination="${destination}"]`,
  )!,
}));
/** Whether the core runs, for the Apps shelf. The menu says it in full. */
let coreState: "starting" | "ready" | "failed" = "starting";
const appsShelf = new AppsShelf(
  element<HTMLButtonElement>("apps-toggle"),
  element("apps-shelf"),
  {
    inbox: openInbox,
    settings() {
      chrome.show("account");
      element("dev-account").focus();
    },
    open(key) {
      const back = DESTINATIONS.find((known) => known === key) ?? null;
      if (isProductDestination(back))
        void navigation.selectDestination(back, true);
      else if (navigation.isHeld(key))
        void serial(async () => {
          navigation.bringApp(key);
        });
    },
    close: (key) => navigation.closeApp(key),
  },
);
notifications.store.onChange(renderShelf);
const stageEmptyNote = element("stage-empty-note");
const stageLoadingPill = element("stage-loading-pill");
const stageLoadingText = element("stage-loading-text");

const chrome = bindChrome({
  stage: element("stage"),
  scrim: element("scrim"),
  panel: element("devtools"),
  focus: element<HTMLButtonElement>("dev-toggle"),
  close: element<HTMLButtonElement>("devtools-close"),
});
// A phone's keyboard covers the Docklet and the search field unless the page
// shrinks to what shows above it.
followKeyboard(document.documentElement, window.visualViewport);
bindDeviceMode({
  viewport: element<HTMLMetaElement>("viewport-meta"),
  fullscreen: element<HTMLButtonElement>("fullscreen"),
  zoomLock: element<HTMLInputElement>("zoom-lock"),
  status: element("device-status"),
});
/** The Pill's card, which holds the address field. */
const searchCard = new SearchCard(
  element<HTMLButtonElement>("pill"),
  element("search-card"),
  addressInput,
  element<HTMLButtonElement>("search-close"),
  () => searchPeople.reset(),
);
element("pill").addEventListener("click", () => appsShelf.close(false));
element("apps-toggle").addEventListener("click", () => searchCard.close(false));
const searchStatus = element("search-status");

const tools = mountTools({
  element,
  productFrame,
  activity,
  workers: productWorkers,
  workerConsole,
  statements,
  people: () => peopleAccess,
  network: () => activeNetwork.id,
  productLoads,
});

const versionsList = element("versions");
const versionsTechnical = element("versions-technical");
const versionsWarning = element("versions-warning");

/**
 * A value that copies `payload` when clicked, pressed with Enter or Space. What
 * is shown may be shortened; what is copied is the whole value. It copies only
 * on a click, and says so in place for a moment.
 */
function copyValue(shown: string, payload: string, label: string): HTMLElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "copyable";
  button.textContent = shown;
  button.setAttribute("aria-label", `${shown}. Copy ${label}`);
  button.addEventListener("click", () => {
    void copyInPage(payload).then((copied) => {
      button.textContent = copied ? "Copied" : "Copy failed";
      button.dataset.copy = copied ? "done" : "failed";
      setTimeout(() => {
        button.textContent = shown;
        delete button.dataset.copy;
      }, 1200);
    });
  });
  return button;
}

function versionLines(rows: VersionRow[]): HTMLElement[] {
  return rows.map((row) => {
    const line = document.createElement("div");
    const label = document.createElement("dt");
    label.textContent = row.label;
    const value = document.createElement("dd");
    if (row.copy === undefined) value.textContent = row.value;
    else value.append(copyValue(row.value, row.copy, row.label));
    if (row.state) value.dataset.state = row.state;
    if (row.title) value.title = row.title;
    if (row.detail) {
      const detail = document.createElement("span");
      detail.className = "detail";
      if (row.copyDetail === undefined) detail.textContent = row.detail;
      else detail.append(copyValue(row.detail, row.copyDetail, row.label));
      value.append(detail);
    }
    line.append(label, value);
    return line;
  });
}

/** Draw the versions list from the build and from what the core reports. */
function renderVersions(coreSchema: string | undefined): void {
  const { summary, mismatch, technical } = versionSections(__BUILD_INFO__, {
    clientSchema: TRUAPI_WIRE_SCHEMA_HASH,
    clientCodec: TRUAPI_CODEC_VERSION,
    coreSchema,
  });
  versionsList.replaceChildren(...versionLines(summary));
  versionsTechnical.replaceChildren(...versionLines(technical));
  versionsWarning.hidden = mismatch === null;
  versionsWarning.textContent = mismatch ?? "";
}
renderVersions(undefined);

const networkSelect = element<HTMLSelectElement>("network");
networkSelect.replaceChildren(
  ...OFFERED_NETWORKS.map((choice) => new Option(choice.label, choice.id)),
);
networkSelect.value = activeNetwork.id;
networkSelect.addEventListener("change", () => {
  rememberNetwork(sessionStorage, networkSelect.value);
  window.location.reload();
});

bindChainBackends({
  list: element("chain-backend-rows"),
  note: element("chain-backends-note"),
  apply: element<HTMLButtonElement>("chain-backends-apply"),
  defaults: element<HTMLButtonElement>("chain-backends-defaults"),
  network: activeNetwork,
  running: runningBackends,
  storage: sessionStorage,
  reload: () => window.location.reload(),
});

/** A line for the Activity view from the host page itself. */
function log(text: string, failed = false): void {
  activity.record({ category: "host", action: text, failed });
}

/** A sink for a part of the host that reports in lines, logged under `category`. */
function linesFor(category: ActivityCategory): (text: string) => void {
  return (text) => activity.record({ category, action: text });
}

/** Show or hide the loading pill over the stage. The frame is not touched. */
function renderLoading(text: string | null): void {
  stageLoadingPill.hidden = text === null;
  stageLoadingText.textContent = text ?? "";
  document.body.dataset.loading = text === null ? "off" : "on";
  productFrame.setAttribute("aria-busy", String(text !== null));
}
const loading = new StageLoading(renderLoading);

const wallets = new WalletStore(localStorage);
const walletInfo = new WalletPublicInfoStore(localStorage);
const storage = new HostStorage(localStorage, localStorageChanges);

let runtime: WorkerPairingHostRuntime | null = null;
/** The product in front, the ones held behind it, and how each is opened. */
const navigation: ProductNavigation = new ProductNavigation({
  network: activeNetwork,
  base: BASE,
  slot: productSlot,
  statusLine: productStatus,
  loading,
  serial,
  walletId: (): string | null => walletSession.active?.id ?? null,
  addresses: import.meta.env.DEV
    ? // Only a dev server reads an address set in this browser, so tests can
      // point Browse at a fixture product; a build uses the network's own.
      loadAddresses(localStorage, activeNetwork.id)
    : defaultAddresses(activeNetwork.id),
  autoOpens: tabSession.autoOpens,
  tap: (current, productId) => tools.tap(productId, current.coreWireSchemaHash),
  showAddress: (text) => {
    addressInput.value = text;
  },
  clearAddress: () => {
    addressInput.value = "";
    renderAddress();
  },
  render: renderProduct,
  resumed: () => void refreshPermissions(),
  recorded: (entry) => {
    recents.record(recentScope(), entry, activeNetwork.id);
    recentsMenu.refresh();
    renderProductRecents();
  },
  activity: activity.record,
  loads: productLoads.record,
});
/** The signed-in wallet, and how it changes. */
const walletSession: WalletSession = new WalletSession({
  network: activeNetwork,
  storage,
  walletInfo,
  navigation,
  serial,
  activity: activity.record,
  afterSignIn: (wallet) => prepareBackup(wallet),
});

/** The core, for a call that must not run without one. */
function coreRuntime(): WorkerPairingHostRuntime {
  if (runtime === null) throw new Error("The core is not running.");
  return runtime;
}

/**
 * What the backup does on the network: the core claims and stores, the host
 * reads and posts. Its pointer posts only once its account's allowance holds
 * at the finalized head, whatever the network's posting rule: recovery reads
 * it, and nothing keeps it up behind the post.
 */
const backupNetwork: BackupNetwork = {
  claimSlot: () => coreRuntime().claimBackupStatementAccount(true),
  settled: async (claim) => {
    await untilFinalizedAllowance(claim, {
      read: async (account) => {
        const access = peopleAccess;
        if (access === null)
          throw new Error("the People chain is not connected");
        const { block, values } = await readGrants(access, [account]);
        return { block, grant: values[0]?.value ?? null };
      },
      sleep: (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
      now: Date.now,
      running: () => walletSession.active !== null,
      wording: {
        account: "The backup's account",
        retry: "the backup tries again later",
      },
    });
  },
  storeSnapshot: (sealed) => coreRuntime().storeBackupBlob(sealed),
  fetchSnapshot: (key) =>
    fetchPreimage(
      activeNetwork.dotns.contentGateway,
      key,
      new AbortController().signal,
      fetch,
    ),
  openStore: async () => {
    const access = chainAccess;
    if (access === null) throw new Error("The chains are not connected.");
    return new StatementStore(
      await access.chain.connect(hexToBytes(access.genesis.people)),
    );
  },
};
/** The wallet's backup, kept unless this browser switched it off (`BACKUP_SWITCH_KEY`). */
const hostBackup = new HostBackup({
  storage: localStorage,
  core: {
    account: () => coreRuntime().backupAccount(),
    signPointer: (payload) => coreRuntime().signBackupPointer(payload),
  },
  // On the dev server, a browser test may run the network itself.
  ...(devSession?.backupTestNetwork ?? { network: backupNetwork }),
  acquire: (name, signal) =>
    new Promise((resolve, reject) => {
      if (navigator.locks === undefined)
        return reject(new Error("this browser has no Web Locks"));
      navigator.locks
        .request(
          name,
          { signal },
          () => new Promise<void>((release) => resolve(release)),
        )
        .catch(reject);
    }),
  log: (line, failed) => log(line, failed),
});
/**
 * The wallet whose backup was looked for, so its members may start, and
 * the backup session to keep while it is signed in, or null when this
 * session keeps none.
 */
let backupPrepared: { walletId: string; session: BackupSession | null } | null =
  null;

/**
 * Restore `wallet`'s backup when this browser holds nothing of it, then keep
 * its backup. Runs once its core session is active and before its workers
 * and products start, which wait for it ({@link syncChatSession}).
 */
async function prepareBackup(wallet: Wallet): Promise<void> {
  let kept: BackupSession | null = null;
  try {
    // The public development phrase keeps nothing of its own on the network.
    if (!backupOn(localStorage) || chatRefusal(wallet.entropy) !== null) return;
    // On the light client, each People connection's statement subscription
    // replaces the topic filter every connection shares
    // (`platform/allowance-finality.ts`), so a backup read would take the
    // chat's topics off it.
    if (runningBackends.people === "light-client") {
      log("Backup: off while People runs on the light client.");
      return;
    }
    const account =
      lastAuthState.tag === "Connected"
        ? lastAuthState.value
        : await new Promise<Extract<AuthState, { tag: "Connected" }>["value"]>(
            (resolve) => connectedWaiters.push(resolve),
          );
    const access = chainAccess;
    if (account.identityAccountId === null || access === null) {
      log("Backup: off for this wallet: it has no identity account.");
      return;
    }
    const session: BackupSession = {
      network: activeNetwork.id,
      walletId: wallet.id,
      entropy: wallet.entropy,
      peopleGenesis: access.genesis.people,
      identity: account.identityAccountId as HexString,
    };
    const wanted = () => walletSession.active?.id === wallet.id;
    const outcome = await hostBackup.prepare(session, wanted);
    // After a failed look a backup may exist that this browser could not
    // read; a backup of its own would replace it.
    if (outcome === "failed")
      log("Backup: not kept in this session, so it replaces no backup.");
    else if (outcome !== "unavailable") kept = session;
  } finally {
    backupPrepared = { walletId: wallet.id, session: kept };
    syncChatSession();
  }
}

/**
 * Session changes and product opens run one at a time, in order. Opening a
 * product awaits the core, so without this a second open or a wallet switch
 * could start before the first open has a handle to dispose, and leave a
 * product connected under a session it was not opened for.
 */
const queue = new SerialQueue(renderWallets, (error) =>
  log(errorText(error), true),
);

function serial(
  task: (runtime: WorkerPairingHostRuntime) => Promise<void>,
): Promise<void> {
  const ready = runtime;
  return ready ? queue.run(() => task(ready)) : Promise.resolve();
}

/** The wallet problem last logged, so a redraw does not repeat it. */
let loggedWalletProblem: string | null = null;
/** How many usable wallets are saved, as the picker last showed them. */
let savedCount = 0;
/** The wallet mode the import fold was last set for. */
let importMode: WalletMode | null = null;

/**
 * Show only the wallet controls that apply. The import fold follows the mode:
 * open when there is no wallet, closed once there is one, and left alone
 * while the mode does not change so a fold you opened stays open.
 */
function renderWalletPanel(): void {
  const selected = walletSelect.value;
  const panel = walletPanel({
    saved: savedCount + testAccounts.length,
    activeId: tabSession.activeId(),
    selectedId: findWallet(selected) === undefined ? "" : selected,
  });
  walletSelect.hidden = !panel.showPicker;
  signInButton.hidden = !panel.showSignIn;
  signOutButton.hidden = !panel.showSignOut;
  // A test account is not saved, so there is nothing to forget.
  forgetButton.hidden = !panel.showForget || !wallets.has(selected);
  testAccountsAbout.hidden = !isTestAccount(selected);
  walletActions.hidden = !(
    panel.showSignIn ||
    panel.showSignOut ||
    panel.showForget
  );
  accountStatusFold.hidden = panel.mode !== "signed-in";
  if (panel.mode !== importMode) {
    importMode = panel.mode;
    importDetails.open = panel.mode === "none";
  }
}

function renderWallets(): void {
  const selected = walletSelect.value || tabSession.activeId() || "";
  const saved = wallets.list();
  const problem = wallets.problem();
  const walletOptions = saved.map(
    (wallet) =>
      new Option(
        walletLabel(wallet, walletInfo.get(wallet.id, activeNetwork.id)),
        wallet.id,
      ),
  );
  if (testAccounts.length === 0) walletSelect.replaceChildren(...walletOptions);
  else {
    const group = (label: string, options: HTMLOptionElement[]) => {
      const node = document.createElement("optgroup");
      node.label = label;
      node.append(...options);
      return node;
    };
    walletSelect.replaceChildren(
      ...(walletOptions.length === 0
        ? []
        : [group("Your wallets", walletOptions)]),
      group(
        "Shared test accounts",
        testAccounts.map((wallet) => {
          // A test account's id says nothing, so until it has signed in its
          // name alone tells it apart.
          const info = walletInfo.get(wallet.id, activeNetwork.id);
          return new Option(
            info === undefined ? wallet.name : walletLabel(wallet, info),
            wallet.id,
          );
        }),
      ),
    );
  }
  if (saved.length === 0 && problem !== null) {
    const notice = new Option(problem, "", false, true);
    notice.disabled = true;
    walletSelect.append(notice);
  }
  if (problem !== loggedWalletProblem && problem !== null)
    activity.record({ category: "session", action: problem, failed: true });
  loggedWalletProblem = problem;
  if (findWallet(selected) !== undefined) walletSelect.value = selected;
  else if (testAccounts.length > 0) {
    // No account is chosen for the user: not even a test account.
    const choose = new Option("Choose an account", "", true, true);
    choose.disabled = true;
    walletSelect.prepend(choose);
  }
  savedCount = saved.length;
  renderControls();
}

function renderControls(): void {
  const ready = runtime !== null && !queue.busy;
  signInButton.disabled =
    !ready ||
    walletSelect.value === "" ||
    walletSelect.value === tabSession.activeId();
  signOutButton.disabled = !ready || tabSession.activeId() === null;
  forgetButton.disabled = !ready || !wallets.has(walletSelect.value);
  addWalletButton.disabled = !ready;
  openButton.disabled = !ready;
  addressGo.disabled = !ready;
  closeButton.disabled = !ready || navigation.product === null;
  resetDataButton.disabled =
    !ready || navigation.product === null || walletSession.active === null;
  renderWalletPanel();
  renderAddress();
  renderDock();
}

/**
 * Show whether the address bar holds text that has not been opened.
 *
 * Typing never navigates. The bar only marks itself, and the product stays
 * loaded until Enter, the go button or the menu's Open button.
 */
function renderAddress(): void {
  const typed = completeAddress(addressInput.value, activeNetwork);
  const dirty =
    typed !== "" &&
    typed !== completeAddress(navigation.opened?.address ?? "", activeNetwork);
  addressForm.dataset.dirty = String(dirty);
  // The dimmed TLD follows a bare label. A leading space would shift it, so it waits.
  const suffix =
    addressInput.value === addressInput.value.trim()
      ? implicitSuffix(addressInput.value, activeNetwork)
      : "";
  addressHintTyped.textContent = suffix === "" ? "" : addressInput.value;
  addressHintSuffix.textContent = suffix;
  if (document.activeElement !== productAddress)
    productAddress.value = addressInput.value;
  renderEffectiveId();
}

/**
 * Show the id the next Open will use, from the address bar and the override,
 * and whether it is entered or derived. This reads no more than what is typed;
 * it never navigates.
 */
function renderEffectiveId(): void {
  productIdReset.disabled = productIdInput.value.trim() === "";
  let text: string;
  let state = "";
  let source = "";
  if (addressInput.value.trim() === "") {
    text = "Enter an address.";
  } else {
    try {
      const address = parseTypedAddress(addressInput.value, activeNetwork);
      const effective = effectiveProductIdFor(address, productIdInput.value);
      source = effective.source;
      if (!effective.usable) {
        state = "needs-id";
        text = `${effective.id}: enter a product id.`;
      } else {
        text =
          effective.source === "entered"
            ? `${effective.id} (entered, not verified)`
            : effective.id;
      }
      if (address.kind === "name" && !window.isSecureContext) {
        state = "needs-id";
        text += ". Names need https or localhost.";
      }
    } catch (error) {
      state = "needs-id";
      text = errorText(error);
    }
  }
  productIdEffective.textContent = text;
  productIdEffective.dataset.state = state;
  productIdEffective.dataset.source = source;
  const entered = productIdInput.value.trim();
  productIdSummary.textContent =
    entered === ""
      ? "Product id: from the address"
      : `Product id: ${entered} (entered)`;
}

/** The recent products as chips under the Product tab's field, newest first. */
function renderProductRecents(): void {
  const entries = recents.list(recentScope(), activeNetwork.id).slice(0, 5);
  productRecents.replaceChildren(
    ...entries.map((entry) => {
      const item = document.createElement("li");
      const chip = document.createElement("button");
      chip.type = "button";
      chip.textContent = displayAddress(entry.address, activeNetwork);
      chip.title = entry.entered ? `as ${entry.productId}` : entry.productId;
      chip.addEventListener("click", () => openRecent(entry));
      item.append(chip);
      return item;
    }),
  );
  productRecents.hidden = entries.length === 0;
  browseLanding.render();
}

/** A message for an error thrown here, without the `Error:` prefix. */
function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function renderProduct(): void {
  const { product, opened, sandboxStatus } = navigation;
  document.body.dataset.product = product ? "open" : "closed";
  addressId.hidden = product === null;
  addressId.textContent = product ? `as ${product.productId}` : "";
  const basis = opened?.via === "name" ? "the name" : "the address";
  addressId.title = product
    ? `Product id ${product.productId}, ${navigation.entered ? "entered by you" : `derived from ${basis}`}. Not verified.`
    : "";
  productStatus.textContent = product
    ? productStatusText(product, opened, sandboxStatus, window.isSecureContext)
    : "No product open.";
  const failed = sandboxStatus.startsWith(SANDBOX_FAILED);
  productDot.dataset.state =
    product === null
      ? "none"
      : failed
        ? "failed"
        : sandboxStatus === ""
          ? "ready"
          : "starting";
  appProblem.hidden = product === null || !failed;
  const details = product
    ? productDetailsText(product, opened, activeNetwork)
    : "";
  productDetails.textContent = details;
  productMore.hidden = details === "";
  renderHostScreens();
  renderControls();
  void refreshProductAllowance();
}

/** Mark the dock: where the product in front belongs, and which destinations still run behind it. */
function renderDock(): void {
  const { product, destination, dockAddresses } = navigation;
  const ready = runtime !== null && !queue.busy;
  for (const { destination: id, button } of dockButtons) {
    button.setAttribute("aria-current", String(destination === id));
    button.disabled = !ready;
    if (!isProductDestination(id)) {
      button.dataset.live = "false";
      button.title =
        id === "chat"
          ? "Chat: this wallet's conversations, and the Inbox of requests from products"
          : "Pocket: this wallet's identity and balance";
      continue;
    }
    button.dataset.live = String(
      navigation.isHeld(id) || (destination === id && product !== null),
    );
    button.title =
      dockAddresses[id] === ""
        ? `${DESTINATION_LABELS[id]}: the products opened before`
        : `${DESTINATION_LABELS[id]}: the Browse app (${dockAddresses[id]}) and the products opened before`;
  }
  stageEmptyNote.textContent = tabSession.emptyStageNote;
  renderShelf();
}

const focusCapsule = element<HTMLButtonElement>("dev-toggle");
/** The Focus capsule's colour: the core's state, and whether a session is signed in. */
function renderCoreDot(): void {
  const state =
    coreState !== "ready"
      ? coreState
      : lastAuthState.tag === "Connected"
        ? "ready"
        : "signed-out";
  focusCapsule.dataset.state = state;
  element("core-dot").dataset.state = state;
  const signed =
    lastAuthState.tag === "Connected" ? sessionLine.textContent : "Signed out";
  focusCapsule.setAttribute(
    "aria-label",
    `Developer tools. ${coreStatus.textContent ?? ""}. ${signed ?? ""}.`,
  );
}

function renderShelf(): void {
  renderCoreDot();
  const { store } = notifications;
  appsShelf.render({
    apps: navigation.openApps(),
    inbox: {
      unseen: store.unseen(),
      unseenFor: (productId) => store.unseen(productId),
    },
    core: {
      network: activeNetwork.displayName,
      state: coreState,
      detail: coreStatus.textContent ?? "",
    },
  });
}

/** Chat's tab bar: show the conversations or the Inbox, in place. */
function showChatTab(tab: ChatTab): void {
  chatTab = tab;
  chatTabs.render(chatTab, notifications.store.unseen());
  renderHostScreens();
  chatTabs.focus(tab);
}

/** The Apps shelf's Inbox: Chat, at its Inbox tab. */
function openInbox(): void {
  chatTab = "inbox";
  void navigation.selectDestination("chat").then(() => {
    if (navigation.destination !== "chat") return;
    showChatTab("inbox");
  });
}

/**
 * Show or hide the screens the host draws itself. Pocket is drawn once, the
 * first time it is shown; Chat draws itself from its engine.
 */
function renderHostScreens(): void {
  const { destination } = navigation;
  const surface =
    destination === "chat"
      ? chatTab === "inbox"
        ? "inbox"
        : "chat"
      : destination === "pocket"
        ? destination
        : destination === "browse" && navigation.product === null
          ? "browse"
          : "product";
  document.body.dataset.surface = surface;
  pocketScreen.hidden = surface !== "pocket";
  if (surface === "pocket" && pocketScreen.childElementCount === 0)
    pocketScreen.replaceChildren(
      ...drawPocket({
        productCard(card, root, bundled) {
          const key = cardKey(card, activeNetwork.networkSuffix);
          bindProductCard(root, bundled, {
            key,
            title: card.title,
            faces: pocketFaces,
            image: (source) =>
              source.tag === "Archive"
                ? productWorkers.archiveImage(key.productId, source.value)
                : bulletinImageUrl(
                    source.value,
                    activeNetwork.dotns.contentGateway,
                  ),
            open: () => void navigation.openPocketCard(key, pocketStatus),
            log: linesFor("workers"),
          });
        },
      }),
      pocketStatus,
    );
  pocketFaces.setShown(surface === "pocket");
  chatScreen.show(surface === "chat");
  notifications.screen.show(surface === "inbox");
  browseLanding.show(surface === "browse");
  if (destination === "chat" || destination === "pocket")
    lastHost = destination;
  renderAppChrome(surface === "product" && navigation.product !== null);
}

/**
 * The circles over a product in front: Minimize when the product can run
 * behind (an App, which every product opened by name or URL is), else Close,
 * and More. Minimize sends it behind, as choosing another destination does;
 * the Apps shelf brings it back, and Browse its own product.
 */
function renderAppChrome(shown: boolean): void {
  appChrome.hidden = !shown;
  // The product's frame then ends below the circles' band (style.css).
  productFrame.dataset.appChrome = String(shown);
  renderAppArea();
  if (!shown) setAppMenu(false);
  const holds = navigation.holdKey() !== null;
  appMinimize.hidden = !holds;
  appClose.hidden = holds;
}

const appArea = element("viewport-app-area");
/** The Preview tab's line on what a product in front gets of the app. */
function renderAppArea(): void {
  if (productFrame.dataset.appChrome !== "true") {
    appArea.textContent = "";
    return;
  }
  const style = getComputedStyle(productSlot);
  const top = Math.round(parseFloat(style.paddingTop));
  const bottom = Math.round(parseFloat(style.paddingBottom));
  const height = productSlot.clientHeight - top - bottom;
  appArea.textContent = `Product frame ${productSlot.clientWidth} × ${height} · the host keeps ${top} px on top${bottom === 0 ? "" : ` and ${bottom} px below`}`;
}
new ResizeObserver(renderAppArea).observe(productSlot);

function setAppMenu(open: boolean): void {
  appMoreMenu.hidden = !open;
  appMore.setAttribute("aria-expanded", String(open));
  if (open) appMoreMenu.querySelector<HTMLButtonElement>("button")?.focus();
}

/** Close the product in front, as the Product tab's Close does. */
const closeFront = (): void => navigation.closeFront();

// A product in Browse goes to the Apps shelf and Browse shows its landing;
// any other goes back to the last host screen.
appMinimize.addEventListener("click", () => {
  if (isProductDestination(navigation.destination))
    void navigation.minimizeToLanding();
  else void navigation.selectDestination(lastHost);
});
element("app-problem-close").addEventListener("click", closeFront);
element("app-problem-details").addEventListener("click", () =>
  chrome.show("product"),
);
appClose.addEventListener("click", closeFront);
appMore.addEventListener("click", () => setAppMenu(appMoreMenu.hidden));
appMoreMenu.addEventListener("click", (event) => {
  const action = (event.target as HTMLElement).closest<HTMLElement>(
    "[data-app-action]",
  )?.dataset.appAction;
  if (action === undefined) return;
  setAppMenu(false);
  if (action === "close") closeFront();
  else chrome.show("product");
});
appMoreMenu.addEventListener("keydown", (event) => {
  if (event.key !== "Escape") return;
  event.stopPropagation();
  setAppMenu(false);
  appMore.focus();
});
document.addEventListener(
  "pointerdown",
  (event) => {
    const target = event.target as Node | null;
    if (
      !appMoreMenu.hidden &&
      target !== null &&
      !appMoreMenu.contains(target) &&
      !appMore.contains(target)
    )
      setAppMenu(false);
  },
  true,
);

/** Give the Chat screen and everything else of the session the tab's session, or none. */
function syncChatSession(): void {
  const signedIn = lastAuthState.tag === "Connected";
  // The label goes into copied evidence: it names no wallet.
  const { id, who } = tabSession.evidence();
  tools.statementsSession(
    `${activeNetwork.id}:${signedIn ? "in" : "out"}:${id}`,
    signedIn ? `${who}, signed in` : "signed out",
  );
  tabSession.sync(
    lastAuthState.tag === "Connected" ? lastAuthState.value : null,
  );
}

/** Give everything of the signed-in wallet's session the account the core reports, or none. */
function syncWalletSession(account: ConnectedAccount | null): void {
  const access = chainAccess;
  const active = walletSession.active;
  // A wallet's members wait until its backup was looked for, so a restore
  // lands before anything reads or writes the wallet's state.
  const prepared = backupPrepared?.walletId === active?.id;
  const wallet =
    active !== null && backupOn(localStorage) && !prepared ? null : active;
  // The backup is kept while its wallet is signed in, whatever path the
  // core's session took to get there.
  const keeping =
    account !== null && prepared ? (backupPrepared?.session ?? null) : null;
  if (keeping === null) hostBackup.stop();
  else if (!hostBackup.keeps(keeping)) hostBackup.start(keeping);
  // Before the session's workers start, so they see Contact Chat among them.
  walletChat.setWallet(
    account === null || access === null || wallet === null
      ? null
      : {
          walletId: wallet.id,
          entropy: wallet.entropy,
          network: activeNetwork.id,
          networkSuffix: access.networkSuffix,
          identityAccountId: account.identityAccountId ?? undefined,
          chatPublicKey: account.chatPublicKey ?? undefined,
          directory: chainDirectory(access.chain, access.genesis),
          openStatementStore: async () =>
            new StatementStore(
              await access.chain.connect(hexToBytes(access.genesis.people)),
              undefined,
              activeNetwork.bestBlockPosting?.peopleGenesis,
            ),
          claimNotificationAccount: (target, submit) => {
            const current = runtime;
            return current === null
              ? Promise.reject(new Error("The core is not running."))
              : statements.claim(
                  { kind: "notification", account: bytesToHex(target), submit },
                  () => current.claimNotificationAccount(target, submit),
                );
          },
          claimProductStatementAccount: (productId, index, submit) => {
            const current = runtime;
            return current === null
              ? Promise.reject(new Error("The core is not running."))
              : statements.claim(
                  {
                    kind: "product",
                    product: productId,
                    index: bytesToHex(index),
                    submit,
                  },
                  () =>
                    current.claimProductStatementAccount(
                      productId,
                      index,
                      submit,
                    ),
                );
          },
          usableAllowance,
        },
  );
  sessionMembers.sync(
    account === null || access === null || wallet === null
      ? null
      : { wallet, access, account },
  );
}

const accountStatusList = element("account-status-rows");
const productStatusList = element("product-status-rows");
const permissionsList = element("permissions-rows");
const permissionsFold = element<HTMLDetailsElement>("permissions-status");
const accountStatusFold = element<HTMLDetailsElement>("account-status");
const accountStatusRefresh = element<HTMLButtonElement>(
  "account-status-refresh",
);
let lastAuthState: AuthState = { tag: "Disconnected" };
/** Who waits for the core to report a signed-in session. */
const connectedWaiters: ((
  session: Extract<AuthState, { tag: "Connected" }>["value"],
) => void)[] = [];
/** The chain and People genesis the status reads from, once the network is up. */
let chainAccess: Pick<
  Network,
  "chain" | "genesis" | "networkSuffix" | "postingReady"
> | null = null;
let chainState: ChainState = { state: "idle" };
/** The root key the chain state belongs to. */
let chainFor: string | null = null;
/** Counts reads and account changes, so a late reply for an older one is dropped. */
let chainRun = 0;
/** What the core's ledger records for the open product, and which read it came from. */
let productAllowance: ProductAllowanceView = { state: "none-open" };
let ledgerRun = 0;
/** The open product's account 0 and its Asset Hub balance, as the last Refresh read them. */
let productChain: ProductChainState = { state: "idle" };
/** Counts reads, product changes and account changes, so a late reply for an older one is dropped. */
let productChainRun = 0;
/** The core's saved permission answers for the open product, and which read they came from. */
let permissions: PermissionsView = { state: "none-open" };
let permissionsRun = 0;

function renderAccountStatus(): void {
  accountStatusList.replaceChildren(
    ...versionLines(
      accountStatusRows(
        lastAuthState,
        {
          networkName: activeNetwork.displayName,
          networkSuffix:
            chainAccess?.networkSuffix ?? activeNetwork.networkSuffix,
          walletName: walletSession.active?.name ?? null,
        },
        chainState,
      ),
    ),
  );
  productStatusList.replaceChildren(
    ...versionLines(productAllowanceRows(productAllowance, productChain)),
  );
  permissionsList.replaceChildren(...versionLines(permissionRows(permissions)));
  accountStatusRefresh.disabled =
    lastAuthState.tag !== "Connected" ||
    chainAccess === null ||
    chainState.state === "loading" ||
    productChain.state === "loading";
}

/**
 * Read the core's saved permission answers for the open product. The core's
 * own non-prompting status call, so nothing is asked and nothing changes. It
 * runs when the product, the account or a decision changes, and when the fold
 * opens; a reply for an older read is dropped.
 */
async function refreshPermissions(): Promise<void> {
  const run = ++permissionsRun;
  const productId = navigation.product?.productId;
  const ready = runtime;
  if (productId === undefined || ready === null) {
    permissions = { state: "none-open" };
  } else if (lastAuthState.tag !== "Connected") {
    permissions = { state: "signed-out" };
  } else {
    permissions = { state: "loading" };
    renderAccountStatus();
    try {
      const { capabilities, networks } = queriesFor(navigation.askedDomains);
      const queries = [...capabilities, ...networks];
      const statuses = await ready.getPermissionAuthorizationStatuses(
        productId,
        queries.map(({ request }) => request),
      );
      if (run !== permissionsRun) return;
      const answers = queries.map((query, index) => ({
        query,
        status: statuses[index],
      }));
      permissions = {
        state: "done",
        capabilities: answers.slice(0, capabilities.length),
        networks: answers.slice(capabilities.length),
      };
    } catch (error) {
      if (run !== permissionsRun) return;
      permissions = { state: "error", message: errorText(error) };
    }
  }
  renderAccountStatus();
}

/** Read again shortly after a decision, once the core has saved it. */
let permissionsTimer: ReturnType<typeof setTimeout> | undefined;
function refreshPermissionsSoon(): void {
  clearTimeout(permissionsTimer);
  permissionsTimer = setTimeout(() => void refreshPermissions(), 250);
}

permissionsFold.addEventListener("toggle", () => {
  if (permissionsFold.open) void refreshPermissions();
});

/**
 * Read what the core's ledger records for the open product. A local read of
 * this wallet's core storage, so it is cheap and runs whenever the product or
 * the account changes; a reply for an older product or account is dropped.
 */
async function refreshProductAllowance(): Promise<void> {
  const run = ++ledgerRun;
  if (
    productChain.state !== "idle" &&
    productChain.productId !== navigation.product?.productId
  ) {
    productChainRun += 1;
    productChain = { state: "idle" };
  }
  void refreshPermissions();
  const productId = navigation.product?.productId;
  if (productId === undefined) productAllowance = { state: "none-open" };
  else if (lastAuthState.tag !== "Connected")
    productAllowance = { state: "signed-out", productId };
  else {
    productAllowance = { state: "loading", productId };
    renderAccountStatus();
    try {
      const bytes = await storage.core.readCoreStorage({
        tag: "StatementRenewalTargets",
      });
      if (run !== ledgerRun) return;
      productAllowance = {
        state: "done",
        productId,
        record: allocationRecord(bytes, productId),
      };
    } catch {
      if (run !== ledgerRun) return;
      productAllowance = { state: "done", productId, record: "unreadable" };
    }
  }
  renderAccountStatus();
}

/**
 * Read the open product's account 0 from the core and its balance on Asset
 * Hub. Read-only and run on Refresh only. A reply is dropped when the product
 * or the account changed meanwhile, and the provider made for the read is
 * disposed whatever the outcome.
 */
async function refreshProductChain(): Promise<void> {
  const access = chainAccess;
  const productId = navigation.product?.productId;
  const current = runtime;
  if (
    productId === undefined ||
    current === null ||
    access === null ||
    lastAuthState.tag !== "Connected"
  ) {
    productChainRun += 1;
    productChain = { state: "idle" };
    return;
  }
  const run = ++productChainRun;
  productChain = { state: "loading", productId };
  renderAccountStatus();
  try {
    const account = await readProductAccount(current, productId);
    const balance = await readBalance(
      access.chain,
      access.genesis.assetHub,
      account,
    );
    if (run !== productChainRun) return;
    productChain = { state: "done", productId, account, balance };
  } catch (error) {
    if (run !== productChainRun) return;
    productChain = { state: "error", productId, message: errorText(error) };
    activity.record({
      category: "products",
      action: "Account not read",
      subject: productId,
      note: errorText(error),
      failed: true,
    });
  }
  renderAccountStatus();
}

/**
 * Read the signed-in account's standing from the People chain. Read-only, on
 * request and once per account when the fold is first opened; nothing polls.
 * A successful read also gives the session its username, through
 * {@link applySessionUsername}. A failed read changes only what this shows,
 * never the session.
 */
async function refreshChainStatus(): Promise<void> {
  const access = chainAccess;
  if (lastAuthState.tag !== "Connected" || access === null) return;
  const { publicKey, identityAccountId } = lastAuthState.value;
  const run = ++chainRun;
  chainState = { state: "loading" };
  renderAccountStatus();
  void refreshProductAllowance();
  void refreshProductChain();
  try {
    const reading = await readPeopleChain(
      access.chain,
      access.genesis.people,
      accountsToRead(publicKey, identityAccountId),
    );
    if (run !== chainRun) return;
    chainState = { state: "done", checkedAt: new Date(), reading };
    applySessionUsername(publicKey, reading.readings);
    const found = usernameFrom(reading.readings);
    const wallet = walletSession.active;
    if (found !== undefined && wallet !== null) {
      walletInfo.update(
        wallet.id,
        { publicKey, username: found },
        activeNetwork.id,
      );
      renderWallets();
    }
  } catch (error) {
    if (run !== chainRun) return;
    chainState = { state: "error", message: errorText(error) };
    activity.record({
      category: "session",
      action: "People chain not read",
      note: errorText(error),
      failed: true,
    });
  }
  renderAccountStatus();
}

function accountsToRead(
  publicKey: AccountToRead["accountId"],
  identityAccountId: AccountToRead["accountId"] | undefined,
): AccountToRead[] {
  return [
    ...(identityAccountId
      ? [{ role: "identity" as const, accountId: identityAccountId }]
      : []),
    { role: "root" as const, accountId: publicKey },
  ];
}

/**
 * Give the session the lite username the People chain records for it.
 *
 * The core gives a local session only the name it is activated with and
 * never looks one up, so `account.getUserId` answers with this name or with
 * none. When the chain's name differs from the session's, the session is
 * activated again with it, which closes and reopens the open product. A
 * reading for another account, or one whose record could not be decoded,
 * changes nothing.
 */
function applySessionUsername(
  publicKey: string,
  readings: AccountReading[],
): void {
  const wallet = walletSession.active;
  if (
    wallet === null ||
    lastAuthState.tag !== "Connected" ||
    lastAuthState.value.publicKey !== publicKey
  )
    return;
  const found = sessionUsernameFrom(readings);
  if (found === undefined) return;
  walletInfo.update(
    wallet.id,
    { publicKey, sessionUsername: found },
    activeNetwork.id,
  );
  if (found === (lastAuthState.value.liteUsername ?? null)) return;
  void walletSession.change(async (current) => {
    if (walletSession.active !== wallet) return;
    await current.activateLocalSession(wallet.entropy, found ?? undefined);
    activity.record({
      category: "session",
      action:
        found === null ? "Session username removed" : "Session username set",
      ...(found === null ? {} : { subject: found }),
      note:
        found === null
          ? "The People chain has no record."
          : "From the People chain.",
    });
  });
}

/**
 * Read the signed-in account's People chain record for the session username
 * only. Runs once per account when the status fold is closed; the fold's own
 * read does the same when it is open.
 */
async function syncSessionUsername(): Promise<void> {
  const access = chainAccess;
  if (lastAuthState.tag !== "Connected" || access === null) return;
  const { publicKey, identityAccountId } = lastAuthState.value;
  try {
    const reading = await readPeopleChain(
      access.chain,
      access.genesis.people,
      accountsToRead(publicKey, identityAccountId),
    );
    applySessionUsername(publicKey, reading.readings);
  } catch (error) {
    activity.record({
      category: "session",
      action: "Session username not read",
      note: errorText(error),
      failed: true,
    });
  }
}

/**
 * Forget the chain state when the account changes, and read it again for the
 * new one. A test account's chain state is read only when its status fold
 * opens: it has no wallet to give a username, so a read at sign-in would serve
 * nothing.
 */
function trackChainAccount(state: AuthState): void {
  const key = state.tag === "Connected" ? state.value.publicKey : null;
  if (key === chainFor) return;
  chainFor = key;
  chainRun += 1;
  chainState = { state: "idle" };
  productChainRun += 1;
  productChain = { state: "idle" };
  void refreshProductAllowance();
  if (key === null) return;
  if (accountStatusFold.open) void refreshChainStatus();
  // Only a wallet has a session username to read.
  else if (walletSession.active !== null) void syncSessionUsername();
}

accountStatusRefresh.addEventListener("click", () => {
  void refreshChainStatus();
});
accountStatusFold.addEventListener("toggle", () => {
  if (accountStatusFold.open && chainState.state === "idle")
    void refreshChainStatus();
});

function renderSession(state: AuthState): void {
  lastAuthState = state;
  if (state.tag === "Connected")
    for (const resolve of connectedWaiters.splice(0)) resolve(state.value);
  hostContacts.followSession(
    state.tag === "Connected" ? state.value.publicKey : null,
  );
  const wallet = walletSession.active;
  if (state.tag === "Connected" && wallet !== null) {
    // Public facts only, kept so the picker can tell wallets apart signed out.
    const { publicKey, fullUsername, liteUsername } = state.value;
    walletInfo.update(
      wallet.id,
      { publicKey, username: fullUsername ?? liteUsername ?? undefined },
      activeNetwork.id,
    );
    renderWallets();
  }
  trackChainAccount(state);
  syncChatSession();
  renderAccountStatus();
  sessionLine.title = "";
  switch (state.tag) {
    case "Connected": {
      const { publicKey, identityAccountId } = state.value;
      const name = tabSession.name(state.value);
      sessionLine.textContent = `Signed in as ${name} · ${abbreviate(publicKey)}`;
      sessionLine.title = `Root key ${publicKey}${identityAccountId ? `\nIdentity account ${identityAccountId}` : ""}`;
      break;
    }
    case "Disconnected":
      sessionLine.textContent = "Signed out";
      break;
    case "LoginFailed":
      sessionLine.textContent = `Sign-in failed: ${state.value.reason}`;
      break;
    default:
      sessionLine.textContent = state.tag;
  }
  renderCoreDot();
  renderProductRecents();
}

signInButton.addEventListener("click", () => {
  const wallet = findWallet(walletSelect.value);
  if (!wallet) return;
  tabSession.signIn(wallet);
});

signOutButton.addEventListener("click", () => {
  tabSession.signOut();
});

forgetButton.addEventListener("click", () => {
  const wallet = wallets.find(walletSelect.value);
  recents.forget(walletSelect.value);
  walletInfo.forget(walletSelect.value);
  wallets.forget(walletSelect.value);
  activity.record({
    category: "session",
    action: "Wallet forgotten",
    subject: wallet?.name ?? "the wallet",
  });
  walletSelect.value = "";
  renderWallets();
});

walletSelect.addEventListener("change", renderControls);

importForm.addEventListener("submit", (event) => {
  event.preventDefault();
  try {
    const wallet = wallets.save(
      walletNameInput.value.trim() || "Imported wallet",
      mnemonicInput.value,
    );
    mnemonicInput.value = "";
    walletNameInput.value = "";
    importDetails.open = false;
    renderWallets();
    walletSelect.value = wallet.id;
    tabSession.signIn(wallet);
  } catch (error) {
    activity.record({
      category: "session",
      action: "Wallet not imported",
      note: errorText(error),
      failed: true,
    });
  }
});

window.addEventListener("storage", (event) => {
  if (event.storageArea === localStorage && WalletStore.isChange(event))
    renderWallets();
});

/** Open what the address bar holds, under the menu's product id if any. */
function openAddress(): void {
  let address: Address;
  try {
    address = parseTypedAddress(addressInput.value, activeNetwork);
  } catch (error) {
    productStatus.textContent = searchStatus.textContent = errorText(error);
    activity.record({
      category: "products",
      action: "Address not read",
      subject: addressInput.value.trim(),
      note: errorText(error),
      failed: true,
    });
    return;
  }
  searchStatus.textContent = "";
  searchCard.close(false);
  const back = destinationOf(
    addressInput.value,
    navigation.dockAddresses,
    activeNetwork,
  );
  if (
    back !== null &&
    back !== navigation.destination &&
    navigation.isHeld(back)
  ) {
    void navigation.selectDestination(back);
    chrome.dismissCovering();
    return;
  }
  const { id: productId, source } = effectiveProductIdFor(
    address,
    productIdInput.value,
  );
  const entered = source === "entered";
  const origin = navigation.overrideOrigin;
  if (entered && origin !== null && origin !== scopeOf(address))
    activity.record({
      category: "products",
      action: "Entered product id moved",
      subject: productId,
      details: [{ label: "Now applies to", value: scopeOf(address) }],
    });
  void serial(async (runtime) => {
    const before = navigation.opened;
    if (await navigation.openTyped(runtime, address, productId, entered))
      return;
    // It did not open: say why where the address was typed.
    if (navigation.opened === null || navigation.opened === before) {
      searchStatus.textContent = productStatus.textContent;
      searchCard.open();
    }
  });
  chrome.dismissCovering();
}

const recents = new RecentProducts(localStorage);

/** Open a recent product as it was opened: its address, and its id when it was entered. */
function openRecent(entry: RecentEntry): void {
  addressInput.value = displayAddress(entry.address, activeNetwork);
  productIdInput.value = entry.entered ? entry.productId : "";
  renderAddress();
  openAddress();
}

const browseLanding = new BrowseLanding({
  root: browseScreen,
  catalog: () => {
    const address = navigation.dockAddresses.browse;
    return address === "" ? null : displayAddress(address, activeNetwork);
  },
  openCatalog: () => void navigation.openCatalog(),
  recents: () =>
    recents
      .list(recentScope(), activeNetwork.id)
      .slice(0, 5)
      .map((entry) => ({
        label: displayAddress(entry.address, activeNetwork),
        at: entry.at,
        open: () => openRecent(entry),
      })),
});

/** History is kept per wallet, like the rest of its data; signed out has its own. */
function recentScope(): string {
  return walletSession.active?.id ?? "signed-out";
}

/**
 * The people the Docklet's search offers beside apps, through the contact
 * picker's own directory, while a wallet is signed in. A person chosen there
 * is shown in Chat as New chat would show them, or told why Chat cannot; a
 * test user has no chat to show them in, so none is offered.
 */
const searchPeople = new SearchPeople({
  available: () => walletSession.active !== null && chatInbox.signedIn(),
  startProblem: () => chatScreen.startProblem(),
  contacts: () => {
    const contacts = walletContacts.contacts();
    const listed = new Set(
      contacts.map((contact) => contact.account.toLowerCase()),
    );
    return [
      ...contacts,
      ...chatInbox
        .roomPeople()
        .filter((person) => !listed.has(person.account.toLowerCase())),
    ];
  },
  blocked: blockedPeople,
  directory: () => usernameDirectory,
  changed: () => recentsMenu.refresh(),
  chose: (person) => void showPersonInChat(person),
});
/** Counts sessions, so a person chosen in one is not shown in the next. */
let searchSession = 0;
chatInbox.onChange((change) => {
  if (change.type === "session") {
    searchSession += 1;
    searchPeople.reset();
  }
  if (change.type === "session" || change.type === "providers")
    recentsMenu.refresh();
});

/** Show a person chosen in the Docklet's search in Chat: their conversation, or a request to them. */
async function showPersonInChat(person: PickedPerson): Promise<void> {
  const session = searchSession;
  addressInput.value = displayAddress(
    navigation.opened?.address ?? "",
    activeNetwork,
  );
  searchStatus.textContent = "";
  renderAddress();
  searchCard.close(false);
  hostNames.remember(person.account, person.name);
  await navigation.selectDestination("chat");
  if (session !== searchSession || navigation.destination !== "chat") return;
  if (chatTab === "inbox") showChatTab("chats");
  if (!chatScreen.startWith(person))
    log("Chat cannot start a conversation in this session.", true);
}

const recentsMenu = bindRecentsMenu({
  input: addressInput,
  list: recentsList,
  network: activeNetwork,
  people: searchPeople,
  typedApp: () => {
    try {
      const address = parseTypedAddress(addressInput.value, activeNetwork);
      return address.kind === "name" ? address.name : address.url.href;
    } catch {
      return null;
    }
  },
  openTyped: () => {
    openAddress();
    addressInput.blur();
  },
  entries: () => recents.list(recentScope(), activeNetwork.id),
  untouched: () =>
    addressInput.value.trim() ===
    displayAddress(navigation.opened?.address ?? "", activeNetwork),
  choose(entry) {
    // The entry brings its own product id, or none, so a replay never
    // inherits whatever id is entered now.
    addressInput.value = displayAddress(entry.address, activeNetwork);
    productIdInput.value = entry.entered ? entry.productId : "";
    renderAddress();
    openAddress();
    addressInput.blur();
  },
});

/**
 * Clear the open product's stored data for the signed-in wallet, after asking.
 * The product is closed first so it cannot write the data back, then opened
 * again as it was. Grants, allowances, other products and other wallets are
 * outside what is cleared.
 */
async function resetAppData(): Promise<void> {
  const wallet = walletSession.active;
  const current = navigation.product;
  if (wallet === null || current === null) return;
  const productId = current.productId;
  const confirmed = await ask({
    title: "Reset app data",
    fields: [
      { label: "Wallet", value: wallet.name },
      { label: "Product", value: productId },
      {
        label: "Clears",
        value:
          "This product's stored data for this wallet, as stored through the host. The page's own browser storage, grants, allowances, other products and other wallets are kept.",
      },
      {
        label: "Before you go on",
        value: "Close other tabs that use this wallet and this product.",
        warning: true,
      },
    ],
    choices: [
      { label: "Cancel", value: false, tone: "cancel" },
      { label: "Reset app data", value: true, tone: "destructive" },
    ],
    dismissed: false,
  });
  if (!confirmed) return;
  await serial(async (runtime) => {
    const how = navigation.opened;
    const product = navigation.product;
    if (
      walletSession.active?.id !== wallet.id ||
      product?.productId !== productId ||
      how === null
    ) {
      activity.record({
        category: "products",
        action: "Data reset skipped",
        subject: productId,
        note: "The open product or the wallet changed.",
      });
      return;
    }
    const url = product.url;
    const entered = navigation.entered;
    const scope = navigation.overrideOrigin ?? "";
    navigation.closeProduct();
    const removed = storage.clearProductData(productId);
    activity.record({
      category: "products",
      action: "Data reset",
      subject: productId,
      details: [
        { label: "Wallet", value: wallet.name },
        { label: "Stored values removed", value: String(removed) },
      ],
    });
    await navigation.openTarget(
      runtime,
      { ...how, url },
      productId,
      entered,
      scope,
    );
  });
}

resetDataButton.addEventListener("click", () => {
  void resetAppData();
});

addressForm.addEventListener("submit", (event) => {
  event.preventDefault();
  openAddress();
  addressInput.blur();
});

productForm.addEventListener("submit", (event) => {
  event.preventDefault();
  openAddress();
});

addressInput.addEventListener("input", () => {
  renderAddress();
  searchPeople.type(addressInput.value);
});
productAddress.addEventListener("input", () => {
  addressInput.value = productAddress.value;
  renderAddress();
});
addressInput.addEventListener("keydown", (event) => {
  if (event.key !== "Escape") return;
  event.stopPropagation();
  if (recentsMenu.close()) return;
  addressInput.value = displayAddress(
    navigation.opened?.address ?? "",
    activeNetwork,
  );
  searchStatus.textContent = "";
  renderAddress();
  searchCard.close(true);
});

productIdInput.addEventListener("input", renderEffectiveId);
productIdReset.addEventListener("click", () => {
  productIdInput.value = "";
  renderEffectiveId();
  productIdInput.focus();
});

for (const { destination: id, button } of dockButtons)
  button.addEventListener("click", () => void navigation.selectDestination(id));

closeButton.addEventListener("click", closeFront);

async function boot(): Promise<void> {
  const network = await connectNetwork(activeNetwork, runningBackends);
  const guarded = devSession?.guardNetwork(network) ?? network;
  // The core's and the host's People traffic are observed apart, labelled by who opened it.
  const observed = (source: TrafficSource): Network => ({
    ...guarded,
    chain: observeStatements(
      guarded.chain,
      guarded.genesis.people,
      source,
      statements.traffic,
    ),
  });
  const connection = observed("core");
  chainAccess = observed("host");
  peopleAccess = { chain: guarded.chain, genesis: guarded.genesis.people };
  const rule = currentPostingRule();
  log(
    rule.wait === "best"
      ? `Statement Store posts: an account posts once its claim's block has a child at the best block (${rule.why}); finality is checked behind each post.`
      : `Statement Store posts: an account posts once its allowance holds at the finalized head (${rule.why}).`,
  );
  activity.record({
    category: "network",
    action: "Chains connected",
    subject: activeNetwork.displayName,
    details: [
      { label: "Name suffix", value: connection.networkSuffix },
      ...CHAIN_ROLES.map((role) => ({
        label: CHAIN_LABELS[role],
        value:
          network.backends[role] === "rpc"
            ? `RPC ${activeNetwork.chains[role].rpc}`
            : "light client",
      })),
    ],
  });
  renderAccountStatus();
  const callbacks = createHostCallbacks({
    network: connection,
    storage,
    activity: activity.record,
    onAuthState: renderSession,
    navigate: (url) => navigation.navigateFromProduct(url),
    onNetworkAsked: (domains) => navigation.askDomains(domains),
    onPermissionDecided: refreshPermissionsSoon,
    preimage: gatewayPreimages(activeNetwork.dotns.contentGateway),
    chat: productChat.platform,
    pocket: pocketPlatform(connection.networkSuffix),
    contacts: hostContacts.platform,
  });
  const worker = new Worker(
    new URL("./platform/core-worker.ts", import.meta.url),
    {
      type: "module",
    },
  );
  runtime = await createWebWorkerPairingHostRuntime(worker, callbacks, {
    role: "signing",
    hostConfig: {
      host: { name: "TrUAPI Web Host", platform: "Web" },
      platform: { type: "browser", version: navigator.userAgent },
      people: { genesisHash: connection.genesis.people },
      bulletin: { genesisHash: connection.genesis.bulletin },
      assetHub: { genesisHash: connection.genesis.assetHub },
      // Required by the config type; a signing host never pairs.
      pairing: { deeplinkScheme: "polkadotapp" },
      networkSuffix: connection.networkSuffix,
    },
  });
  coreStatus.textContent = `Core ready on ${activeNetwork.displayName} · wire schema ${runtime.coreWireSchemaHash?.slice(0, 12) ?? "unknown"}`;
  coreState = "ready";
  renderShelf();
  activity.record({
    category: "host",
    action: "Core ready",
    details: [
      { label: "Network", value: activeNetwork.displayName },
      { label: "Wire schema", value: runtime.coreWireSchemaHash ?? "unknown" },
    ],
  });
  productWorkers.resume();
  renderVersions(runtime.coreWireSchemaHash);
  if (runtime.coreWireSchemaHash !== TRUAPI_WIRE_SCHEMA_HASH)
    activity.record({
      category: "host",
      action: "Wire schema mismatch",
      note: "The inspector groups frames without decoding them.",
      failed: true,
      details: [
        { label: "Core", value: runtime.coreWireSchemaHash ?? "none reported" },
        { label: "This page", value: TRUAPI_WIRE_SCHEMA_HASH },
      ],
    });
  renderWallets();

  await tabSession.resume();

  // Prefill only. The product id decides what the core trusts the page as,
  // so a link must not be able to open a product under one without the
  // user choosing Open.
  const params = new URLSearchParams(window.location.search);
  addressInput.value = params.get("product") ?? "";
  productIdInput.value = params.get("productId") ?? "";
  renderAddress();

  // The product this tab opened itself before a reload comes back with the
  // wallet. A link's `product` parameter never does.
  const last = navigation.readTabProduct();
  if (last && !params.has("product")) {
    addressInput.value = displayAddress(last.address, activeNetwork);
    const derived = effectiveProductIdFor(last.parsed, "").id;
    const entered = last.productId !== derived;
    if (entered) productIdInput.value = last.productId;
    renderAddress();
    navigation.restore(last.destination);
    renderProduct();
    if (walletSession.active)
      await serial((runtime) =>
        navigation.openResolved(runtime, last.parsed, last.productId, entered),
      );
  }
}

renderWallets();
renderProduct();
renderAccountStatus();
renderProductRecents();
boot().catch((error: unknown) => {
  coreStatus.textContent = `The core failed to start: ${String(error)}`;
  coreState = "failed";
  renderShelf();
  activity.record({
    category: "host",
    action: "Core failed to start",
    note: errorText(error),
    failed: true,
  });
});
