import "./style.css";
import {
  createWebWorkerPairingHostRuntime,
  PRODUCT_RESOURCE_STATUS_UNSUPPORTED,
} from "@parity/truapi-host/web";
import type { WorkerPairingHostRuntime } from "@parity/truapi-host/web";
import type { AuthState } from "@parity/truapi-host";
import { TRUAPI_CODEC_VERSION, TRUAPI_WIRE_SCHEMA_HASH } from "@parity/truapi";
import { createInAppDebugger } from "@parity/truapi-debugger";
import {
  effectiveProductIdFor,
  parseAddress,
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
  type GateState,
  type PermissionsView,
} from "./permissions-status.js";
import { bindChrome } from "./chrome.js";
import { bindDockResize } from "./dock-resize.js";
import { PASEO_DOTNS, resolveSource } from "./dotns.js";
import { createFrameTap } from "./frame-tap.js";
import { createHostCallbacks } from "./callbacks.js";
import { connectPaseo, type Network } from "./network.js";
import { readPeopleChain } from "./people-chain.js";
import {
  parseProductResourceStatus,
  type ResourcesState,
} from "./product-resources.js";
import { StageLoading, documentsBeforeProduct } from "./loading.js";
import { openProduct, type OpenProduct } from "./product.js";
import {
  notOpenableText,
  productDetailsText,
  productStatusText,
  type Opened,
} from "./product-status.js";
import {
  NO_RELAXATIONS,
  activeRelaxations,
  loadRelaxations,
  sameRelaxations,
  saveRelaxations,
  type Relaxations,
} from "./relaxations.js";
import { PortLedger } from "./sandbox/ports.js";
import {
  containerOffAllowed,
  productLabel,
  requireSecureHost,
  sandboxOrigin,
  sandboxUrl,
} from "./sandbox/origin.js";
import { HostStorage, localStorageChanges } from "./storage.js";
import { SerialQueue } from "./serial-queue.js";
import { versionSections, type VersionRow } from "./versions.js";
import { bindViewport } from "./viewport.js";
import { walletPanel, type WalletMode } from "./wallet-panel.js";
import { WalletStore, type Wallet } from "./wallets.js";

/** The wallet this tab is signed in with, kept across reloads of the tab only. */
const TAB_WALLET_KEY = "truapi-web-signing-host:tab-wallet";
/** The product this tab last opened, so a reload brings it back. */
const TAB_PRODUCT_KEY = "truapi-web-signing-host:tab-product";
/** Why archives cannot run without the container on this host. */
const CONTAINER_OFF_REFUSED =
  "Archives without the container need localhost: products here share cookies by host name.";

/** Where product origins live, as the server was started with. Nothing typed or linked here changes it. */
const SANDBOX_TEMPLATE = __SANDBOX_POLICY__.template;
const MAX_LOG_LINES = 500;
/** Whether archives may run without the container here: only beside a loopback host, where each product is its own host name. */
const containerOffOk = containerOffAllowed(window.location, SANDBOX_TEMPLATE);

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
const addressGo = element<HTMLButtonElement>("address-go");
const productIdInput = element<HTMLInputElement>("product-id");
const expectContainerInput = element<HTMLInputElement>("expect-container");
const productIdReset = element<HTMLButtonElement>("product-id-reset");
const productIdEffective = element("product-id-effective");
const openButton = element<HTMLButtonElement>("open");
const closeButton = element<HTMLButtonElement>("close-product");
const productStatus = element("product-status");
const productMore = element<HTMLDetailsElement>("product-more");
const productDetails = element("product-details");
const sandboxOriginInput = element<HTMLInputElement>("sandbox-origin");
const relaxContainerInput = element<HTMLInputElement>("relax-container");
const relaxNetworkInput = element<HTMLInputElement>("relax-network");
const relaxNote = element("relax-note");
const relaxBanner = element("relax-banner");
const relaxBannerText = element("relax-banner-text");
const relaxBannerOff = element<HTMLButtonElement>("relax-banner-off");
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
});

