import { createWebWorkerPairingHostRuntime } from "@parity/truapi-host/web";
import type { WorkerPairingHostRuntime } from "@parity/truapi-host/web";
import type { AuthState } from "@parity/truapi-host";
import { TRUAPI_CODEC_VERSION, TRUAPI_WIRE_SCHEMA_HASH } from "@parity/truapi";
import { createInAppDebugger } from "@parity/truapi-debugger";
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
import { bindDockResize } from "../tools/dock-resize.js";
import { resolveSource } from "./products/dotns.js";
import { createFrameTap } from "../tools/inspector/frame-tap.js";
import { messagePortBridge } from "./chat/draft/contract.js";
import { ChatV2Host } from "./chat/draft/host.js";
import { LocalChatDemo, demoAllowed } from "../tools/demos/local-demo.js";
import { identityPortOver } from "./chat/draft/identity-bridge.js";
import {
  NOVA_WORKER_PRODUCT,
  NovaWorkerMode,
} from "../tools/demos/worker-mode.js";
import { sessionChatKeys } from "./chat/nova-session.js";
import { createHostCallbacks } from "./app/callbacks.js";
import { gatewayPreimages } from "./products/preimage.js";
import { connectNetwork, type Network } from "./platform/network.js";
import {
  NETWORKS,
  networkConfig,
  type NetworkConfig,
} from "./platform/network-config.js";
import { DEFAULT_NETWORK } from "./platform/network-scope.js";
import {
  chosenNetwork,
  rememberNetwork,
  tabProductKey,
  tabWalletKey,
  type NetworkChoice,
} from "./platform/network-choice.js";
import { ask, setPromptArea } from "./app/prompt.js";
import { RecentProducts } from "./app/recents.js";
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
import { StageLoading, documentsBeforeProduct } from "./products/loading.js";
import {
  DESTINATIONS,
  DESTINATION_LABELS,
  destinationOf,
  isProductDestination,
  loadAddresses,
  resolveNavigation,
  saveAddress,
  type Destination,
  type DockAddresses,
} from "./app/shell.js";
import { drawPocket } from "./pocket/screen.js";
import {
  cardKey,
  cardLaunchPath,
  pinnedCards,
  pocketPlatform,
} from "./pocket/pocket.js";
import { PocketFaces, type PocketCardKey } from "./pocket/faces.js";
import { bindProductCard } from "./pocket/card-view.js";
import { bulletinImageUrl } from "./products/render.js";
import { resolveWidget } from "./pocket/widget.js";
import { ChatScreen } from "./chat/screen.js";
import { bindWorkersMenu } from "./products/workers-menu.js";
import {
  NotificationStore,
  notificationsKey,
} from "./notifications/notifications.js";
import { NotificationsScreen } from "./notifications/screen.js";
import { ChatTabs, type ChatTab } from "./chat/tabs.js";
import { AppsShelf, type ShelfApp } from "./app/apps-shelf.js";
import { ChatInbox } from "./chat/inbox.js";
import {
  HostContacts,
  chooseContact,
  type PickedPerson,
} from "./contacts/contacts.js";
import { HostNames } from "./contacts/host-names.js";
import { blockedAccounts, chatRecordKey } from "./chat/nova/store.js";
import {
  exactUsernameDirectory,
  routeDirectory,
} from "./contacts/username-directory.js";
import { chainDirectory, lookupOwner } from "./chat/nova/directory.js";
import { bytesToHex, type HexString } from "@parity/truapi/scale";
import { accountProofs } from "./chat/account-proofs.js";
import { ProductChat, productChatKey } from "./chat/product-chat.js";
import { ProductWorkers } from "./products/product-workers.js";
import { resolveWorker } from "./products/worker-manifest.js";
import { mountChatDiagnostics } from "../tools/inspector/chat-diagnostics-view.js";
import { ChatDiagnostics } from "../tools/inspector/diagnostics.js";
import { openProduct, type OpenProduct } from "./products/product.js";
import {
  notOpenableText,
  productDetailsText,
  productStatusText,
  type Opened,
} from "./products/status.js";
import {
  hostBase,
  loaderUrl,
  mountScope,
  requireSecureHost,
} from "./products/loader/mount.js";
import { HostStorage, localStorageChanges } from "./platform/storage.js";
import { SerialQueue } from "./app/serial-queue.js";
import { versionSections, type VersionRow } from "./app/versions.js";
import { bindViewport } from "../tools/viewport.js";
import { SearchCard } from "./app/search-card.js";
import { bindDeviceMode } from "./app/device-mode.js";
import { SearchPeople } from "./contacts/search-people.js";
import {
  WalletPublicInfoStore,
  sessionUsernameFrom,
  usernameFrom,
  walletLabel,
} from "./app/wallet-label.js";
import { walletPanel, type WalletMode } from "./app/wallet-panel.js";
import { WalletStore, type Wallet } from "./platform/wallets.js";

const MAX_LOG_LINES = 500;
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
const walletNameInput = element<HTMLInputElement>("wallet-name");
const mnemonicInput = element<HTMLTextAreaElement>("mnemonic");
const importForm = element<HTMLFormElement>("import-form");
const importDetails = element<HTMLDetailsElement>("import");
const addWalletButton = element<HTMLButtonElement>("add-wallet");
const walletActions = element("wallet-actions");
const logList = element("log");
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
/** How the sandbox status of a product whose loader failed starts. */
const SANDBOX_FAILED = "Sandbox failed:";
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
const productSlot = element("product-slot");
/** The host screen a minimized product goes back to: the last one shown. */
let lastHost: "chat" | "pocket" = "chat";
const appChrome = element("app-chrome");
const appMinimize = element<HTMLButtonElement>("app-minimize");
const appClose = element<HTMLButtonElement>("app-close");
const appMore = element<HTMLButtonElement>("app-more");
const appMoreMenu = element("app-more-menu");
const pocketScreen = element("pocket-screen");
/** What Pocket says about a card it could not open. */
const pocketStatus = document.createElement("p");
pocketStatus.className = "pocket-status";
pocketStatus.setAttribute("role", "status");
const chatBadge = document.querySelector<HTMLElement>(
  '#dock [data-destination="chat"] [data-badge]',
);
const chatDiagnostics = new ChatDiagnostics();
/** Rooms that product workers keep, and the workers that keep them. */
const productChat = new ProductChat(localStorage);
/**
 * The local chat product demo, offered only by a dev server on this machine:
 * the synthetic chat product, run as one of the host's workers. A build
 * reads `import.meta.env.DEV` as false and leaves the demo's code out.
 */
const localChatDemo =
  import.meta.env.DEV && demoAllowed(true, window.location.hostname)
    ? new LocalChatDemo({
        base: BASE,
        origin: window.location.origin,
        port: window.location.port,
        changed: () => {
          if (!localChatDemo!.running())
            chatV2.forget(localChatDemo!.productId);
          productWorkers.defaultsChanged();
        },
      })
    : null;
