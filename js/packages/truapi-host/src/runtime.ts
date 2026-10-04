import type {
  HostChatActionSubscribeItem,
  HostRendererActionSubscribeItem,
  ProductRendererRenderRequest,
  RendererNode,
  WireProvider,
} from "@parity/truapi";
import { ReceivingEvent, ReceivingWatch } from "@parity/truapi";
import { Option, Vector } from "@parity/truapi/scale";
import {
  ReceivingAuthority,
  ReceivingRegistration,
} from "./generated/host-callbacks.js";
import { CoreStorageKey as GeneratedCoreStorageKey } from "./generated/host-callbacks.js";
import type {
  CoreAdmin,
  CoreStorageKey,
  ProductExecutionKind,
} from "./generated/host-callbacks.js";

// The typed capability interfaces below come straight from the
// `truapi::platform` Rust module via `truapi-codegen --platform-ts-output`.
// They are the host-author-facing surface: each method takes/returns
// typed wrappers (`HostDevicePermissionRequest`, etc.) rather than raw
// SCALE bytes. The web worker pairing-host runtime adapts this typed surface
// into the byte-oriented callback bridge consumed by the WASM core.
export * from "./generated/host-callbacks.js";
export type { JsonRpcConnection as PlatformJsonRpcConnection } from "./generated/host-callbacks.js";

/** Encode a typed core-storage slot for hosts that need an opaque backing key. */
export function encodeCoreStorageKey(key: CoreStorageKey): Uint8Array {
  return GeneratedCoreStorageKey.enc(key);
}

/** Authenticated, ready native Chat peer. Host-private; never a product directory. */
export interface NativeChatContact {
  /** Canonical lowercase 0x-prefixed People identity account. */
  peerIdentity: string;
  /** Verified roster name, absent when authorized products disagree. */
  username?: string;
}

/** Current signing wallet's authenticated native Chat contacts, restored from storage. */
export interface NativeChatContactsSnapshot {
  /** Canonical lowercase 0x-prefixed root wallet public key. */
  walletPublicKey: string;
  /** Canonical lowercase 0x-prefixed People chain genesis hash. */
  genesisHash: string;
  /** Deterministically ordered, deduplicated ready peers. */
  contacts: NativeChatContact[];
}

/**
 * Async-or-sync return. Synchronous hosts (e.g. the dotli main-thread
 * shell hitting localStorage) can return a plain value; the WASM bridge
 * awaits every return so an `async` impl also works.
 */
export type Awaitable<T> = T | Promise<T>;

/** Canonical SCALE results returned by resident receiving runtime hooks. */
export const receivingRegistrationsCodec = Vector(ReceivingRegistration);
export const receivingEventsCodec = Vector(ReceivingEvent);
export const receivingEventCodec = Option(ReceivingEvent);
const receivingWatchesCodec = Vector(ReceivingWatch);

/** Minimal host-owned callbacks for a wallet-free service-worker receiver. */
export interface NotificationReceiverCallbacks {
  /** Resolve current verified artifact/account state, never product message claims. */
  receiverAuthority?(productId: string): Awaitable<ReceivingAuthority | undefined>;
  /** Separate receiving consent, not an OS permission or relay acknowledgement. */
  receiverConsent?(authority: ReceivingAuthority, watches: ReceivingWatch[]): Awaitable<boolean>;
  /** Wake asynchronous transport work; never await remote synchronization. */
  receiverChanged?(): Awaitable<void>;
  readReceivingState(): Awaitable<Uint8Array | undefined>;
  /** Atomically replace the private ledger. Only one receiver may write it. */
  writeReceivingState(bytes: Uint8Array): Awaitable<void>;
}

/** Encode only the canonical domain records at the standalone WASM boundary. */
export function createNotificationReceiverCallbacks(callbacks: NotificationReceiverCallbacks) {
  return {
    receiverAuthority: async (productId: string) => {
      const authority = await callbacks.receiverAuthority?.(productId);
      return authority === undefined ? undefined : ReceivingAuthority.enc(authority);
    },
    receiverConsent: async (authority: Uint8Array, watches: Uint8Array) => {
      if (!callbacks.receiverConsent) throw new Error("background receiving unsupported");
      return callbacks.receiverConsent(ReceivingAuthority.dec(authority), receivingWatchesCodec.dec(watches));
    },
    receiverChanged: async () => {
      if (!callbacks.receiverChanged) throw new Error("background receiving unsupported");
      return callbacks.receiverChanged();
    },
    readReceivingState: async () => callbacks.readReceivingState(),
    writeReceivingState: async (bytes: Uint8Array) => callbacks.writeReceivingState(bytes),
  };
}

