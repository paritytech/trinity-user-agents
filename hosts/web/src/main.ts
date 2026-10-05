import "./style.css";
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
} from "./address.js";
import {
  accountStatusRows,
  abbreviate,
  productAllowanceRows,
  type ChainState,
  type ProductAllowanceView,
} from "./account-status.js";
import { allocationRecord } from "./allowance-ledger.js";
import {
  permissionRows,
  queriesFor,
  type PermissionsView,
} from "./permissions-status.js";
import { bindRecentsMenu } from "./address-recents.js";
import { bindChrome } from "./chrome.js";
import { copyInPage } from "./copy.js";
import { bindDockResize } from "./dock-resize.js";
import { resolveSource } from "./dotns.js";
import { createFrameTap } from "./frame-tap.js";
import { createHostCallbacks } from "./callbacks.js";
import { connectNetwork, type Network } from "./network.js";
import {
  NETWORKS,
  networkConfig,
  type NetworkConfig,
} from "./network-config.js";
import { DEFAULT_NETWORK } from "./network-scope.js";
import {
  chosenNetwork,
  rememberNetwork,
  tabProductKey,
  tabWalletKey,
  type NetworkChoice,
} from "./network-choice.js";
import { ask } from "./prompt.js";
import { RecentProducts } from "./recents.js";
import {
  readPeopleChain,
  type AccountReading,
  type AccountToRead,
} from "./people-chain.js";
import {
  readBalance,
  readProductAccount,
  type ProductChainState,
} from "./product-chain.js";
import { StageLoading, documentsBeforeProduct } from "./loading.js";
import { openProduct, type OpenProduct } from "./product.js";
import {
  notOpenableText,
  productDetailsText,
  productStatusText,
  type Opened,
} from "./product-status.js";
import {
  hostBase,
  loaderUrl,
  mountScope,
  requireSecureHost,
} from "./sandbox/mount.js";
import { HostStorage, localStorageChanges } from "./storage.js";
import { SerialQueue } from "./serial-queue.js";
import { versionSections, type VersionRow } from "./versions.js";
import { bindViewport } from "./viewport.js";
import {
  WalletPublicInfoStore,
  sessionUsernameFrom,
  usernameFrom,
  walletLabel,
} from "./wallet-label.js";
import { walletPanel, type WalletMode } from "./wallet-panel.js";
import { WalletStore, type Wallet } from "./wallets.js";

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
const productIdReset = element<HTMLButtonElement>("product-id-reset");
const productIdEffective = element("product-id-effective");
const openButton = element<HTMLButtonElement>("open");
const closeButton = element<HTMLButtonElement>("close-product");
const productStatus = element("product-status");
const productMore = element<HTMLDetailsElement>("product-more");
const productDetails = element("product-details");
const productFrame = element("product-frame");
const stageLoadingPill = element("stage-loading-pill");
const stageLoadingText = element("stage-loading-text");

const chrome = bindChrome({
  appbar: document.querySelector<HTMLElement>(".appbar")!,
  stage: element("stage"),
  scrim: element("scrim"),
  panels: { menu: element("menu"), inspector: element("inspector") },
  toggles: {
    menu: element<HTMLButtonElement>("menu-toggle"),
    inspector: element<HTMLButtonElement>("inspector-toggle"),
  },
});

const inspector = createInAppDebugger();
inspector.mount(element("inspector-mount"));
bindDockResize(
  element("inspector-resize"),
  element("inspector"),
  document.querySelector<HTMLElement>(".appbar")!,
);

