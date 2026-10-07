// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

// The CLI host: boots the Rust core in-process and owns everything the core
// leaves to a host. That means rendering (via the presenter), owner-only
// persistence, pooled chain connections, and clearing product storage across
// identities.
//
// It is parameterized by the EMBEDDING app's metadata: for a terminal app the
// app is its own host (d3pot is not "a product inside dotli", it IS the
// host), so name/icon/version, the pairing deeplink scheme, and the
// people/bulletin genesis hashes all come from the caller.

import { join } from "node:path";
import type { AuthState, WireProvider } from "@parity/truapi-host";
import { createChainPool, type ChainEndpoints } from "./chain-pool.js";
import { createHostCallbacks } from "./callbacks.js";
import { FileKeyValueStore } from "./kv.js";
import { createLoopbackProvider } from "./loopback.js";
import { createTerminalPresenter, type HostPresenter } from "./presenter.js";
import { loadWasmCore } from "./wasm.js";

export interface CliHostConfig {
  /** Metadata describing the embedding app, shown by the wallet on pairing. */
  host: { name: string; icon?: string; version?: string };
  /** Wallet pairing deeplink scheme (the wallet's URI scheme, not yours). */
  pairing: { deeplinkScheme: string };
  /** People-chain genesis hash (identity lookup), `0x`-prefixed hex. */
  people: { genesisHash: string };
  /** Bulletin-chain genesis hash (in-core preimage submission). */
  bulletin: { genesisHash: string };
  /** Asset Hub genesis hash (dotNS session-username resolution). */
  assetHub: { genesisHash: string };
  /**
   * Chains this host serves, keyed by genesis hash. Also answers the core's
   * `featureSupported` probes, and entries carrying a `role` are advertised
   * through `supportedChains`.
   */
  chains: ChainEndpoints;
  /**
   * Environment id advertised through the core's supported-chains set, e.g.
   * `paseo-next-v2`. Informational, but products see it in `getChainInfo`.
   */
  network: string;
  /** Directory for the host's persistent state. Files are created 0600. */
  storageDir: string;
  /** Defaults to the current OS and node version. */
  platform?: { type?: string; version?: string };
  /** Rendering/prompting surface. Defaults to a stderr terminal presenter. */
  presenter?: HostPresenter;
  theme?: "Dark" | "Light";
  /** BCP 47 language tag for products. Defaults to the process locale. */
  locale?: string;
  /**
   * Core `tracing` verbosity. Defaults to `warn` because the core's
   * SSO-timeout diagnosis is ONLY visible at `warn` or above today.
   */
  logLevel?: string;
  /** Optional preimage retrieval backend. Default: always a miss. */
  lookupPreimage?: (key: Uint8Array) => Promise<Uint8Array | undefined>;
  /** Leases per pooled socket; see chain-pool. */
  maxLeasesPerSocket?: number;
  /** Override the `@parity/truapi-host` dist directory (exotic setups). */
  wasmDir?: string;
  /** Diagnostics sink for non-user-facing host events. */
  log?: (line: string) => void;
}

export interface CliHostProduct {
  productId: string;
  /**
   * The product side of the wire. Feed it to `@parity/truapi`'s
   * `createTransport`/`createClient` (or product-sdk's injection seam).
   */
  provider: WireProvider;
  /** Core-owned logout via this product runtime; clears product storage. */
  disconnectSession(): Promise<void>;
  dispose(): void;
}

export interface CliHost {
  /** Instantiate one product core over an in-process loopback wire. */
  createProduct(options: { productId: string }): CliHostProduct;
  /** Cancel an in-flight pairing (the user gave up on the QR). */
  cancelPairing(): void;
  /** Core-owned logout, then clears product storage (keys carry no account). */
  disconnectSession(): Promise<void>;
  /** Tell the core the persisted auth-session blob may have changed. */
  notifySessionStoreChanged(): void;
  /**
   * The last auth state the core emitted, or `undefined` before the first
   * emission. The core emits NOTHING at boot when unauthenticated. Render
   * logged-out from absence, do not wait for a state.
   */
  authState(): AuthState | undefined;
  onAuthState(listener: (state: AuthState) => void): () => void;
  waitForAuthState(
    predicate: (state: AuthState) => boolean,
    timeoutMs?: number,
  ): Promise<AuthState>;
  /**
   * Wipe core-namespaced product storage. Called automatically on logout and
   * when a DIFFERENT identity connects (product-storage keys are scoped by
   * product id only, so the next identity would inherit the previous one's
   * data).
   */
  clearProductStorage(): Promise<void>;
  storagePaths: { core: string; product: string };
  dispose(): void;
}

function defaultPlatformType(): string {
  switch (process.platform) {
    case "darwin":
      return "macOS";
    case "linux":
      return "Linux";
    case "win32":
      return "Windows";
    default:
      return process.platform;
  }
}

const LAST_IDENTITY_KEY = "lastConnectedIdentity";