/** Raw host-only receiving hooks on both full resident and standalone WASM cores.
 * Products must use Notifications actions, never these host-authority hooks.
 */
export interface RawReceivingRuntime {
  receivingPending(): Promise<Uint8Array>;
  receivingSynchronized(productId: string, revision: bigint): Promise<boolean>;
  receivingIngest(
    productId: string, revision: bigint, watchId: string,
    actualGenesis: string, actualChannel: string, actualTopics: string[], frame: Uint8Array,
  ): Promise<Uint8Array>;
  receivingIngestStatement(
    productId: string, revision: bigint, watchId: string,
    actualGenesis: string, statement: Uint8Array,
  ): Promise<Uint8Array>;
  receivingPrepareDisplay(productId: string, revision: bigint, eventId: string): Promise<Uint8Array>;
  receivingConfirmDisplay(productId: string, revision: bigint, eventId: string): Promise<void>;
  /** Clear a reservation after explicit display failure, never after an unknown outcome. */
  receivingCancelDisplay(productId: string, revision: bigint, eventId: string): Promise<void>;
  receivingValidateActivation(productId: string, revision: bigint, eventId: string): Promise<Uint8Array>;
  receivingActivate(productId: string, revision: bigint, eventId: string): Promise<Uint8Array>;
  receivingRevoke(productId: string): Promise<void>;
  receivingMarkTransportChanged(productId: string): Promise<void>;
}

/** Standalone commands carry the immutable authority captured by the trusted execution channel.
 * Authority bytes encode ReceivingAuthority; response is Result<latest response,HostNotificationReceivingError>.
 */
export interface RawNotificationReceiver extends RawReceivingRuntime {
  commandForExecution(authority: Uint8Array, action: number, payload: Uint8Array): Promise<Uint8Array>;
  free(): void;
}

/**
 * Open a JSON-RPC connection for `genesisHash`. The wasm bridge passes
 * `onResponse` so the host can push JSON-RPC replies back asynchronously.
 * Returning `null` (or throwing) tells the core no provider is available.
 * `onClosed`, when supplied, is called when the remote response stream ends
 * or fails. Local `close()` is idempotent and does not call it.
 */
export type ChainConnect = (
  genesisHash: string,
  onResponse: (json: string) => void,
  onClosed?: () => void,
) => Awaitable<ChainConnection | null>;

/** Open only a host-allowlisted HOP endpoint for this Bulletin chain. */
export type HopConnect = (
  bulletinGenesisHash: string,
  endpoint: string,
  onResponse: (json: string) => void,
  onClosed?: () => void,
) => Awaitable<ChainConnection | null>;

/**
 * Per-connection handle returned by `chainConnect` or `hopConnect`. `send`
 * forwards a JSON-RPC request string; `close` tears the connection down.
 */
export interface ChainConnection {
  send(request: string): void;
  close(): void;
}

/**
 * Verbosity threshold for the wasm core's `tracing` output. The Rust core
 * parses the string; known values are `off`, `error`, `warn`, `info`, `debug`,
 * and `trace`.
 */
export type LogLevel = string;

