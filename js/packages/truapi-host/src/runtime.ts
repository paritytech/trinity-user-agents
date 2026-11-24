import type { ExecutableKind, ProductId } from "@parity/truapi";
import type {
  HostChatActionSubscribeItem,
  HostRendererActionSubscribeItem,
  ProductRendererRenderRequest,
  RendererNode,
  WireProvider,
} from "@parity/truapi";
import { CoreStorageKey as GeneratedCoreStorageKey } from "./generated/host-callbacks.js";
import type { CoreAdmin, CoreStorageKey } from "./generated/host-callbacks.js";

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

/**
 * A host build's policy on reserved values for its stack. The core reserves
 * Polkadot's genesis hashes and the `dot` TLD. A build can reserve more here
 * and cannot reserve fewer. A stack that names a reserved value, the core's or
 * the build's, under any chain role starts only when `allowed` is set. Extra
 * values only protect networks the build does not run: `allowed` cannot be set
 * alongside them, so the build's own stack cannot name them.
 *
 * The core refuses a policy that would reserve less than it says: a field this
 * type does not have, an entry it cannot read, such as a genesis hash that is
 * not 32 bytes or a TLD that is not a string, `allowed` together with extra
 * values, which the allowed stack may name anyway, and an extra TLD that no
 * stack can have. It checks the policy after it reads the other fields of
 * `stack` or `testStack`, so one of those that is missing, unknown or
 * malformed is reported first. It checks the policy before the stack itself:
 * before it reads the key from a `stack`'s pointer link and verifies its
 * descriptor, and before it checks a `testStack`'s identity, TLD and genesis
 * hashes against the core's rules.
 */
export interface ReservedValuesPolicy {
  /**
   * Whether the stack may name reserved values. Only a Polkadot or
   * Polkadot-fork build sets it, and then reserves no extra values. Defaults
   * to `false`.
   */
  allowed?: boolean;
  /**
   * Genesis hashes reserved on top of the core's, as hex or bytes. The `0x`
   * prefix is optional.
   */
  extraGenesisHashes?: (string | Uint8Array)[];
  /**
   * dotNS TLDs reserved on top of the core's, each written as a stack's TLD
   * is, without the leading dot. The core's refusal of any other entry names
   * the TLDs a stack can have.
   */
  extraTlds?: string[];
}

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
  /** The build's policy on reserved values. Defaults to refusing the core's. */
  reservedValues?: ReservedValuesPolicy;
}

/** A fetch of the stack's descriptor for the host to make. */
export interface StackFetchRequest {
  /** The descriptor's URL. Send a `GET` and do not follow redirects. */
  url: string;
  /** The `ETag` to send as `If-None-Match`, when the core holds one. */
  ifNoneMatch?: string;
  /**
   * The largest body to read. Stop reading and throw once a body passes it.
   */
  maxBytes: number;
}

/**
 * What a fetch of the stack's descriptor got. Anything but these two is a
 * failed fetch, which the callback throws: a transport error, a timeout, a
 * redirect, another status or a body larger than `maxBytes`.
 */
export type StackFetchResponse =
  /** `200 OK`: the body as served and the response's `ETag`. */
  | { status: "document"; bytes: Uint8Array; etag?: string }
  /** `304 Not Modified`. */
  | { status: "notModified" };

/** The stack the core runs, and what the host shows and does about it. */
export interface StackStatus {
  /**
   * The verified descriptor the core runs, as plain descriptor JSON. The
   * host's own features read their chains and services from it.
   */
  descriptor: string;
  /**
   * The descriptor's `generation`, which changes when the stack restarts its
   * chains. Key cached chain data by it, such as a light client's database:
   * data kept under another generation is for chains that are gone.
   */
  generation: string;
  /**
   * Past `validUntil` when the core last looked, at start and after each
   * fetch: show the stack as stale and keep running on it.
   */
  stale: boolean;
  /**
   * Past `expiresAt` when the core last looked, at start and after each
   * fetch: the stack has shut down.
   */
  expired: boolean;
  /**
   * The core stored a newer copy that changes a chain, the network or the
   * generation. It runs that copy from its next start, so build the runtime
   * again to follow it. A copy on another TLD is refused and never sets it.
   */
  restartRequired: boolean;
  /** The last finished fetch. Not set before the first one. */
  lastRefresh?: {
    /**
     * `current`: the published descriptor is the copy the core holds.
     * `unreachable` and `refused`: the core keeps the copy it holds.
     * `notStored`: the published descriptor is newer, but core storage did
     * not keep it, so the core asks for no restart and stores it after the
     * next fetch.
     */
    outcome: "current" | "unreachable" | "refused" | "notStored";
    /**
     * Why a fetch was `unreachable` or `notStored`, or the rule a `refused`
     * descriptor breaks.
     */
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
   * Fetch the stack's descriptor over HTTP. Throw or reject when the server
   * gave no usable answer: the core keeps its copy and reports
   * `unreachable`. The core gives up after 30 seconds. It asks once its
   * copy is an hour old, which is at start unless the copy it stored was
   * fetched less than an hour ago.
   */
  fetchStackDescriptor?(
    request: StackFetchRequest,
  ): StackFetchResponse | Promise<StackFetchResponse>;
  /**
   * The stack's status when the runtime starts, and on each change. A host
   * without `fetchStackDescriptor` gets the one at start. A runtime built
   * from a test stack has no status and calls neither callback.
   */
  stackChanged?(status: StackStatus): void;
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
   * Present only on runtimes that keep a live channel to the core.
   */
  render?(request: ProductRendererRenderRequest, sink: RenderSink): () => void;
}