const versionsList = element("versions");
const versionsTechnical = element("versions-technical");
const versionsWarning = element("versions-warning");

function versionLines(rows: VersionRow[]): HTMLElement[] {
  return rows.map((row) => {
    const line = document.createElement("div");
    const label = document.createElement("dt");
    label.textContent = row.label;
    const value = document.createElement("dd");
    value.textContent = row.value;
    if (row.state) value.dataset.state = row.state;
    if (row.title) value.title = row.title;
    if (row.detail) {
      const detail = document.createElement("span");
      detail.className = "detail";
      detail.textContent = row.detail;
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
const storage = new HostStorage(localStorage, localStorageChanges);

let runtime: WorkerPairingHostRuntime | null = null;
let activeWallet: Wallet | null = null;
let product: OpenProduct | null = null;
let opened: Opened | null = null;
/** Whether the open product's id was entered rather than derived. */
let openedWithOverride = false;
/** The origin the entered product id was last opened with, for the log. */
let overrideOrigin: string | null = null;
/** The relaxations the tab has chosen, and the ones the open product started with. */
let relaxations: Relaxations = loadRelaxations(sessionStorage);
let appliedRelaxations: Relaxations = NO_RELAXATIONS;
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
    ...saved.map((wallet) => new Option(wallet.name, wallet.id)),
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
  const typed = addressInput.value.trim();
  const dirty = typed !== "" && typed !== (opened?.address ?? "");
  addressForm.dataset.dirty = String(dirty);
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
      const address = parseAddress(addressInput.value);
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
  openButton.textContent = product?.lost ? "Reopen" : "Open";
  productStatus.textContent = product
    ? productStatusText(product, opened, sandboxStatus, window.isSecureContext)
    : "No product open.";
  const details = product ? productDetailsText(product, opened) : "";
  productDetails.textContent = details;
  productMore.hidden = details === "";
  renderRelaxations();
  renderControls();
  void refreshProductAllowance();
}

/**
 * Show which protections are off. The banner stays for as long as any is: in
 * force for the open product, or chosen and waiting for the next Open.
 */
function renderRelaxations(): void {
  relaxContainerInput.disabled = !containerOffOk;
  relaxContainerInput.title = containerOffOk ? "" : CONTAINER_OFF_REFUSED;
  relaxContainerInput.checked = relaxations.archiveWithoutContainer;
  relaxNetworkInput.checked = relaxations.approveNetworkWithoutAsking;
  const inForce = product !== null;
  const shown = activeRelaxations(inForce ? appliedRelaxations : relaxations);
  const pending = inForce && !sameRelaxations(relaxations, appliedRelaxations);
  relaxBanner.hidden = shown.length === 0 && !pending;
  relaxBannerOff.textContent = inForce ? "Turn off and reopen" : "Turn off";
  document.body.dataset.relaxed = String(!relaxBanner.hidden);
  relaxBannerText.textContent = pending
    ? "Relaxations changed. Press Open to apply."
    : `Relaxed: ${shown.join(", ")}.`;
  relaxNote.textContent = pending
    ? "Changed. Press Open to apply."
    : "Applies on the next Open.";
}

function setRelaxations(next: Relaxations): void {
  relaxations = next;
  saveRelaxations(sessionStorage, next);
  log(
    activeRelaxations(next).length === 0
      ? "developer relaxations cleared"
      : `developer relaxations chosen: ${activeRelaxations(next).join("; ")}`,
  );
  renderRelaxations();
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
/** What the chains hold for the open product, as the core read it on the last Refresh. */
let resources: ResourcesState = { state: "idle" };
/** Counts reads and account changes, so a late reply for an older one is dropped. */
let resourcesRun = 0;
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
          networkSuffix: chainAccess?.networkSuffix ?? "paseo",
          walletName: activeWallet?.name ?? null,
        },
        chainState,
      ),
    ),
  );
  productStatusList.replaceChildren(
    ...versionLines(productAllowanceRows(productAllowance, resources)),
  );
  permissionsList.replaceChildren(
    ...versionLines(
      permissionRows(permissions, {
        gate: gateState(),
        autoApproveNetwork: appliedRelaxations.approveNetworkWithoutAsking,
        archiveWithoutContainer: appliedRelaxations.archiveWithoutContainer,
      }),
    ),
  );
  accountStatusRefresh.disabled =
    lastAuthState.tag !== "Connected" ||
    chainAccess === null ||
    chainState.state === "loading" ||
    resources.state === "loading";
}