/** Configuration for one product runtime hosted by the wasm core. */
export interface ProductRuntimeConfig {
  /** Stable identifier used to scope product accounts, permissions, and storage. */
  productId: string;
  /** Trusted executable kind selected by the host; defaults to `App`. */
  executionKind?: ProductExecutionKind;
  /** Metadata describing the host application. */
  host: {
    /** Human-readable host name. */
    name: string;
    /** Host icon URL. */
    icon?: string;
    /** Host application version. */
    version?: string;
    /**
     * Platform category the host runs on, reported to products via
     * `System.host_info`. Hosts that omit it report `"Unknown"`.
     */
    platform?: "Web" | "Android" | "Ios" | "Desktop" | "Cli" | "Unknown";
  };
  /** Metadata describing the platform running the host. */
  platform?: {
    /** Platform or operating-system name. */
    type?: string;
    /** Platform or operating-system version. */
    version?: string;
  };
  /** People-chain configuration used for statement-store SSO. */
  people: {
    /** People-chain genesis hash. */
    genesisHash: string | Uint8Array;
  };
  /** Bulletin-chain configuration used for in-core preimage submission. */
  bulletin: {
    /** Bulletin-chain genesis hash. */
    genesisHash: string | Uint8Array;
  };
  /**
   * Asset Hub configuration. Session usernames and the product manifests
   * that carry `trustedProducts` grants are both read from the dotNS
   * contracts deployed there. Without a usable genesis hash no manifest
   * resolves, so every cross-product grant not already cached is refused,
   * indistinguishably from the other product having granted nothing. An
   * all-zero hash declares deliberately that this host has no Asset Hub.
   *
   * The wasm signing host requires the same `assetHub.genesisHash`, though it
   * takes an untyped config object rather than this interface.
   */
  assetHub: {
    /** Asset Hub genesis hash. */
    genesisHash: string | Uint8Array;
  };
  /** Wallet pairing configuration. */
  pairing: {
    /** URI scheme used for wallet pairing deeplinks. */
    deeplinkScheme: string;
  };
  /**
   * dotNS TLD the host's reserved identities derive under, such as `"dot"` or
   * `"paseo"`.
   *
   * Required by a signing host, which derives its own keys and so has to know
   * which network's identities it is deriving. A pairing host never derives
   * them -- the paired wallet does -- and ignores this.
   */
  networkSuffix?: string;
}

/**
 * Sink for one render. `onUpdate` receives a complete replacement tree each
 * time; there is no patching.
 */
export interface RenderSink {
  onUpdate(node: RendererNode): void;
  /**
   * The render ended cleanly and the last tree delivered stands. Exactly one
   * of `onComplete` or `onError` fires per render.
   */
  onComplete?(): void;
  /**
   * The render failed and any tree already delivered is partial, so it must
   * not be left on screen as final. Covers a product that declined or could
   * not encode a tree, a connection that may not reach the renderer or has
   * closed, and a tree the host's own codec or renderer rejected.
   */
  onError?(error: Error): void;
}

/**
 * Demand on one product's worker crossed zero. `wanted: true` when the first
 * reference formed, so the host runs the worker; `wanted: false` when the last
 * one left, so the host may stop it.
 */
export interface WorkerDemandChange {
  productId: string;
  wanted: boolean;
}

export interface TrUApiProductProvider extends WireProvider, CoreAdmin {
  /**
   * Re-tune the wasm core's log level at runtime. Present on runtimes that
   * keep a live channel to the core (e.g. the Web Worker provider); absent on
   * one-shot constructions that only accept `logLevel` up front.
   */
  setLogLevel?(level: LogLevel): void;

  /**
   * Publish one host-authored Chat action into the product's action stream —
   * the path a posted message, a command, or a host-drawn `Actions` button
   * takes back to the product. Buffered until the product subscribes. Rejects
   * when this connection may not reach Chat.
   *
   * Present only on runtimes that keep a live channel to the core.
   */
  publishChatAction?(action: HostChatActionSubscribeItem): Promise<void>;

  /**
   * Publish one action triggered inside a product-rendered body. Buffered
   * until the product subscribes. Rejects when this connection may not reach
   * the product's renderer.
   *
   * Present only on runtimes that keep a live channel to the core.
   */
  publishRendererAction?(item: HostRendererActionSubscribeItem): Promise<void>;

  /**
   * Ask the product to draw one body, streaming replacement trees until the
   * returned disposer is called. Reports failure through `sink.onError` rather
   * than throwing, so a dead render never takes the host's surrounding surface
   * with it.
   *
   * An open render holds one worker reference for the provider's product: the
   * runtime acquires it when the stream starts and releases it when the stream
   * ends or the disposer runs.
   *
   * Present only on runtimes that keep a live channel to the core.
   */
  render?(request: ProductRendererRenderRequest, sink: RenderSink): () => void;
}