/**
 * Where the wallet's Nova chat runs, offered only by a dev server on this
 * machine: the host (the default) or the Nova chat worker, a product worker
 * this dev server builds. The two never run at once. A build reads
 * `import.meta.env.DEV` as false and never creates it.
 */
const novaWorkerMode =
  import.meta.env.DEV && demoAllowed(true, window.location.hostname)
    ? new NovaWorkerMode({
        storage: localStorage,
        base: BASE,
        origin: window.location.origin,
        // The host's engine stops before the worker starts, and the worker
        // stops before the host's engine starts again.
        changed: () => {
          if (novaWorkerMode!.mode() === "host") {
            chatV2.forget(NOVA_WORKER_PRODUCT);
            productWorkers.defaultsChanged();
            chatInbox.syncHostChat();
          } else {
            chatInbox.syncHostChat();
            productWorkers.defaultsChanged();
          }
          engineSwitch?.render();
        },
        linkChanged: () => engineSwitch?.render(),
        blocked: (account) =>
          blockedPeople().some(
            (blocked) => blocked.toLowerCase() === account.toLowerCase(),
          ),
        log,
      })
    : null;
/**
 * The host end of the Chat Extension v2 draft bridge. It runs call media only
 * for the local demo, with the synthetic engine, which opens no device.
 */
const chatV2 = new ChatV2Host({
  media: (productId) => localChatDemo?.media(productId) ?? null,
  contactRooms: (productId) =>
    novaWorkerMode?.isChatProduct(productId) ?? false,
  log,
});
/** The host's own names for the people of a chat product's contact rooms. */
const hostNames = new HostNames();
/** The wallet and network the v2 declarations belong to. */
let chatV2Scope: string | null = null;
const workerFrames = document.createElement("div");
workerFrames.id = "worker-frames";
workerFrames.hidden = true;
document.body.append(workerFrames);
const productWorkers = new ProductWorkers({
  runtime: () => runtime,
  resolve: (name, network) =>
    localChatDemo?.resolves(name)
      ? localChatDemo.resolve()
      : novaWorkerMode?.resolves(name)
        ? novaWorkerMode.resolve()
        : resolveWorker(name, network),
  defaults: () => [
    ...(localChatDemo?.workerNames() ?? []),
    ...(novaWorkerMode?.workerNames() ?? []),
  ],
  container: workerFrames,
  base: BASE,
  storage: localStorage,
  tap: (channel) =>
    runtime === null
      ? undefined
      : createFrameTap(inspector, channel, runtime.coreWireSchemaHash),
  log,
  chatV2: {
    connect: (productId, port) =>
      chatV2.connect(productId, messagePortBridge(port)),
    disconnect: (productId) => chatV2.disconnect(productId),
  },
  ...(novaWorkerMode === null
    ? {}
    : {
        novaIdentity: {
          connect: (productId: string, port: MessagePort) =>
            novaWorkerMode.connect(productId, identityPortOver(port)),
          disconnect: (productId: string) =>
            novaWorkerMode.disconnect(productId),
        },
      }),
});
/** The faces of Pocket's cards, drawn by the workers of the products behind them. */
const pocketFaces = new PocketFaces({
  workers: productWorkers,
  storage: localStorage,
  log,
});
/** The chat providers of the signed-in session: Nova, product rooms, and chat products over the v2 draft bridge. */
const chatInbox = new ChatInbox({
  storage: localStorage,
  diagnostics: chatDiagnostics,
  rooms: {
    chat: productChat,
    channel: (productId) => productWorkers.provider(productId),
    workerState: (productId) => productWorkers.state(productId),
    onWorkersChange: (listener) => productWorkers.onChange(listener),
    archiveImage: (productId, path) =>
      productWorkers.archiveImage(productId, path),
    gateway: () => activeNetwork.dotns.contentGateway,
    log,
  },
  workerNames: () => productWorkers.names(),
  log,
  chatV2,
  hostChat: () => novaWorkerMode?.hostRuns() ?? true,
  // The Nova chat worker's rooms are named by the host, and it starts chats
  // with whom the user chose, through a reference only its identity bridge
  // resolves.
  chatV2Options: (productId) =>
    novaWorkerMode === null || productId !== NOVA_WORKER_PRODUCT
      ? {}
      : {
          names: (account) => hostNames.name(account),
          onNamesChange: (listener) => hostNames.onChange(listener),
          start: {
            reference: (account) => novaWorkerMode.startReference(account),
            blocked: () =>
              novaWorkerMode.isChatProduct(productId)
                ? null
                : "The Nova chat worker is not connected yet.",
          },
        },
});
/** Which of Chat's tabs shows: the conversations or the Inbox. */
let chatTab: ChatTab = "chats";
/**
 * The Inbox, a prototype the dev server offers on this machine only: Chat's
 * Inbox tab and the Apps shelf's Inbox open this one screen over this one
 * store. A build reads `import.meta.env.DEV` as false and leaves it out, with
 * both entries.
 */
const chatTabs =
  import.meta.env.DEV && demoAllowed(true, window.location.hostname)
    ? new ChatTabs(showChatTab)
    : null;