export async function createCliHost(config: CliHostConfig): Promise<CliHost> {
  const core = await loadWasmCore({ wasmDir: config.wasmDir });
  core.bindings.setLogLevel(config.logLevel ?? "warn");

  const coreStore = new FileKeyValueStore(
    join(config.storageDir, "core-storage.json"),
  );
  const productStore = new FileKeyValueStore(
    join(config.storageDir, "product-storage.json"),
  );
  // Host-private bookkeeping (NOT a core storage slot): which identity last
  // connected, so a crash between logout and login still cannot leak one
  // identity's product storage to the next.
  const metaStore = new FileKeyValueStore(
    join(config.storageDir, "host-meta.json"),
  );

  const pool = createChainPool({
    endpoints: config.chains,
    maxLeasesPerSocket: config.maxLeasesPerSocket,
    log: config.log,
  });

  const ownsPresenter = config.presenter === undefined;
  const presenter = config.presenter ?? createTerminalPresenter();

  let lastAuthState: AuthState | undefined;
  const authListeners = new Set<(state: AuthState) => void>();
  // Identity checks are async but auth emissions are ordered. Serialize the
  // reactions so a Connected/Disconnected flurry cannot interleave clears.
  let identityChain: Promise<void> = Promise.resolve();

  const dispatchAuthState = (state: AuthState): void => {
    lastAuthState = state;
    if (state.tag === "Connected") {
      const identity = state.value.publicKey;
      identityChain = identityChain
        .catch(() => {})
        .then(async () => {
          const previous = await metaStore.get(LAST_IDENTITY_KEY);
          if (previous !== null && previous !== identity) {
            config.log?.(
              "different identity connected; clearing product storage",
            );
            await productStore.clear();
          }
          await metaStore.set(LAST_IDENTITY_KEY, identity);
        });
    }
    presenter.authStateChanged(state);
    for (const listener of [...authListeners]) {
      listener(state);
    }
  };

  const callbacks = createHostCallbacks({
    coreStore,
    productStore,
    pool,
    presenter,
    endpoints: config.chains,
    network: config.network,
    theme: config.theme ?? "Dark",
    locale: config.locale ?? Intl.DateTimeFormat().resolvedOptions().locale,
    lookupPreimage: config.lookupPreimage,
    onAuthState: dispatchAuthState,
    // Product-storage operations wait for any in-flight identity check, so a
    // product racing a Connected-with-new-identity emission can never read
    // the previous identity's entries before the clear lands.
    productStorageGate: () => identityChain,
    log: config.log,
  });

  // Since engine 0.16 the runtime expects one callback OUTSIDE the typed
  // platform surface: `workerDemandChanged`, the core's signal to start or
  // stop a product's worker. Workers are how browsers deploy product cores.
  // This host runs the core in-process, so the demand is acknowledged as a
  // no-op.
  const rawCallbacks = {
    ...(core.createRawCallbacks(callbacks) as Record<string, unknown>),
    workerDemandChanged: (productId: string, transition: string) => {
      config.log?.(`workerDemandChanged(${productId}, ${transition})`);
    },
  };
  const runtime = new core.bindings.WasmPairingHostRuntime(rawCallbacks, {
    host: config.host,
    platform: {
      type: config.platform?.type ?? defaultPlatformType(),
      version: config.platform?.version ?? process.version,
    },
    people: { genesisHash: config.people.genesisHash },
    bulletin: { genesisHash: config.bulletin.genesisHash },
    assetHub: { genesisHash: config.assetHub.genesisHash },
    pairing: { deeplinkScheme: config.pairing.deeplinkScheme },
  });

  const products = new Set<CliHostProduct>();
  let disposed = false;

  const clearProductStorage = async (): Promise<void> => {
    await productStore.clear();
  };

  return {
    createProduct({ productId }) {
      const { provider, core: productCore } = createLoopbackProvider(
        (coreCallbacks) => runtime.productRuntime({ productId }, coreCallbacks),
        {
          onReceiveError: (error) => {
            config.log?.(`receiveFrame(${productId}) failed: ${String(error)}`);
          },
        },
      );
      const product: CliHostProduct = {
        productId,
        provider,
        async disconnectSession() {
          await productCore.disconnectSession();
          await clearProductStorage();
        },
        dispose() {
          products.delete(product);
          provider.dispose();
          productCore.dispose();
        },
      };
      products.add(product);
      return product;
    },

    cancelPairing() {
      runtime.cancelPairing();
    },

    async disconnectSession() {
      await runtime.disconnectSession();
      await clearProductStorage();
    },

    notifySessionStoreChanged() {
      runtime.notifySessionStoreChanged();
    },

    authState() {
      return lastAuthState;
    },

    onAuthState(listener) {
      authListeners.add(listener);
      return () => authListeners.delete(listener);
    },

    waitForAuthState(predicate, timeoutMs) {
      if (lastAuthState !== undefined && predicate(lastAuthState)) {
        return Promise.resolve(lastAuthState);
      }
      return new Promise((resolve, reject) => {
        const timer =
          timeoutMs === undefined
            ? undefined
            : setTimeout(() => {
                authListeners.delete(listener);
                reject(
                  new Error(
                    `timed out after ${String(timeoutMs)}ms waiting for an auth state`,
                  ),
                );
              }, timeoutMs);
        const listener = (state: AuthState): void => {
          if (!predicate(state)) {
            return;
          }
          authListeners.delete(listener);
          if (timer !== undefined) {
            clearTimeout(timer);
          }
          resolve(state);
        };
        authListeners.add(listener);
      });
    },

    clearProductStorage,

    storagePaths: {
      core: coreStore.filePath,
      product: productStore.filePath,
    },

    dispose() {
      if (disposed) {
        return;
      }
      disposed = true;
      for (const product of [...products]) {
        product.dispose();
      }
      runtime.free();
      pool.closeAll();
      if (ownsPresenter) {
        presenter.dispose();
      }
    },
  };
}
