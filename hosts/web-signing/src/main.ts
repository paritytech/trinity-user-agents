import "./style.css";
import { createWebWorkerPairingHostRuntime } from "@parity/truapi-host/web";
import type { WorkerPairingHostRuntime } from "@parity/truapi-host/web";
import type { AuthState } from "@parity/truapi-host";
import { generateMnemonic } from "@scure/bip39";
import { wordlist } from "@scure/bip39/wordlists/english.js";
import { createHostCallbacks } from "./callbacks.js";
import { connectPaseo } from "./network.js";
import {
  openProduct,
  parseProductUrl,
  productIdFor,
  type OpenProduct,
} from "./product.js";
import { HostStorage, localStorageChanges } from "./storage.js";
import { SerialQueue } from "./serial-queue.js";
import { bindViewport } from "./viewport.js";
import { WalletStore, type Wallet } from "./wallets.js";

/** The wallet this tab is signed in with, kept across reloads of the tab only. */
const TAB_WALLET_KEY = "truapi-web-signing-host:tab-wallet";
const MAX_LOG_LINES = 500;

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
const addWalletButton = element<HTMLButtonElement>("add-wallet");
const logList = element("log");
const productForm = element<HTMLFormElement>("product-form");
const productUrlInput = element<HTMLInputElement>("product-url");
const productIdInput = element<HTMLInputElement>("product-id");
const openButton = element<HTMLButtonElement>("open");
const productStatus = element("product-status");
const productFrame = element("product-frame");

bindViewport(element("viewport"), element("viewport-area"), productFrame);

function log(text: string): void {
  const line = document.createElement("li");
  line.textContent = `${new Date().toLocaleTimeString()} ${text}`;
  logList.append(line);
  while (logList.childElementCount > MAX_LOG_LINES)
    logList.firstElementChild?.remove();
  line.scrollIntoView({ block: "nearest" });
}

const wallets = new WalletStore(localStorage);
const storage = new HostStorage(localStorage, localStorageChanges);

let runtime: WorkerPairingHostRuntime | null = null;
let activeWallet: Wallet | null = null;
let product: OpenProduct | null = null;

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

function renderWallets(): void {
  const selected = walletSelect.value || activeWallet?.id || "";
  walletSelect.replaceChildren(
    ...wallets.list().map((wallet) => new Option(wallet.name, wallet.id)),
  );
  if (wallets.find(selected)) walletSelect.value = selected;
  renderControls();
}

function renderControls(): void {
  const ready = runtime !== null && !queue.busy;
  signInButton.disabled = !ready || walletSelect.value === activeWallet?.id;
  signOutButton.disabled = !ready || activeWallet === null;
  forgetButton.disabled =
    !ready || wallets.mnemonicOf(walletSelect.value) === undefined;
  addWalletButton.disabled = !ready;
  openButton.disabled = !ready;
}

function renderAuthState(state: AuthState): void {
  switch (state.tag) {
    case "Connected": {
      const { publicKey, identityAccountId, fullUsername, liteUsername } =
        state.value;
      const name =
        fullUsername ?? liteUsername ?? activeWallet?.name ?? "wallet";
      sessionLine.textContent =
        `Signed in as ${name}. Root key ${publicKey}` +
        (identityAccountId ? `, identity account ${identityAccountId}` : "");
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
    closeProduct();
    try {
      await change(runtime);
    } finally {
      if (reopen) await openInFrame(runtime, reopen.url, reopen.productId);
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
  product?.dispose();
  product = null;
  productStatus.textContent = "No product open.";
}

async function openInFrame(
  runtime: WorkerPairingHostRuntime,
  url: URL,
  productId: string,
): Promise<void> {
  closeProduct();
  try {
    product = await openProduct(runtime, url, productId, productFrame);
    productStatus.textContent = `${url.href} as ${productId}`;
    log(`opened ${url.href} as ${productId}`);
  } catch (error) {
    productStatus.textContent = `Could not open ${url.href} as ${productId}: ${String(error)}`;
    log(productStatus.textContent);
  }
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

addWalletButton.addEventListener("click", () => {
  const typed = mnemonicInput.value.trim();
  const mnemonic = typed || generateMnemonic(wordlist, 128);
  try {
    const wallet = wallets.save(
      walletNameInput.value.trim() || "Test wallet",
      mnemonic,
    );
    // A new phrase stays on screen so it can be written down; an imported
    // one is cleared.
    mnemonicInput.value = typed ? "" : mnemonic;
    walletNameInput.value = "";
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

productForm.addEventListener("submit", (event) => {
  event.preventDefault();
  try {
    const url = parseProductUrl(productUrlInput.value);
    const productId = productIdFor(url, productIdInput.value);
    void serial((runtime) => openInFrame(runtime, url, productId));
  } catch (error) {
    productStatus.textContent = String(error);
  }
});

async function boot(): Promise<void> {
  const network = await connectPaseo();
  const callbacks = createHostCallbacks({
    network,
    storage,
    log,
    onAuthState: renderAuthState,
    onProductNavigation: (url) =>
      log(`dotNS navigation is not supported by this host: ${url}`),
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
  productUrlInput.value = params.get("product") ?? "";
  productIdInput.value = params.get("productId") ?? "";
}

renderWallets();
boot().catch((error: unknown) => {
  coreStatus.textContent = `The core failed to start: ${String(error)}`;
  log(coreStatus.textContent);
});
