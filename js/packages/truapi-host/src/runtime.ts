import type {
  HostChatActionSubscribeItem,
  HostRendererActionSubscribeItem,
  ProductRendererRenderRequest,
  RendererNode,
  WireProvider,
} from "@parity/truapi";
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

/**
 * Async-or-sync return. Synchronous hosts (e.g. the dotli main-thread
 * shell hitting localStorage) can return a plain value; the WASM bridge
 * awaits every return so an `async` impl also works.
 */
export type Awaitable<T> = T | Promise<T>;

/**
 * Open a JSON-RPC connection for `genesisHash`. The wasm bridge passes
 * `onResponse` so the host can push JSON-RPC replies back asynchronously.
 * Returning `null` (or throwing) tells the core no provider is available.
 */
export type ChainConnect = (
  genesisHash: string,
  onResponse: (json: string) => void,
) => Awaitable<ChainConnection | null>;

/**
 * Per-connection handle returned by `chainConnect`. `send` forwards a
 * SCALE-encoded JSON-RPC request; `close` tears the connection down.
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