const notifications =
  chatTabs === null
    ? null
    : (() => {
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
  (pendingRequests) => {
    if (chatBadge === null) return;
    chatBadge.hidden = pendingRequests === 0;
    chatBadge.textContent = String(pendingRequests);
  },
  async () => {
    const person = await hostContacts.choose({ title: "New chat" }, () =>
      chatInbox.roomPeople(),
    );
    if (person !== null) hostNames.remember(person.account, person.name);
    return person;
  },
  chatTabs?.root,
);
bindWorkersMenu({
  form: element<HTMLFormElement>("worker-form"),
  input: element<HTMLInputElement>("worker-name"),
  add: element<HTMLButtonElement>("worker-add"),
  list: element<HTMLUListElement>("worker-list"),
  status: element("worker-status"),
  workers: productWorkers,
  signedIn: () => chatInbox.signedIn(),
  onSessionChange: (listener) =>
    chatInbox.onChange((change) => {
      if (change.type === "session") listener();
    }),
});
/**
 * The menu's Developer section: test users and sample data. Only a dev server
 * on this machine loads it, so a build has none of its code.
 */
const devSession =
  notifications === null
    ? null
    : (await import("../tools/demos/dev-tools.js")).startDevTools({
        network: activeNetwork,
        base: BASE,
        notifications: notifications.store,
        section: element("developer"),
        marker: element("test-user-marker"),
        stage: element("product-slot"),
        leaveWalletSession: () => {
          sessionStorage.removeItem(tabWalletKey(activeNetwork.id));
          sessionStorage.removeItem(tabProductKey(activeNetwork.id));
        },
        useStorage: (scope) => storage.useWallet(scope, activeNetwork.id),
        tap: (productId) =>
          runtime === null
            ? undefined
            : createFrameTap(inspector, productId, runtime.coreWireSchemaHash),
        showProduct: () => void selectDestination("browse"),
        pickContact: (asker): Promise<{ account: string } | null> =>
          hostContacts.choose(asker),
        log,
      });
notifications?.screen.setActions(devSession?.notificationActions ?? null);
/** The made-up user this tab acts as instead of a wallet, if any. */
const testUser = devSession?.testUser ?? null;
/**
 * The people the signed-in wallet blocked in Chat. The host's chat record
 * keeps them whichever engine runs, so the contact picker and the Nova chat
 * worker's start references read one list. A test user blocks no one.
 */
function blockedPeople(): HexString[] {
  const wallet = activeWallet;
  return testUser !== null || wallet === null
    ? []
    : blockedAccounts(localStorage, chatRecordKey(activeNetwork.id, wallet.id));
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
 * The contact picker, over the session's chat contacts or the test user's:
 * the one products open with `contacts.pick` and Chat's New chat opens.
 */
const hostContacts = new HostContacts({
  list: () => devSession?.contacts ?? chatInbox.contacts(),
  onChange: (listener) => chatInbox.onChange(listener),
  choose: (asker, contacts, blocked) =>
    chooseContact(asker, contacts, usernameDirectory, blocked),
  blocked: blockedPeople,
  onSessionChange: (listener) =>
    chatInbox.onChange((change) => {
      if (change.type === "session") listener();
    }),
  removed: () => runtime?.notifyContactsChanged(),
  log,
});
/** The Developer menu's chat engine switch, with the Nova chat worker's state. */
const engineSwitch =
  novaWorkerMode === null || devSession === null
    ? null
    : (await import("../tools/demos/engine-switch.js")).drawEngineSwitch({
        section: element("developer"),
        signedIn: () => chatInbox.signedIn(),
        mode: () => novaWorkerMode.mode(),
        setMode: (mode, options) => novaWorkerMode.setMode(mode, options),
        link: () => novaWorkerMode.link(),
        worker: () => productWorkers.state(NOVA_WORKER_PRODUCT),
        productId: NOVA_WORKER_PRODUCT,
      });
/** The Developer menu's chat demo control, beside the chat engine switch. */
const chatDemo =
  localChatDemo === null || devSession === null
    ? null
    : (await import("../tools/demos/chat-demo.js")).drawChatDemo({
        section: element("developer"),
        signedIn: () => chatInbox.signedIn(),
        demo: {
          productId: localChatDemo.productId,
          source: localChatDemo.source,
          running: () => localChatDemo.running(),
          start: () => localChatDemo.start(),
          stop: () => localChatDemo.stop(),
          restart: () => {
            localChatDemo.stop();
            localChatDemo.start();
          },
        },
      });
productWorkers.onChange(() => {
  engineSwitch?.render();
  chatDemo?.render();
});
chatInbox.onChange((change) => {
  if (change.type === "session") chatDemo?.render();
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
      if (isProductDestination(back)) void selectDestination(back);
      else if (held.has(key))
        void serial(async () => {
          bringApp(key);
        });
    },
    close: closeApp,
  },
);
notifications?.store.onChange(renderShelf);
const stageEmptyTitle = element("stage-empty-title");
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

const inspector = createInAppDebugger();
const inspectorMount = element("inspector-mount");
inspector.mount(inspectorMount);
const chatDiagnosticsView = mountChatDiagnostics(
  element("chat-diagnostics"),
  chatDiagnostics,
);
const inspectorNote = element("inspector-note");
const INSPECTOR_NOTES = {
  wire: "Frames stay in this tab and may carry sensitive payloads.",
  chat: "Chat events are not TrUAPI frames. Metadata only unless payload capture is on; kept in memory and cleared when the session changes.",
} as const;
const inspectorViewButtons = [
  ...document.querySelectorAll<HTMLButtonElement>("[data-inspector-view]"),
];
/** Show the TrUAPI traces or the chat events in the inspector. */
function showInspectorView(view: keyof typeof INSPECTOR_NOTES): void {
  for (const button of inspectorViewButtons)
    button.setAttribute(
      "aria-selected",
      String(button.dataset.inspectorView === view),
    );
  inspectorMount.hidden = view !== "wire";
  chatDiagnosticsView.show(view === "chat");
  inspectorNote.textContent = INSPECTOR_NOTES[view];
}
for (const button of inspectorViewButtons)
  button.addEventListener("click", () =>
    showInspectorView(
      button.dataset.inspectorView === "chat" ? "chat" : "wire",
    ),
  );
bindDockResize(element("devtools-resize"), element("devtools"));

bindViewport({
  select: element<HTMLSelectElement>("viewport"),
  scale: element("viewport-scale"),
  area: element("viewport-area"),
  frame: productFrame,
  fills: window.matchMedia("(max-width: 900px)"),
  custom: {
    group: element("viewport-custom"),
    width: element<HTMLInputElement>("viewport-width"),
    height: element<HTMLInputElement>("viewport-height"),
  },
  handle: element("viewport-handle"),
  note: element("viewport-note"),
  caption: element("viewport-caption"),
  presets: [
    ...document.querySelectorAll<HTMLButtonElement>(
      "#dev-preview [data-viewport]",
    ),
  ],
  frames: [
    ...document.querySelectorAll<HTMLButtonElement>(
      "#dev-preview [data-frame]",
    ),
  ],
  frameGroup: element("viewport-frames"),
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

function log(text: string): void {
  const line = document.createElement("li");
  line.textContent = `${new Date().toLocaleTimeString()} ${text}`;
  logList.append(line);
  while (logList.childElementCount > MAX_LOG_LINES)
    logList.firstElementChild?.remove();
  logList.scrollTop = logList.scrollHeight;
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
let activeWallet: Wallet | null = null;
let product: OpenProduct | null = null;
let opened: Opened | null = null;
/** Whether the open product's id was entered rather than derived. */
let openedWithOverride = false;
/** The origin the entered product id was last opened with, for the log. */
let overrideOrigin: string | null = null;
/** What the sandbox loader last said about the open product, until the page takes over. */
let sandboxStatus = "";

/**
 * The dock destination the product in front belongs to, or null for a product
 * a link opened. Choosing a destination sends the one in front behind it
 * instead of ending it, so Chat keeps running while Browse is open.
 */
let destination: Destination | null = testUser === null ? null : "browse";
let dockAddresses: DockAddresses = loadAddresses(
  localStorage,
  activeNetwork.id,
);

/** A product held behind the one in front, with the bookkeeping that belongs to it. */
interface Held {
  product: OpenProduct;
  opened: Opened | null;
  entered: boolean;
  origin: string | null;
  sandbox: string;
  domains: string[];
}
/**
 * The products running behind the one in front: a dock destination's under
 * the destination, and an app opened by a link or an address under
 * {@link appKey}, one per product, until it is closed.
 */
const held = new Map<string, Held>();
/** Where an app that belongs to no destination is held. */
const appKey = (productId: string): string => `app:${productId}`;

/**
 * Session changes and product opens run one at a time, in order. Opening a
 * product awaits the core, so without this a second open or a wallet switch
 * could start before the first open has a handle to dispose, and leave a
 * product connected under a session it was not opened for.
 */
const queue = new SerialQueue(renderWallets, (error) => log(String(error)));

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
  const panel = walletPanel({
    saved: savedCount,
    activeId: activeWallet?.id ?? null,
    selectedId: wallets.has(walletSelect.value) ? walletSelect.value : "",
  });
  walletSelect.hidden = !panel.showPicker;
  signInButton.hidden = !panel.showSignIn;
  signOutButton.hidden = !panel.showSignOut;
  forgetButton.hidden = !panel.showForget;
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
  const selected = walletSelect.value || activeWallet?.id || "";
  const saved = wallets.list();
  const problem = wallets.problem();
  walletSelect.replaceChildren(
    ...saved.map(
      (wallet) =>
        new Option(
          walletLabel(wallet, walletInfo.get(wallet.id, activeNetwork.id)),
          wallet.id,
        ),
    ),
  );
  if (saved.length === 0 && problem !== null) {
    const notice = new Option(problem, "", false, true);
    notice.disabled = true;
    walletSelect.append(notice);
  }
  if (problem !== loggedWalletProblem && problem !== null) log(problem);
  loggedWalletProblem = problem;
  if (saved.some((wallet) => wallet.id === selected))
    walletSelect.value = selected;
  savedCount = saved.length;
  renderControls();
}

function renderControls(): void {
  const ready = runtime !== null && !queue.busy;
  signInButton.disabled =
    !ready || walletSelect.value === activeWallet?.id || testUser !== null;
  signOutButton.disabled = !ready || activeWallet === null;
  forgetButton.disabled = !ready || !wallets.has(walletSelect.value);
  addWalletButton.disabled = !ready || testUser !== null;
  openButton.disabled = !ready;
  addressGo.disabled = !ready;
  closeButton.disabled = !ready || product === null;
  resetDataButton.disabled =
    !ready || product === null || activeWallet === null;
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
    typed !== completeAddress(opened?.address ?? "", activeNetwork);
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
      chip.addEventListener("click", () => {
        addressInput.value = displayAddress(entry.address, activeNetwork);
        productIdInput.value = entry.entered ? entry.productId : "";
        renderAddress();
        openAddress();
      });
      item.append(chip);
      return item;
    }),
  );
  productRecents.hidden = entries.length === 0;
}