/** What gates the open product's own requests: the container, as it stands. */
function gateState(): GateState {
  if (product?.lost) return "ended";
  switch (product?.container?.state) {
    case "connected":
      return "on";
    case "waiting":
      return "waiting";
    default:
      return "none";
  }
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

/** How long the chains may take to answer a resource read before it is given up. */
const RESOURCE_READ_TIMEOUT_MS = 60_000;

/**
 * Ask the core what the chains hold for the open product: its Statement Store
 * allocation, Bulletin authorization and PGAS balance. Read-only, run on
 * Refresh only. A reply is dropped when the account changed meanwhile, and is
 * shown only while the product it was read for is still the open one.
 */
async function refreshProductResources(): Promise<void> {
  const productId = product?.productId;
  const current = runtime;
  if (
    productId === undefined ||
    current === null ||
    lastAuthState.tag !== "Connected"
  ) {
    resourcesRun += 1;
    resources = { state: "idle" };
    return;
  }
  const run = ++resourcesRun;
  resources = { state: "loading", productId };
  renderAccountStatus();
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(() => {
      reject(
        new Error(
          `no answer from the chains after ${RESOURCE_READ_TIMEOUT_MS / 1000}s`,
        ),
      );
    }, RESOURCE_READ_TIMEOUT_MS);
  });
  try {
    const json = await Promise.race([
      current.getProductResourceStatus(productId),
      timeout,
    ]);
    if (run !== resourcesRun) return;
    resources = {
      state: "done",
      productId,
      checkedAt: new Date(),
      status: parseProductResourceStatus(json),
    };
  } catch (error) {
    if (run !== resourcesRun) return;
    resources =
      errorText(error) === PRODUCT_RESOURCE_STATUS_UNSUPPORTED
        ? { state: "unsupported", productId }
        : { state: "error", productId, message: errorText(error) };
    if (resources.state === "error")
      log(`Product resource status: ${resources.message}`);
  } finally {
    clearTimeout(timer);
  }
  renderAccountStatus();
}

/**
 * Read the signed-in account's standing from the People chain. Read-only, on
 * request and once per account when the fold is first opened; nothing polls.
 * A failed read changes only what this shows, never the session.
 */
async function refreshChainStatus(): Promise<void> {
  const access = chainAccess;
  if (lastAuthState.tag !== "Connected" || access === null) return;
  const { publicKey, identityAccountId } = lastAuthState.value;
  const run = ++chainRun;
  chainState = { state: "loading" };
  renderAccountStatus();
  void refreshProductAllowance();
  void refreshProductResources();
  try {
    const reading = await readPeopleChain(access.chain, access.genesis.people, [
      ...(identityAccountId
        ? [{ role: "identity" as const, accountId: identityAccountId }]
        : []),
      { role: "root" as const, accountId: publicKey },
    ]);
    if (run !== chainRun) return;
    chainState = { state: "done", checkedAt: new Date(), reading };
  } catch (error) {
    if (run !== chainRun) return;
    chainState = { state: "error", message: errorText(error) };
    log(`People chain status: ${errorText(error)}`);
  }
  renderAccountStatus();
}

/** Forget the chain state when the account changes, and read again if the fold is open. */
function trackChainAccount(state: AuthState): void {
  const key = state.tag === "Connected" ? state.value.publicKey : null;
  if (key === chainFor) return;
  chainFor = key;
  chainRun += 1;
  chainState = { state: "idle" };
  resourcesRun += 1;
  resources = { state: "idle" };
  void refreshProductAllowance();
  if (key !== null && accountStatusFold.open) void refreshChainStatus();
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
    storage.useWallet(wallet.id);
    activeWallet = wallet;
    try {
      await runtime.activateLocalSession(wallet.entropy);
    } catch (error) {
      storage.useWallet(null);
      activeWallet = null;
      throw new Error(
        `Could not sign in with ${wallet.name}: ${String(error)}`,
      );
    }
    sessionStorage.setItem(TAB_WALLET_KEY, wallet.id);
    log(`signed in with ${wallet.name}`);
  });
}