bindViewport({
  select: element<HTMLSelectElement>("viewport"),
  scale: element("viewport-scale"),
  area: element("viewport-area"),
  frame: productFrame,
  root: document.body,
  fills: window.matchMedia("(max-width: 900px)"),
  custom: {
    group: element("viewport-custom"),
    width: element<HTMLInputElement>("viewport-width"),
    height: element<HTMLInputElement>("viewport-height"),
  },
  handle: element("viewport-handle"),
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
  signInButton.disabled = !ready || walletSelect.value === activeWallet?.id;
  signOutButton.disabled = !ready || activeWallet === null;
  forgetButton.disabled = !ready || !wallets.has(walletSelect.value);
  addWalletButton.disabled = !ready;
  openButton.disabled = !ready;
  addressGo.disabled = !ready;
  closeButton.disabled = !ready || product === null;
  resetDataButton.disabled =
    !ready || product === null || activeWallet === null;
  renderWalletPanel();
  renderAddress();
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
  const details = product
    ? productDetailsText(product, opened, activeNetwork)
    : "";
  productDetails.textContent = details;
  productMore.hidden = details === "";
  renderControls();
  void refreshProductAllowance();
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
  renderAccountStatus();
  sessionLine.title = "";
  switch (state.tag) {
    case "Connected": {
      const { publicKey, identityAccountId, fullUsername, liteUsername } =
        state.value;
      const name =
        fullUsername ?? liteUsername ?? activeWallet?.name ?? "wallet";
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
    closeProduct();
    try {
      await change(runtime);
    } finally {
      if (reopen && how)
        await openTarget(
          runtime,
          { ...how, url: reopen.url },
          reopen.productId,
          entered,
          scope,
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
): Promise<void> {
  closeProduct();
  const op = loading.begin("Loading…");
  sandboxStatus = "";
  try {
    product = await openProduct(runtime, target.url, productId, productFrame, {
      mounted: target.via === "name",
      tap: createFrameTap(inspector, productId, runtime.coreWireSchemaHash),
      onFrameLoad(count) {
        if (count < documentsBeforeProduct(target.via)) return;
        loading.finish(op);
        sandboxStatus = "";
        renderProduct();
      },
      onSandboxStatus(state, detail) {
        sandboxStatus = `Sandbox: ${detail}`;
        if (state === "error") {
          loading.finish(op);
          log(`sandbox could not start the product: ${detail}`);
        }
        renderProduct();
      },
    });
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
    recents.record(
      recentScope(),
      { address: target.address, productId, entered },
      activeNetwork.id,
    );
    recentsMenu.refresh();
    sessionStorage.setItem(
      tabProductKey(activeNetwork.id),
      JSON.stringify({ address: target.address, productId }),
    );
    log(
      target.via === "name"
        ? `opened ${target.address} as ${productId} from ${target.url.href}`
        : `opened ${target.url.href} as ${productId}`,
    );
  } catch (error) {
    loading.finish(op);
    productStatus.textContent = `Could not open ${target.url.href} as ${productId}: ${errorText(error)}`;
    log(productStatus.textContent);
  }
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
    productStatus.textContent = errorText(error);
    log(productStatus.textContent);
    return;
  }
  const { id: productId, source } = effectiveProductIdFor(
    address,
    productIdInput.value,
  );
  const entered = source === "entered";
  if (entered && overrideOrigin !== null && overrideOrigin !== scopeOf(address))
    log(`product id ${productId} now applies to ${scopeOf(address)}`);
  void serial((runtime) => openResolved(runtime, address, productId, entered));
  chrome.dismissCovering();
}

const recents = new RecentProducts(localStorage);

/** History is kept per wallet, like the rest of its data; signed out has its own. */
function recentScope(): string {
  return activeWallet?.id ?? "signed-out";
}

const recentsMenu = bindRecentsMenu({
  input: addressInput,
  list: recentsList,
  network: activeNetwork,
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
      { label: "Cancel", value: false },
      { label: "Reset app data", value: true, primary: true },
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

addressInput.addEventListener("input", renderAddress);
addressInput.addEventListener("keydown", (event) => {
  if (event.key !== "Escape") return;
  if (recentsMenu.close()) return;
  addressInput.value = displayAddress(opened?.address ?? "", activeNetwork);
  renderAddress();
  addressInput.blur();
});

productIdInput.addEventListener("input", renderEffectiveId);
productIdReset.addEventListener("click", () => {
  productIdInput.value = "";
  renderEffectiveId();
  productIdInput.focus();
});

closeButton.addEventListener("click", () => {
  void serial(async () => {
    closeProduct();
    sessionStorage.removeItem(tabProductKey(activeNetwork.id));
    log("closed the product");
  });
});

async function boot(): Promise<void> {
  const connection = await connectNetwork(activeNetwork);
  chainAccess = connection;
  renderAccountStatus();
  const callbacks = createHostCallbacks({
    network: connection,
    storage,
    log,
    onAuthState: renderSession,
    onProductNavigation: (url) =>
      log(`dotNS navigation is not supported by this host: ${url}`),
    onNetworkAsked(domains) {
      askedDomains = [...new Set([...askedDomains, ...domains])];
    },
    onPermissionDecided: refreshPermissionsSoon,
  });
  const worker = new Worker(new URL("./core-worker.ts", import.meta.url), {
    type: "module",
  });
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
  log("core ready");
  renderVersions(runtime.coreWireSchemaHash);
  if (runtime.coreWireSchemaHash !== TRUAPI_WIRE_SCHEMA_HASH)
    log(
      `inspector will group frames without decoding them: the core's wire schema ` +
        `${runtime.coreWireSchemaHash ?? "(none reported)"} is not this page's ${TRUAPI_WIRE_SCHEMA_HASH}`,
    );
  renderWallets();

  const tabWallet = wallets.find(
    sessionStorage.getItem(tabWalletKey(activeNetwork.id)) ?? "",
  );
  if (tabWallet) {
    walletSelect.value = tabWallet.id;
    await signIn(tabWallet);
  }

  // Prefill only. The product id decides what the core trusts the page as,
  // so a link must not be able to open a product under one without the
  // user choosing Open.
  const params = new URLSearchParams(window.location.search);
  addressInput.value = params.get("product") ?? "";
  productIdInput.value = params.get("productId") ?? "";
  renderAddress();

  // The product this tab opened itself before a reload comes back with the
  // wallet. A link's `product` parameter never does.
  const last = readTabProduct();
  if (last && !params.has("product")) {
    addressInput.value = displayAddress(last.address, activeNetwork);
    const derived = effectiveProductIdFor(last.parsed, "").id;
    const entered = last.productId !== derived;
    if (entered) productIdInput.value = last.productId;
    renderAddress();
    if (activeWallet)
      await serial((runtime) =>
        openResolved(runtime, last.parsed, last.productId, entered),
      );
  }
}

function readTabProduct(): {
  address: string;
  productId: string;
  parsed: Address;
} | null {
  try {
    const stored: unknown = JSON.parse(
      sessionStorage.getItem(tabProductKey(activeNetwork.id)) ?? "null",
    );
    if (typeof stored !== "object" || stored === null) return null;
    const { address, productId } = stored as Record<string, unknown>;
    if (typeof address !== "string" || typeof productId !== "string")
      return null;
    return { address, productId, parsed: parseTypedAddress(address, activeNetwork) };
  } catch {
    return null;
  }
}

renderWallets();
renderProduct();
renderAccountStatus();
boot().catch((error: unknown) => {
  coreStatus.textContent = `The core failed to start: ${String(error)}`;
  log(coreStatus.textContent);
});
