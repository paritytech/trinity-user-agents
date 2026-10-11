// Shape of the web-targeted truapi WASM bundle. `make wasm` writes the
// wasm-pack glue and its `.wasm` payload to `dist/wasm/web/`; the ambient
// declaration in `src/wasm/web/truapi_server.d.ts` types that module against
// these interfaces so the worker can name it in a statically analysable import.

import type { PermissionAuthorizationRuntime } from "./worker-permission-authorization.js";
import type { LocalIdentity } from "./worker-protocol.js";
import type { WalletAllowanceSnapshot } from "./wallet-allowances.js";
import type { NativeChatContactsSnapshot } from "./runtime.js";
import type { RawNotificationReceiver, RawReceivingRuntime } from "./runtime.js";

/** Cancellable handle on one live render stream inside the core. */
export interface WorkerRendererSubscription {
  cancel(): void;
  free(): void;
}

/** One product-scoped core inside the worker. */
export interface WorkerProductRuntime {
  receiveFrame(frame: Uint8Array): Promise<void>;
  /** Drop cached contact handles after a contact is removed or blocked. */
  notifyContactsChanged(): void;
  dispose(): void;
  free(): void;
  /** Throws when the connection may not reach Chat. */
  publishChatAction(action: Uint8Array): void;
  /**
   * Publish one action triggered inside a product-rendered body, as a
   * SCALE-encoded `HostRendererActionSubscribeItem`. Throws when the
   * connection may not reach the product's renderer.
   */
  publishRendererAction(item: Uint8Array): void;
  /**
   * Start the host-initiated render subscription for one body. `request` is a
   * SCALE-encoded `ProductRendererRenderRequest`. `onUpdate` receives each
   * SCALE-encoded `RendererNode`, then exactly one of `onComplete` (last tree
   * stands) or `onError` (the product could not serve the render; the last
   * tree is partial). Terminals never arrive during the call itself; a request
   * the core refuses outright throws instead.
   */
  render(
    request: Uint8Array,
    onUpdate: (node: Uint8Array) => void,
    onComplete: () => void,
    onError: (reason: string) => void,
  ): WorkerRendererSubscription;
}

/** What the host does with a product's worker after demand on it changed. */
export type WorkerTransition = "Start" | "Stop";

/** Runtime operations shared by paired and browser-local signing hosts. */
export interface WorkerHostRuntime extends PermissionAuthorizationRuntime, RawReceivingRuntime {
  productRuntime(
    product: unknown,
    coreCallbacks: unknown,
    platformCallbacks?: unknown,
  ): WorkerProductRuntime;
  disconnectSession(): Promise<void>;
  notifyContactsChanged(): void;
  sessionChatIdentityKey(): Uint8Array | undefined;
  deviceStatementKey(): Uint8Array | undefined;
  deviceEncryptionKey(): Promise<Uint8Array>;
  productSubtreePublicKey(
    productId: string,
    timeoutMs?: number,
  ): Promise<Uint8Array | undefined>;
  clearProductState(productId: string): Promise<void>;
  /** Only on a core built with `test-host`. */
  setSubmitPreimagesLocally?(local: boolean): void;
  /**
   * Take one reference on the product's worker. The first one reports
   * `"Start"` through the runtime's `workerDemandChanged` callback.
   */
  acquireWorker(productId: string): void;
  /**
   * Release one reference. The last one reports `"Stop"` the same way.
   */
  releaseWorker(productId: string): void;
  free(): void;
}

/** The long-lived pairing-host runtime product cores are created from. */
export interface WorkerPairingHostRuntime extends WorkerHostRuntime {
  getNativeChatContacts(): Promise<NativeChatContactsSnapshot>;
  cancelPairing(): void;
  notifySessionStoreChanged(): void;
  activateStoredSession(): Promise<void>;
  activateExternalSession(blob: Uint8Array): Promise<void>;
  resetSessionState(): Promise<void>;
}

/**
 * A browser-local signing host activated from caller-owned entropy.
 *
 * A signing host owns the user's keys and establishes sessions from local
 * entropy rather than by pairing with a wallet. It is present only in a core
 * built with `wasm-signing-host`: the testing bundle or a production `web`
 * bundle built with `--signing-host`. The constructor is otherwise absent.
 */
export interface WorkerSigningHostRuntime extends WorkerHostRuntime {
  activateLocalSession(secret: Uint8Array): Promise<void>;
  /**
   * Activate and give the session a display name, which is what
   * `account.get_user_id` answers with.
   */
  activateLocalSessionWithIdentity(
    secret: Uint8Array,
    liteUsername?: string,
  ): Promise<void>;
  /** Only on a core built with the non-production `test-host` feature. */
  setGrantAllowancesUnchecked?(granted: boolean): void;
  localIdentityContext(): { activationId: string; identityAccountId: string };
  localIdentityAuthProof(
    activationId: string,
    challenge: Uint8Array,
  ): Uint8Array;
  localLiteRegistrationBody(
    activationId: string,
    usernameBase: string,
    verifier: Uint8Array,
  ): Promise<string>;
  refreshLocalIdentity(activationId: string): Promise<LocalIdentity>;
  getWalletAllowanceSnapshot(
    activationId: string,
    productIds: string[],
  ): Promise<WalletAllowanceSnapshot>;
  getNativeChatContacts(): Promise<NativeChatContactsSnapshot>;
  /** Only on a core built with the non-production `test-host` feature. */
  setWithheldResources?(tags: string[]): void;
}

/** Module surface the wasm-pack glue exports. */
export interface WasmModuleShape {
  default: (input?: unknown) => Promise<unknown>;
  /** Wallet-free receiver for the host's single durable service-worker owner. */
  WasmNotificationReceiver: new (callbacks: unknown) => RawNotificationReceiver;
  WasmPairingHostRuntime: new (
    callbacks: unknown,
    hostConfig: unknown,
  ) => WorkerPairingHostRuntime;
  /** Only with `wasm-signing-host`; see {@link WorkerSigningHostRuntime}. */
  WasmSigningHostRuntime?: new (
    callbacks: unknown,
    hostConfig: unknown,
  ) => WorkerSigningHostRuntime;
  WasmProductRuntime: new (
    callbacks: unknown,
    runtimeConfig: unknown,
  ) => WorkerProductRuntime;
  setLogLevel?: (level: string) => void;
  /**
   * Derive a product account public key from that product's hard-subtree
   * public key and a SCALE-encoded `DerivationIndex`. Pure: no runtime or
   * session needed, so a host can call it after `default()` alone.
   */
  deriveProductAccountPublicKey: (
    productSubtreePublicKey: Uint8Array,
    derivationIndex: Uint8Array,
  ) => Uint8Array;
  /** SS58 address for a product account public key, at the core's prefix. */
  productAccountAddress: (publicKey: Uint8Array) => string;
  /**
   * The core's own `TRUAPI_WIRE_SCHEMA_HASH`, exported by `truapi`'s wasm
   * bridge. Optional because `dist/wasm/web/` is gitignored and built by hand, so
   * a stale bundle predating the export is a normal state to find at runtime; a
   * core that cannot vouch for its table streams frames without a `schema` stamp
   * and the debugger groups them without decoding.
   */
  wireSchemaHash?: () => string;
}