async function signOutOf(runtime: WorkerPairingHostRuntime): Promise<void> {
  await runtime.disconnectSession();
  storage.useWallet(null);
  sessionStorage.removeItem(TAB_WALLET_KEY);
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
 * loaded by a sandbox page on an origin of the product's own, which is never
 * this page's origin. Nothing is served from here.
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
  if (relaxations.archiveWithoutContainer && !containerOffOk)
    throw new Error(CONTAINER_OFF_REFUSED);
  // Fail before any lookup when this host has no origin to give the product.
  const origin = await sandboxOrigin(window.location, productId, {
    template: SANDBOX_TEMPLATE,
    ports: new PortLedger(localStorage),
  });
  productStatus.textContent = `Resolving ${address.name} on Paseo Asset Hub…`;
  let choice: Awaited<ReturnType<typeof resolveSource>>;
  try {
    choice = await resolveSource(address.name);
  } catch (error) {
    throw new Error(
      `Could not read ${address.name}: ${errorText(error)}. Check the connection, or enter a URL.`,
    );
  }
  const { source, skipped } = choice;
  if (source === null) throw new Error(notOpenableText(address.name, skipped));
  return {
    address: shown,
    via: "name",
    cid: source.cid,
    kind: source.kind,
    origin,
    record: source.record,
    skipped,
    url: sandboxUrl({
      origin,
      cid: source.cid,
      gateway: PASEO_DOTNS.contentGateway,
      hostOrigin: window.location.origin,
      kind: source.kind,
      start: `/${address.suffix}`,
      container: !relaxations.archiveWithoutContainer,
      owner: productLabel(productId),
    }),
  };
}

/** The scope an entered product id was used with, for the log. */
function scopeOf(address: Address): string {
  return address.kind === "url" ? address.url.origin : address.name;
}

/**
 * Retire the sandbox port a product found held by another product's data, so
 * the next Open takes another. The report is dropped when a different product
 * is open by then.
 */