/** A message for an error thrown here, without the `Error:` prefix. */
function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function renderProduct(): void {
  document.body.dataset.product = product ? "open" : "closed";
  addressId.hidden = product === null;
  addressId.textContent = product ? `as ${product.productId}` : "";
  const basis = opened?.via === "name" ? "the name" : "the address";
  addressId.title = product
    ? `Product id ${product.productId}, ${openedWithOverride ? "entered by you" : `derived from ${basis}`}. Not verified.`
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
  const ready = runtime !== null && !queue.busy;
  for (const { destination: id, button } of dockButtons) {
    button.setAttribute("aria-current", String(destination === id));
    button.disabled = !ready;
    if (!isProductDestination(id)) {
      button.dataset.live = "false";
      button.title =
        id === "chat"
          ? notifications === null
            ? "Chat: this wallet's conversations, kept by the host"
            : "Chat: this wallet's conversations, and the Inbox of requests from products"
          : "Pocket: this wallet's identity and balance";
      continue;
    }
    button.dataset.live = String(
      held.has(id) || (destination === id && product !== null),
    );
    button.title =
      dockAddresses[id] === ""
        ? `${DESTINATION_LABELS[id]}: no product yet. Open one with the search button.`
        : `${DESTINATION_LABELS[id]}: ${dockAddresses[id]}`;
  }
  const bare =
    isProductDestination(destination) &&
    product === null &&
    dockAddresses[destination] === ""
      ? destination
      : null;
  stageEmptyTitle.textContent =
    bare === null
      ? "No product open"
      : `${DESTINATION_LABELS[bare]} has no product yet`;
  stageEmptyNote.textContent =
    bare === null
      ? "Pick Chat, Pocket or Browse, or open a product with the search button."
      : "Open a name or URL with the search button. The host remembers it for this tab.";
  renderShelf();
}

/** When each open product was opened, so the shelf keeps them in that order. */
const openedOrder = new WeakMap<OpenProduct, number>();
let openings = 0;

/** The products open in this tab, in the order they were opened, the one in front marked. */
function openApps(): ShelfApp[] {
  const app = (
    key: string,
    open: OpenProduct,
    how: Opened | null,
    front: boolean,
  ): ShelfApp & { instance: OpenProduct } => ({
    instance: open,
    key,
    // A name reads as itself; a URL product by its id, such as its host.
    name:
      how?.via === "name"
        ? displayAddress(how.address, activeNetwork)
        : open.productId,
    productId: open.productId,
    front,
  });
  const order = (open: OpenProduct) => openedOrder.get(open) ?? 0;
  return [
    ...(product === null ? [] : [app("front", product, opened, true)]),
    ...[...held].map(([key, entry]) =>
      app(key, entry.product, entry.opened, false),
    ),
  ]
    .sort((left, right) => order(left.instance) - order(right.instance))
    .map(({ instance: _instance, ...shown }) => shown);
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
  const store = notifications?.store ?? null;
  appsShelf.render({
    apps: openApps(),
    inbox:
      store === null
        ? null
        : {
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

/** Close a product from the Apps shelf: the one in front, or one running behind. */
function closeApp(key: string): void {
  void serial(async () => {
    if (key === "front") {
      if (product === null) return;
      const name = product.productId;
      closeProduct();
      sessionStorage.removeItem(tabProductKey(activeNetwork.id));
      log(`closed ${name}`);
      return;
    }
    const entry = held.get(key);
    if (entry === undefined) return;
    entry.product.dispose();
    held.delete(key);
    const back = DESTINATIONS.find((known) => known === key) ?? null;
    log(
      `closed ${entry.product.productId}, which ran behind${isProductDestination(back) ? ` ${DESTINATION_LABELS[back]}` : ""}`,
    );
    renderProduct();
  });
}

/** Chat's tab bar: show the conversations or the Inbox, in place. */
function showChatTab(tab: ChatTab): void {
  chatTab = tab;
  chatTabs?.render(chatTab, notifications?.store.unseen() ?? 0);
  renderHostScreens();
  chatTabs?.focus(tab);
}

/** The Apps shelf's Inbox: Chat, at its Inbox tab. */
function openInbox(): void {
  if (notifications === null) return;
  chatTab = "inbox";
  void selectDestination("chat").then(() => {
    if (destination !== "chat") return;
    showChatTab("inbox");
  });
}

/**
 * Show or hide the screens the host draws itself. Pocket is drawn once, the
 * first time it is shown; Chat draws itself from its engine.
 */
function renderHostScreens(): void {
  const surface =
    destination === "chat"
      ? chatTab === "inbox" && notifications !== null
        ? "inbox"
        : "chat"
      : destination === "pocket"
        ? destination
        : testUser !== null && product === null
          ? "test-product"
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
            open: () => void openPocketCard(key),
            log,
          });
        },
      }),
      pocketStatus,
    );
  pocketFaces.setShown(surface === "pocket");
  chatScreen.show(surface === "chat");
  notifications?.screen.show(surface === "inbox");
  devSession?.showProduct(surface === "test-product");
  if (destination === "chat" || destination === "pocket")
    lastHost = destination;
  renderAppChrome(surface === "product" && product !== null);
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
  const holds = holdKey() !== null;
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
function closeFront(): void {
  void serial(async () => {
    closeProduct();
    sessionStorage.removeItem(tabProductKey(activeNetwork.id));
    log("closed the product");
  });
}

