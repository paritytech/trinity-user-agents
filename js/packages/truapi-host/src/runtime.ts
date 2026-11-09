import type { ProductId } from "@parity/truapi";
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
  ExecutableKind,
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

/** The Trinity stack a host build bundles. */
export interface BundledStack {
  /**
   * The stack's link, `polkadotapp://stack?key=0x…&url=…`. It must carry a key:
   * the core refuses an unsigned stack.
   */
  pointer: string;
  /**
   * The bundled descriptor's bytes as served: a signed envelope that verifies
   * under the pointer's key.
   */
  descriptor: Uint8Array;
  /**
   * Whether this build may run a stack that uses Polkadot's genesis hashes or
   * the `dot` TLD. Only a Polkadot or Polkadot-fork build sets it. Defaults to
   * `false`.
   */
  allowsReservedValues?: boolean;
  /**
   * The record the core last passed to {@link StackCallbacks.storeStackRecord}.
   * The core starts from the descriptor in it when that is newer than
   * `descriptor`, so a stack that restarted since the build still starts.
   */
  stored?: Uint8Array;
}

/** A fetch of the stack's descriptor for the host to make. */
export interface StackFetchRequest {
  /** The descriptor's URL. Send a `GET` and do not follow redirects. */
  url: string;
  /** The `ETag` to send as `If-None-Match`, when the core holds one. */
  ifNoneMatch?: string;
  /**
   * The largest body to read. Stop reading and answer `unreachable` once a
   * body passes it.
   */
  maxBytes: number;
}

/** What a fetch of the stack's descriptor got. */
export type StackFetchResponse =
  /** `200 OK`: the body as served and the response's `ETag`. */
  | { status: "document"; bytes: Uint8Array; etag?: string }
  /** `304 Not Modified`. */
  | { status: "notModified" }
  /**
   * Anything else: a transport error, a timeout, a redirect, another status
   * or a body larger than `maxBytes`. `reason` goes to the core's log.
   */
  | { status: "unreachable"; reason?: string };

/** The stack the core runs, and what the host shows and does about it. */
export interface StackStatus {
  /**
   * The verified descriptor the core runs, as plain descriptor JSON. The
   * host's own features read their chains and services from it, so they run
   * the same stack as the core.
   */
  descriptor: string;
  /** Past `validUntil`: show the stack as stale and keep running on it. */
  stale: boolean;
  /** Past `expiresAt`: the stack has shut down. */
  expired: boolean;
  /**
   * The core holds a newer copy that changes a chain, the network or the TLD.
   * It runs that copy from its next start, so build the runtime again to
   * follow it.
   */
  restartRequired: boolean;
  /**
   * The stack restarted its chains in that newer copy: drop cached chain
   * data, such as a light client's database, before the next start.
   */
  newGeneration: boolean;
  /** The last finished fetch. Not set before the first one. */
  lastRefresh?: {
    /**
     * `current`: the published descriptor is the copy the core holds.
     * `unreachable` and `refused`: the core keeps the copy it holds.
     */
    outcome: "current" | "unreachable" | "refused";
    /** The rule a refused descriptor breaks. */
    reason?: string;
  };
}

/**
 * Raw callbacks a host passes to the wasm runtime, next to the generated
 * ones, to keep the stack's descriptor current. Without
 * `fetchStackDescriptor` the core stays on the copy it started from.
 */
export interface StackCallbacks {
  /**
   * Fetch the stack's descriptor over HTTP. The core gives up after 30
   * seconds. A rejection counts as `unreachable`.
   */
  fetchStackDescriptor?(
    request: StackFetchRequest,
  ): StackFetchResponse | Promise<StackFetchResponse>;
  /** The stack's status when the runtime starts, and on each change. */
  stackChanged?(status: StackStatus): void;
  /**
   * Keep `record` and pass it back as {@link BundledStack.stored} at the next
   * start. Each call replaces the record before it.
   */
  storeStackRecord?(record: Uint8Array): void | Promise<void>;
}

/** Configuration for one product runtime hosted by the wasm core. */
export interface ProductRuntimeConfig {
  /** Stable identifier used to scope product accounts, permissions, and storage. */
  productId: ProductId;
  /** Trusted executable kind selected by the host; defaults to `App`. */
  executionKind?: ExecutableKind;
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
  /**
   * The stack the host build runs. Every chain the core uses, the network
   * `getChainInfo` reports and the TLD a signing host derives its identities
   * under come from it.
   */
  stack: BundledStack;
  /** Wallet pairing configuration. */
  pairing: {
    /** URI scheme used for wallet pairing deeplinks. */
    deeplinkScheme: string;
  };
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
  productId: ProductId;
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