async function skipHeldPort(target: Target): Promise<void> {
  const port = Number(target.url.port);
  const held = `port ${port} holds another product's data`;
  let status: string;
  try {
    if (port > 0) await new PortLedger(localStorage).retire(port);
    status = `Sandbox: ${held}. It is skipped now; press Open again.`;
  } catch (error) {
    status = `Sandbox: ${held}, and it could not be skipped: ${errorText(error)}`;
  }
  log(status);
  if (product?.url !== target.url) return;
  sandboxStatus = status;
  renderProduct();
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
  appliedRelaxations = relaxations;
  sandboxStatus = "";
  try {
    product = await openProduct(runtime, target.url, productId, productFrame, {
      tap: createFrameTap(inspector, productId, runtime.coreWireSchemaHash),
      expectContainer:
        target.via === "name"
          ? !appliedRelaxations.archiveWithoutContainer
          : expectContainerInput.checked,
      onContainerConnected() {
        log("the page's container connected; permission prompts are active");
        sandboxStatus = "";
        renderProduct();
      },
      onDrop: (reason) =>
        log(`dropped a frame on the wrong channel: ${reason}`),
      onFrameLoad(count) {
        if (count >= documentsBeforeProduct(target.via)) loading.finish(op);
      },
      onSandboxStatus(state, detail, code) {
        sandboxStatus = `Sandbox: ${detail}`;
        if (state === "error") {
          loading.finish(op);
          log(`sandbox could not start the product: ${detail}`);
        }
        if (code === "owner") void skipHeldPort(target);
        renderProduct();
      },
      onLost() {
        log(
          "the page loaded a new document; its connection to the host ended and is not restored",
        );
        renderProduct();
      },
    });
    opened = {
      address: target.address,
      via: target.via,
      cid: target.cid,
      kind: target.kind,
      origin: target.origin,
      record: target.record,
      skipped: target.skipped,
    };
    openedWithOverride = entered;
    askedDomains = [];
    overrideOrigin = entered ? scope : null;
    addressInput.value = target.address;
    renderProduct();
    sessionStorage.setItem(
      TAB_PRODUCT_KEY,
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
    address = parseAddress(addressInput.value);
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
  addressInput.value = opened?.address ?? "";
  renderAddress();
  addressInput.blur();
});

productIdInput.addEventListener("input", renderEffectiveId);
productIdReset.addEventListener("click", () => {
  productIdInput.value = "";
  renderEffectiveId();
  productIdInput.focus();
});

relaxContainerInput.addEventListener("change", () =>
  setRelaxations({
    ...relaxations,
    archiveWithoutContainer: relaxContainerInput.checked,
  }),
);
relaxNetworkInput.addEventListener("change", () =>
  setRelaxations({
    ...relaxations,
    approveNetworkWithoutAsking: relaxNetworkInput.checked,
  }),
);
relaxBannerOff.addEventListener("click", () => {
  setRelaxations(NO_RELAXATIONS);
  const current = opened;
  const id = product?.productId;
  if (current === null || id === undefined) return;
  // A name is resolved again, so its sandbox URL carries the new settings.
  void serial((runtime) =>
    openResolved(
      runtime,
      parseAddress(current.address),
      id,
      openedWithOverride,
    ),
  );
});

sandboxOriginInput.value = SANDBOX_TEMPLATE;

/** A stored container-off choice does not survive onto a host that cannot keep cookies apart. */
if (relaxations.archiveWithoutContainer && !containerOffOk) {
  setRelaxations({ ...relaxations, archiveWithoutContainer: false });
  log(CONTAINER_OFF_REFUSED);
}

closeButton.addEventListener("click", () => {
  void serial(async () => {
    closeProduct();
    sessionStorage.removeItem(TAB_PRODUCT_KEY);
    log("closed the product");
  });
});

async function boot(): Promise<void> {
  const network = await connectPaseo();
  chainAccess = network;
  renderAccountStatus();
  const callbacks = createHostCallbacks({
    network,
    storage,
    log,
    onAuthState: renderSession,
    onProductNavigation: (url) =>
      log(`dotNS navigation is not supported by this host: ${url}`),
    approveNetwork: () => appliedRelaxations.approveNetworkWithoutAsking,
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
      host: { name: "TrUAPI web signing host", platform: "Web" },
      platform: { type: "browser", version: navigator.userAgent },
      people: { genesisHash: network.genesis.people },
      bulletin: { genesisHash: network.genesis.bulletin },
      assetHub: { genesisHash: network.genesis.assetHub },
      // Required by the config type; a signing host never pairs.
      pairing: { deeplinkScheme: "polkadotapp" },
      networkSuffix: network.networkSuffix,
    },
  });
  coreStatus.textContent = `Core ready on Paseo · wire schema ${runtime.coreWireSchemaHash?.slice(0, 12) ?? "unknown"}`;
  log("core ready");
  renderVersions(runtime.coreWireSchemaHash);
  if (runtime.coreWireSchemaHash !== TRUAPI_WIRE_SCHEMA_HASH)
    log(
      `inspector will group frames without decoding them: the core's wire schema ` +
        `${runtime.coreWireSchemaHash ?? "(none reported)"} is not this page's ${TRUAPI_WIRE_SCHEMA_HASH}`,
    );
  renderWallets();

  const tabWallet = wallets.find(sessionStorage.getItem(TAB_WALLET_KEY) ?? "");
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
    addressInput.value = last.address;
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
      sessionStorage.getItem(TAB_PRODUCT_KEY) ?? "null",
    );
    if (typeof stored !== "object" || stored === null) return null;
    const { address, productId } = stored as Record<string, unknown>;
    if (typeof address !== "string" || typeof productId !== "string")
      return null;
    return { address, productId, parsed: parseAddress(address) };
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
