// Shape of the web-targeted truapi WASM bundle. `make wasm` writes the
// wasm-pack glue and its `.wasm` payload to `dist/wasm/web/`; the ambient
// declaration in `src/wasm/web/truapi_server.d.ts` types that module against
// these interfaces so the worker can name it in a statically analysable import.

import type { PermissionAuthorizationRuntime } from "./worker-permission-authorization.js";

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

/** Runtime operations shared by paired and browser-local signing hosts. */
export interface WorkerHostRuntime extends PermissionAuthorizationRuntime {
  productRuntime(
    product: unknown,
    coreCallbacks: unknown,
  ): WorkerProductRuntime;
  disconnectSession(): Promise<void>;
  sessionChatIdentityKey(): Uint8Array | undefined;
  deviceStatementKey(): Uint8Array | undefined;
  deviceEncryptionKey(): Promise<Uint8Array>;
  productSubtreePublicKey(
    productId: string,
    timeoutMs?: number,
  ): Promise<Uint8Array | undefined>;
  clearProductState(productId: string): Promise<void>;
  free(): void;
}

/** The long-lived pairing-host runtime product cores are created from. */
export interface WorkerPairingHostRuntime extends WorkerHostRuntime {
  cancelPairing(): void;
  notifySessionStoreChanged(): void;
  activateStoredSession(): Promise<void>;
  activateExternalSession(blob: Uint8Array): Promise<void>;
  resetSessionState(): Promise<void>;
}

/** A browser-local signing host activated from caller-owned entropy. */
export interface WorkerSigningHostRuntime extends WorkerHostRuntime {
  activateLocalSession(secret: Uint8Array): Promise<void>;
  activateLocalSessionWithIdentity(
    secret: Uint8Array,
    liteUsername?: string,
  ): Promise<void>;
}

/** Module surface the wasm-pack glue exports. */
export interface WasmModuleShape {
  default: (input?: unknown) => Promise<unknown>;
  WasmPairingHostRuntime: new (
    callbacks: unknown,
    hostConfig: unknown,
  ) => WorkerPairingHostRuntime;
  WasmSigningHostRuntime: new (
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