appMinimize.addEventListener("click", () => void selectDestination(lastHost));
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

/** Give the Chat screen the signed-in wallet's session, or none. */
function syncChatSession(): void {
  const access = chainAccess;
  const wallet = activeWallet;
  if (lastAuthState.tag !== "Connected" || access === null || wallet === null) {
    pocketFaces.setSession(null);
    productWorkers.setSession(null);
    productChat.setScope(null);
    notifications?.store.setScope(devSession?.notificationsScope ?? null);
    localChatDemo?.stop();
    novaWorkerMode?.setSession(null);
    hostNames.setSession(null);
    if (chatV2Scope !== null) chatV2.reset();
    chatV2Scope = null;
    chatInbox.setSession(null);
    engineSwitch?.render();
    return;
  }
  const { identityAccountId, chatPublicKey, fullUsername, liteUsername } =
    lastAuthState.value;
  hostNames.setSession({
    scope: `${activeNetwork.id}:${wallet.id}`,
    record: () =>
      localStorage.getItem(chatRecordKey(activeNetwork.id, wallet.id)),
    directory: () => chainDirectory(access.chain, access.genesis),
  });
  // First, so the workers and the inbox below see this session's engine choice.
  const restartNova = novaWorkerMode?.setSession({
    network: activeNetwork.id,
    walletId: wallet.id,
    identity: `${identityAccountId ?? ""}:${chatPublicKey ?? ""}`,
    keys: () =>
      sessionChatKeys({
        entropy: wallet.entropy,
        networkSuffix: access.networkSuffix,
        identityAccountId: identityAccountId ?? undefined,
        chatPublicKey: chatPublicKey ?? undefined,
      }),
  });
  const scope = productChatKey(activeNetwork.id, wallet.id);
  if (scope !== chatV2Scope) {
    // The demo belongs to the session it was started in.
    localChatDemo?.stop();
    chatV2.reset();
  }
  chatV2Scope = scope;
  productChat.setScope(scope);
  notifications?.store.setScope(notificationsKey(activeNetwork.id, wallet.id));
  productWorkers.setSession({ walletId: wallet.id, network: activeNetwork });
  pocketFaces.setSession({
    walletId: wallet.id,
    networkId: activeNetwork.id,
    cards: pinnedCards(activeNetwork.networkSuffix),
  });
  chatInbox.setSession({
    walletId: wallet.id,
    entropy: wallet.entropy,
    network: activeNetwork.id,
    networkSuffix: access.networkSuffix,
    identityAccountId: identityAccountId ?? undefined,
    chatPublicKey: chatPublicKey ?? undefined,
    username: fullUsername ?? liteUsername ?? undefined,
    chain: access.chain,
    genesis: access.genesis,
    identityBackend: activeNetwork.identityBackend,
    proofs: async (productId) => {
      if (runtime === null) throw new Error("The core is not running.");
      return accountProofs(runtime, productId);
    },
  });
  if (restartNova === true) {
    // Its frame holds the link of the identity that ended; a new frame asks again.
    chatV2.forget(NOVA_WORKER_PRODUCT);
    void productWorkers.restart(NOVA_WORKER_PRODUCT);
  }
  engineSwitch?.render();
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
/** The chain and People genesis the status reads from, once the network is up. */
let chainAccess: Pick<Network, "chain" | "genesis" | "networkSuffix"> | null =
  null;
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
/** Domains the core asked about for the open product, in this tab. */
let askedDomains: string[] = [];

function renderAccountStatus(): void {
  accountStatusList.replaceChildren(
    ...versionLines(
      accountStatusRows(
        lastAuthState,
        {
          networkName: activeNetwork.displayName,
          networkSuffix:
            chainAccess?.networkSuffix ?? activeNetwork.networkSuffix,
          walletName: activeWallet?.name ?? null,
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
  const productId = product?.productId;
  const ready = runtime;
  if (productId === undefined || ready === null) {
    permissions = { state: "none-open" };
  } else if (lastAuthState.tag !== "Connected") {
    permissions = { state: "signed-out" };
  } else {
    permissions = { state: "loading" };
    renderAccountStatus();
    try {
      const { capabilities, networks } = queriesFor(askedDomains);
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
    productChain.productId !== product?.productId
  ) {
    productChainRun += 1;
    productChain = { state: "idle" };
  }
  void refreshPermissions();
  const productId = product?.productId;
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
  const productId = product?.productId;
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
    log(`Product account: ${errorText(error)}`);
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
    if (found !== undefined && activeWallet !== null) {
      walletInfo.update(
        activeWallet.id,
        { publicKey, username: found },
        activeNetwork.id,
      );
      renderWallets();
    }
  } catch (error) {
    if (run !== chainRun) return;
    chainState = { state: "error", message: errorText(error) };
    log(`People chain status: ${errorText(error)}`);
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
  const wallet = activeWallet;
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
  void changeSession(async (current) => {
    if (activeWallet !== wallet) return;
    await current.activateLocalSession(wallet.entropy, found ?? undefined);
    log(
      found === null
        ? "session username removed: the People chain has no record"
        : `session username set to ${found} from the People chain`,
    );
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
    log(`Session username: ${errorText(error)}`);
  }
}

/** Forget the chain state when the account changes, and read it again for the new one. */
function trackChainAccount(state: AuthState): void {
  // A test user signs in with a public test phrase; the host reads nothing on chain for it.
  const key =
    state.tag === "Connected" && testUser === null
      ? state.value.publicKey
      : null;
  if (key === chainFor) return;
  chainFor = key;
  chainRun += 1;
  chainState = { state: "idle" };
  productChainRun += 1;
  productChain = { state: "idle" };
  void refreshProductAllowance();
  if (key === null) return;
  if (accountStatusFold.open) void refreshChainStatus();
  else void syncSessionUsername();
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
  if (state.tag === "Connected" && activeWallet !== null) {
    // Public facts only, kept so the picker can tell wallets apart signed out.
    const { publicKey, fullUsername, liteUsername } = state.value;
    walletInfo.update(
      activeWallet.id,
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
      const { publicKey, identityAccountId, fullUsername, liteUsername } =
        state.value;
      const name =
        fullUsername ??
        liteUsername ??
        activeWallet?.name ??
        testUser?.name ??
        "wallet";
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

/**
 * Run a session change with the product closed.
 *
 * A product runtime must not outlive the session it was opened under, so the
 * product is closed first and opened again once the new session is in place.
 */
function changeSession(
  change: (runtime: WorkerPairingHostRuntime) => Promise<void>,
): Promise<void> {
  return serial(async (runtime) => {
    const reopen = product;
    const how = opened;
    const entered = openedWithOverride;
    const scope = overrideOrigin ?? "";
    closeHeld();
    closeProduct();
    try {
      await change(runtime);
    } finally {
      // A card's Widget is opened again as a Widget, not as the product's App.
      if (reopen && how)
        await openTarget(
          runtime,
          { ...how, url: reopen.url },
          reopen.productId,
          entered,
          scope,
          reopen.executionKind === "Widget" ? "Widget" : "App",
        );
    }
  });
}

function signIn(wallet: Wallet): Promise<void> {
  return changeSession(async (runtime) => {
    if (activeWallet) await signOutOf(runtime);
    storage.useWallet(wallet.id, activeNetwork.id);
    activeWallet = wallet;
    try {
      await runtime.activateLocalSession(
        wallet.entropy,
        walletInfo.get(wallet.id, activeNetwork.id)?.sessionUsername,
      );
    } catch (error) {
      storage.useWallet(null, activeNetwork.id);
      activeWallet = null;
      throw new Error(
        `Could not sign in with ${wallet.name}: ${String(error)}`,
      );
    }
    sessionStorage.setItem(tabWalletKey(activeNetwork.id), wallet.id);
    log(`signed in with ${wallet.name}`);
  });
}

async function signOutOf(runtime: WorkerPairingHostRuntime): Promise<void> {
  await runtime.disconnectSession();
  storage.useWallet(null, activeNetwork.id);
  sessionStorage.removeItem(tabWalletKey(activeNetwork.id));
  log(`signed out of ${activeWallet?.name ?? "wallet"}`);
  activeWallet = null;
}

function closeProduct(): void {
  loading.clear();
  askedDomains = [];
  product?.dispose();
  product = null;
  opened = null;
  sandboxStatus = "";
  renderProduct();
}

/** End every product held behind the one in front. They belong to the session that opened them. */
function closeHeld(): void {
  for (const entry of held.values()) entry.product.dispose();
  held.clear();
}

/**
 * Where the product in front runs behind: its destination, or its own key as
 * an app. Null when it cannot run behind, as a Pocket card's Widget cannot.
 */
function holdKey(): string | null {
  if (product === null) return null;
  if (isProductDestination(destination)) return destination;
  return product.executionKind === "App" ? appKey(product.productId) : null;
}

/**
 * Send the product in front behind the dock, still running, or end it when it
 * cannot run behind. Either way nothing is in front afterwards.
 */
function leaveFront(): void {
  const key = holdKey();
  if (key === null || product === null) {
    closeProduct();
    return;
  }
  loading.clear();
  product.setVisible(false);
  held.set(key, {
    product,
    opened,
    entered: openedWithOverride,
    origin: overrideOrigin,
    sandbox: sandboxStatus,
    domains: askedDomains,
  });
  product = null;
  opened = null;
  sandboxStatus = "";
  askedDomains = [];
  openedWithOverride = false;
  overrideOrigin = null;
  renderProduct();
}

/** Bring a held product back in front exactly as it was left. */
function resumeFront(back: Held): void {
  product = back.product;
  opened = back.opened;
  openedWithOverride = back.entered;
  overrideOrigin = back.origin;
  sandboxStatus = back.sandbox;
  askedDomains = back.domains;
  product.setVisible(true);
  addressInput.value = displayAddress(opened?.address ?? "", activeNetwork);
  rememberFront();
  renderProduct();
  void refreshPermissions();
}

/** Keep this tab's way back to the product in front across a reload. */
function rememberFront(): void {
  if (product === null || opened === null) return;
  sessionStorage.setItem(
    tabProductKey(activeNetwork.id),
    JSON.stringify({
      address: opened.address,
      productId: product.productId,
      destination,
    }),
  );
}

/**
 * Put a dock destination in front: resume it if it is running behind, else
 * open its address. The product that was in front keeps running behind its own
 * destination.
 */
function selectDestination(next: Destination): Promise<void> {
  return serial(async (runtime) => {
    if (
      destination === next &&
      (product !== null || !isProductDestination(next))
    )
      return;
    if (destination !== next) leaveFront();
    destination = next;
    if (!isProductDestination(next) || testUser !== null) {
      // The address bar names a product, and a host screen is not one.
      addressInput.value = "";
      renderAddress();
      renderProduct();
      return;
    }
    const back = held.get(next);
    if (back !== undefined) {
      held.delete(next);
      resumeFront(back);
      return;
    }
    renderProduct();
    const address = dockAddresses[next];
    if (address === "") return;
    await openSaved(runtime, address);
  });
}

/** Open a saved address as it was opened before: the name or URL, with the id it derives. */
async function openSaved(
  runtime: WorkerPairingHostRuntime,
  address: string,
): Promise<void> {
  let parsed: Address;
  try {
    parsed = parseTypedAddress(address, activeNetwork);
  } catch (error) {
    productStatus.textContent = errorText(error);
    log(productStatus.textContent);
    return;
  }
  const { id } = effectiveProductIdFor(parsed, "");
  await openResolved(runtime, parsed, id, false);
}

/**
 * A product asked the host to open `url`. A name of this network opens in
 * front of the destination the request came from, which keeps running behind
 * it. A destination's own product is resumed instead of opened twice.
 */
function navigateFromProduct(url: string): Promise<void> {
  const where = resolveNavigation(url, activeNetwork);
  if (where.kind === "external") {
    window.open(where.url, "_blank", "noopener,noreferrer");
    return Promise.resolve();
  }
  if (where.kind === "refused") {
    log(`navigation refused: ${where.reason}`);
    return Promise.reject(new Error(where.reason));
  }
  // The host has taken the request once it is queued. What it opens is
  // reported in the menu and the log, not back to the product.
  const known = destinationOf(where.address, dockAddresses, activeNetwork);
  if (known !== null) void selectDestination(known);
  else
    void serial(async (runtime) => {
      let parsed: Address;
      try {
        parsed = parseTypedAddress(where.address, activeNetwork);
      } catch (error) {
        productStatus.textContent = errorText(error);
        log(productStatus.textContent);
        return;
      }
      const { id } = effectiveProductIdFor(parsed, "");
      // An app already open comes to the front as it was, as on iOS.
      if (destination === null && holdKey() === appKey(id)) return;
      if (bringApp(appKey(id))) return;
      leaveFront();
      destination = null;
      renderProduct();
      await openResolved(runtime, parsed, id, false);
    });
  return Promise.resolve();
}

/**
 * Bring the app held under `key` to the front, sending the one in front
 * behind. False when no app is held there. Runs inside {@link serial}.
 */
function bringApp(key: string): boolean {
  const back = held.get(key);
  if (back === undefined) return false;
  leaveFront();
  held.delete(key);
  destination = null;
  resumeFront(back);
  return true;
}

/** Where a navigation ends up and how it got there. */
interface Target extends Opened {
  url: URL;
}

/**
 * Turn what the bar holds into a URL to embed.
 *
 * A URL is used as typed. A name is looked up on Asset Hub, and its content is
 * loaded by the sandbox loader, a static page of this host, which mounts it
 * under this host's own path for the active wallet and this product.
 */
async function resolveTarget(
  address: Address,
  productId: string,
): Promise<Target> {
  if (address.kind === "url")
    return { address: address.url.href, via: "url", url: address.url };
  const shown = address.suffix
    ? `${address.name}/${address.suffix}`
    : address.name;
  requireSecureHost(window.isSecureContext, window.location.origin);
  productStatus.textContent = `Resolving ${address.name} on ${activeNetwork.displayName} Asset Hub…`;
  let choice: Awaited<ReturnType<typeof resolveSource>>;
  try {
    choice = await resolveSource(address.name, activeNetwork);
  } catch (error) {
    throw new Error(
      `Could not read ${address.name}: ${errorText(error)}. Check the connection, or enter a URL.`,
    );
  }
  const { source, skipped } = choice;
  if (source === null) throw new Error(notOpenableText(address.name, skipped));
  const scope = mountScope(BASE, {
    walletId: activeWallet?.id ?? "signed-out",
    productId,
    cid: source.cid,
    network: activeNetwork.id,
  });
  return {
    address: shown,
    via: "name",
    cid: source.cid,
    kind: source.kind,
    mount: scope,
    record: source.record,
    skipped,
    url: loaderUrl({
      base: BASE,
      origin: window.location.origin,
      scope,
      gateway: activeNetwork.dotns.contentGateway,
      kind: source.kind,
      start: `/${address.suffix}`,
    }),
  };
}

/** The scope an entered product id was used with, for the log. */
function scopeOf(address: Address): string {
  return address.kind === "url" ? address.url.origin : address.name;
}

/**
 * Embed an already resolved target, closing the current product first. The
 * bookkeeping for the bar and the entered id changes only once the product is
 * open.
 */
async function openTarget(
  runtime: WorkerPairingHostRuntime,
  target: Target,
  productId: string,
  entered: boolean,
  scope: string,
  executionKind: "App" | "Widget" = "App",
): Promise<void> {
  closeProduct();
  const op = loading.begin("Loading…");
  sandboxStatus = "";
  // A product sent behind the dock while it loads must not write to the screen.
  let handle: OpenProduct | null = null;
  const inFront = (): boolean => handle === null || handle === product;
  try {
    handle = product = await openProduct(
      runtime,
      target.url,
      productId,
      productSlot,
      {
        mounted: target.via === "name",
        executionKind,
        tap: createFrameTap(inspector, productId, runtime.coreWireSchemaHash),
        onFrameLoad(count) {
          if (count < documentsBeforeProduct(target.via)) return;
          loading.finish(op);
          if (!inFront()) return;
          sandboxStatus = "";
          renderProduct();
        },
        onSandboxStatus(state, detail) {
          if (state === "error") {
            loading.finish(op);
            log(`sandbox could not start the product: ${detail}`);
          }
          if (!inFront()) return;
          sandboxStatus = `${state === "error" ? SANDBOX_FAILED : "Sandbox:"} ${detail}`;
          renderProduct();
        },
      },
    );
    openings += 1;
    openedOrder.set(handle, openings);
    opened = {
      address: target.address,
      via: target.via,
      cid: target.cid,
      kind: target.kind,
      mount: target.mount,
      record: target.record,
      skipped: target.skipped,
    };
    openedWithOverride = entered;
    askedDomains = [];
    overrideOrigin = entered ? scope : null;
    addressInput.value = displayAddress(target.address, activeNetwork);
    renderProduct();
    // A Widget opens only from its card: the address, typed or reloaded,
    // would open the product's App instead.
    if (executionKind === "App") {
      recents.record(
        recentScope(),
        { address: target.address, productId, entered },
        activeNetwork.id,
      );
      recentsMenu.refresh();
      renderProductRecents();
      rememberFront();
      if (isProductDestination(destination))
        dockAddresses = saveAddress(
          localStorage,
          activeNetwork.id,
          destination,
          target.address,
        );
    }
    renderProduct();
    log(
      target.via === "name"
        ? `opened ${target.address} as ${productId} (${executionKind}) from ${target.url.href}`
        : `opened ${target.url.href} as ${productId}`,
    );
  } catch (error) {
    loading.finish(op);
    productStatus.textContent = `Could not open ${target.url.href} as ${productId}: ${errorText(error)}`;
    log(productStatus.textContent);
  }
}

/**
 * Open a Pocket card: its product's Widget, at the card named in the query,
 * in front of Pocket. The Widget runs as the product, so it shares the
 * worker's and the app's storage, accounts and grants. Which content runs is
 * `findWidget`'s rule; Pocket stays in front, and says why, when there is
 * none to run. Choosing Pocket in the dock ends the Widget.
 */
function openPocketCard(key: PocketCardKey): Promise<void> {
  return serial(async (runtime) => {
    pocketStatus.textContent = `Looking up ${key.productId}'s widget…`;
    let widget: Awaited<ReturnType<typeof resolveWidget>>;
    try {
      requireSecureHost(window.isSecureContext, window.location.origin);
      widget = await resolveWidget(key.productId, activeNetwork);
    } catch (error) {
      pocketStatus.textContent = errorText(error);
      log(`pocket: ${key.cardId} not opened: ${errorText(error)}`);
      return;
    }
    if (destination !== "pocket") {
      pocketStatus.textContent = "";
      return;
    }
    pocketStatus.textContent = "";
    if (widget.fromBaseName)
      log(
        `pocket: ${key.productId} publishes no widget; its base name's content runs as the Widget, as the iOS host serves it`,
      );
    leaveFront();
    destination = null;
    sessionStorage.removeItem(tabProductKey(activeNetwork.id));
    const start = cardLaunchPath(key.cardId);
    const scope = mountScope(BASE, {
      walletId: activeWallet?.id ?? "signed-out",
      productId: key.productId,
      cid: widget.cid,
      network: activeNetwork.id,
    });
    await openTarget(
      runtime,
      {
        address: `${key.productId}${start}`,
        via: "name",
        cid: widget.cid,
        kind: widget.kind,
        mount: scope,
        record: widget.record,
        skipped: [],
        url: loaderUrl({
          base: BASE,
          origin: window.location.origin,
          scope,
          gateway: activeNetwork.dotns.contentGateway,
          kind: widget.kind,
          start,
        }),
      },
      key.productId,
      false,
      key.productId,
      "Widget",
    );
  });
}

/**
 * Resolve `address` and open it as `productId`. The current product stays
 * until resolution succeeds, so a lookup that fails changes nothing.
 */
async function openResolved(
  runtime: WorkerPairingHostRuntime,
  address: Address,
  productId: string,
  entered: boolean,
): Promise<void> {
  let target: Target;
  const op = loading.begin(
    address.kind === "name" ? "Looking up name…" : "Loading…",
  );
  try {
    target = await resolveTarget(address, productId);
  } catch (error) {
    loading.finish(op);
    productStatus.textContent = errorText(error);
    log(productStatus.textContent);
    return;
  }
  await openTarget(runtime, target, productId, entered, scopeOf(address));
}

signInButton.addEventListener("click", () => {
  const wallet = wallets.find(walletSelect.value);
  if (wallet) void signIn(wallet);
});

signOutButton.addEventListener("click", () => {
  void changeSession(signOutOf);
});

forgetButton.addEventListener("click", () => {
  const wallet = wallets.find(walletSelect.value);
  recents.forget(walletSelect.value);
  walletInfo.forget(walletSelect.value);
  wallets.forget(walletSelect.value);
  log(`forgot ${wallet?.name ?? "the wallet"}`);
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
    void signIn(wallet);
  } catch (error) {
    log(String(error));
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
    log(productStatus.textContent);
    return;
  }
  searchStatus.textContent = "";
  searchCard.close(false);
  const back = destinationOf(addressInput.value, dockAddresses, activeNetwork);
  if (back !== null && back !== destination && held.has(back)) {
    void selectDestination(back);
    chrome.dismissCovering();
    return;
  }
  const { id: productId, source } = effectiveProductIdFor(
    address,
    productIdInput.value,
  );
  const entered = source === "entered";
  if (entered && overrideOrigin !== null && overrideOrigin !== scopeOf(address))
    log(`product id ${productId} now applies to ${scopeOf(address)}`);
  void serial(async (runtime) => {
    const before = opened;
    // An address typed over a host screen or an app opens as an app of its
    // own; one already running behind comes to the front instead.
    if (!isProductDestination(destination)) {
      if (holdKey() !== appKey(productId)) {
        if (bringApp(appKey(productId))) return;
        leaveFront();
      }
      destination = null;
    }
    await openResolved(runtime, address, productId, entered);
    // It did not open: say why where the address was typed.
    if (opened === null || opened === before) {
      searchStatus.textContent = productStatus.textContent;
      searchCard.open();
    }
  });
  chrome.dismissCovering();
}

const recents = new RecentProducts(localStorage);

/** History is kept per wallet, like the rest of its data; signed out has its own. */
function recentScope(): string {
  return activeWallet?.id ?? "signed-out";
}

/**
 * The people the Docklet's search offers beside apps, through the contact
 * picker's own directory, while a wallet is signed in. A person chosen there
 * is shown in Chat as New chat would show them, or told why Chat cannot; a
 * test user has no chat to show them in, so none is offered.
 */
const searchPeople = new SearchPeople({
  available: () => testUser === null && chatInbox.signedIn(),
  startProblem: () => chatScreen.startProblem(),
  contacts: () => {
    const contacts = chatInbox.contacts();
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
  addressInput.value = displayAddress(opened?.address ?? "", activeNetwork);
  searchStatus.textContent = "";
  renderAddress();
  searchCard.close(false);
  hostNames.remember(person.account, person.name);
  await selectDestination("chat");
  if (session !== searchSession || destination !== "chat") return;
  if (chatTab === "inbox") showChatTab("chats");
  if (!chatScreen.startWith(person))
    log("Chat cannot start a conversation in this session.");
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
    displayAddress(opened?.address ?? "", activeNetwork),
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
  const wallet = activeWallet;
  const current = product;
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
    const how = opened;
    if (
      activeWallet?.id !== wallet.id ||
      product?.productId !== productId ||
      how === null
    ) {
      log("reset skipped: the open product or the wallet changed");
      return;
    }
    const url = product.url;
    const entered = openedWithOverride;
    const scope = overrideOrigin ?? "";
    closeProduct();
    const removed = storage.clearProductData(productId);
    log(
      `reset app data of ${productId} for ${wallet.name}: ${removed} stored value${removed === 1 ? "" : "s"} removed`,
    );
    await openTarget(runtime, { ...how, url }, productId, entered, scope);
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
  addressInput.value = displayAddress(opened?.address ?? "", activeNetwork);
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
  button.addEventListener("click", () => void selectDestination(id));

closeButton.addEventListener("click", closeFront);

async function boot(): Promise<void> {
  const connection = await connectNetwork(activeNetwork);
  chainAccess = connection;
  renderAccountStatus();
  const callbacks = createHostCallbacks({
    network: connection,
    storage,
    log,
    onAuthState: renderSession,
    navigate: navigateFromProduct,
    onNetworkAsked(domains) {
      askedDomains = [...new Set([...askedDomains, ...domains])];
    },
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
  log("core ready");
  productWorkers.resume();
  renderVersions(runtime.coreWireSchemaHash);
  if (runtime.coreWireSchemaHash !== TRUAPI_WIRE_SCHEMA_HASH)
    log(
      `inspector will group frames without decoding them: the core's wire schema ` +
        `${runtime.coreWireSchemaHash ?? "(none reported)"} is not this page's ${TRUAPI_WIRE_SCHEMA_HASH}`,
    );
  renderWallets();

  const tabWallet =
    testUser === null
      ? wallets.find(
          sessionStorage.getItem(tabWalletKey(activeNetwork.id)) ?? "",
        )
      : undefined;
  if (tabWallet) {
    walletSelect.value = tabWallet.id;
    await signIn(tabWallet);
  }
  await serial((current) => devSession?.start(current) ?? Promise.resolve());

  // Prefill only. The product id decides what the core trusts the page as,
  // so a link must not be able to open a product under one without the
  // user choosing Open.
  const params = new URLSearchParams(window.location.search);
  addressInput.value = params.get("product") ?? "";
  productIdInput.value = params.get("productId") ?? "";
  renderAddress();

  // The product this tab opened itself before a reload comes back with the
  // wallet. A link's `product` parameter never does.
  const last = testUser === null ? readTabProduct() : null;
  if (last && !params.has("product")) {
    addressInput.value = displayAddress(last.address, activeNetwork);
    const derived = effectiveProductIdFor(last.parsed, "").id;
    const entered = last.productId !== derived;
    if (entered) productIdInput.value = last.productId;
    renderAddress();
    destination = last.destination;
    renderProduct();
    if (activeWallet)
      await serial((runtime) =>
        openResolved(runtime, last.parsed, last.productId, entered),
      );
  }
}

function readTabProduct(): {
  address: string;
  productId: string;
  destination: Destination | null;
  parsed: Address;
} | null {
  try {
    const stored: unknown = JSON.parse(
      sessionStorage.getItem(tabProductKey(activeNetwork.id)) ?? "null",
    );
    if (typeof stored !== "object" || stored === null) return null;
    const { address, productId, destination } = stored as Record<
      string,
      unknown
    >;
    if (typeof address !== "string" || typeof productId !== "string")
      return null;
    return {
      address,
      productId,
      destination: DESTINATIONS.find((known) => known === destination) ?? null,
      parsed: parseTypedAddress(address, activeNetwork),
    };
  } catch {
    return null;
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
  log(coreStatus.textContent);
});
