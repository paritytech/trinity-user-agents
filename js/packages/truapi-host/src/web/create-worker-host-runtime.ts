import type {
  ChainConnection,
  ProductRuntimeConfig,
  LogLevel,
  NativeChatContactsSnapshot,
  PermissionAuthorizationRequest,
  PermissionAuthorizationStatus,
  ProductExecutionKind,
  RequiredHostCallbacks,
  TrUApiProductProvider,
  WorkerDemandChange,
} from "../index.js";
import type {
  Bytes32,
  GenericError,
  HostChatActionSubscribeItem,
  HostRendererActionSubscribeItem,
  RendererNode,
} from "@parity/truapi";
import {
  HostChatActionSubscribeItem as HostChatActionSubscribeItemCodec,
  HostRendererActionSubscribeItem as HostRendererActionSubscribeItemCodec,
  HostWorkerBeginOperationResponse as HostWorkerBeginOperationResponseCodec,
  ProductRendererRenderRequest as ProductRendererRenderRequestCodec,
  RendererNode as RendererNodeCodec,
} from "@parity/truapi";
import {
  NativeChatPickedFile,
  PermissionAuthorizationRequest as PermissionAuthorizationRequestCodec,
  ProductContext as ProductContextCodec,
} from "../generated/host-callbacks.js";
import { createWasmRawCallbacks } from "../generated/host-callbacks-adapter.js";
import type { RawCallbacks } from "../generated/host-callbacks-adapter.js";
import { isLoopbackWsUrl } from "../worker-protocol.js";
import type {
  CallbackName,
  HostRole,
  LocalIdentity,
  LocalIdentityProgress,
  MainToWorker,
  SubscriptionName,
  WorkerToMain,
} from "../worker-protocol.js";
import {
  COINAGE_WALLET_CALLBACKS,
  MAX_JSON_RPC_CONNECTIONS,
} from "../worker-protocol.js";
import { bytesToHex, Vector } from "@parity/truapi/scale";
import { startRawSubscription } from "../generated/worker-callbacks.js";
import { errorMessage, toError } from "../error.js";
import { createBrowserNativeChatFilesHost } from "./native-chat-files.js";
import {
  validateAllowanceProductIds,
  type WalletAllowanceSnapshot,
} from "../wallet-allowances.js";

export type WebWorkerHostConfig = Omit<
  ProductRuntimeConfig,
  "productId" | "executionKind"
>;
export type WebWorkerSigningHostConfig = WebWorkerHostConfig & {
  /** Bare dotNS network suffix (`dot`, `paseo`, or `testnet`). */
  networkSuffix: string;
  /** Trusted u32 asset instance, required for instance-scoped Coinage runtimes. */
  coinageInstanceId?: number;
};

export interface WorkerPairingHostRuntime {
  /**
   * The encoding core's wire-schema hash, when the core reports one.
   *
   * An in-host debugger tap runs on this side of the worker boundary and has no
   * other way to reach it, so without this it can only stamp frames with the
   * page bundle's own constant — a different artifact from the core that
   * actually encoded them. The debugger then refuses to decode, exactly as it
   * should. Undefined for a core built before the export existed.
   */
  readonly coreWireSchemaHash: string | undefined;
  createProvider(
    product: {
      productId: string;
      executionKind?: ProductExecutionKind;
    },
    callbacks?: WebWorkerHostCallbacks,
  ): Promise<TrUApiProductProvider>;
  disconnectSession(): Promise<void>;
  cancelPairing(): void;
  notifySessionStoreChanged(): void;
  /**
   * Tell the core the host's contacts changed. Call it whenever a contact is
   * removed or blocked, so a contact handle the core cached stops resolving.
   */
  notifyContactsChanged(): void;
  /** Host-only native roster. Pairing hosts reject as unsupported. */
  getNativeChatContacts(): Promise<NativeChatContactsSnapshot>;
  /**
   * Restore the session persisted in the core's `AuthSession` slot. Resolves
   * once product frames may use it, so a host can await this at boot before
   * routing. Rejects when the runtime has been disposed or the worker faulted,
   * so a host never routes on an activation that did not run.
   */
  activateStoredSession(): Promise<void>;
  /**
   * Install an already-paired session the host holds itself, without copying
   * it into core storage. Rejects on a disposed runtime, as
   * {@link WorkerPairingHostRuntime.activateStoredSession} does.
   */
  activateExternalSession(blob: Uint8Array): Promise<void>;
  /**
   * Establish a session from host-held BIP-39 entropy.
   *
   * Signing hosts only. A pairing host has no local secret and rejects this:
   * it waits for a wallet to answer over the statement-store channel instead.
   */
  activateLocalSession(
    secret: Uint8Array,
    liteUsername?: string,
  ): Promise<void>;
  setGrantAllowancesUnchecked(granted: boolean): Promise<void>;
  /**
   * Answer these resource tags as refused, replacing any earlier set.
   *
   * Withholding one resource while the rest stay granted is what lets a suite
   * prove its product survives a refusal it cannot otherwise arrange.
   */
  setWithheldResources(tags: string[]): Promise<void>;
  /**
   * Drop the active paired session without notifying the peer. Rejects on a
   * disposed runtime, as
   * {@link WorkerPairingHostRuntime.activateStoredSession} does.
   */
  resetSessionState(): Promise<void>;
  getPermissionAuthorizationStatus(
    productId: string,
    request: PermissionAuthorizationRequest,
  ): Promise<PermissionAuthorizationStatus>;
  getPermissionAuthorizationStatuses(
    productId: string,
    requests: PermissionAuthorizationRequest[],
  ): Promise<PermissionAuthorizationStatus[]>;
  setPermissionAuthorizationStatus(
    productId: string,
    request: PermissionAuthorizationRequest,
    status: PermissionAuthorizationStatus,
  ): Promise<void>;
  refreshPermissionAuthorization(
    productId: string,
    request: PermissionAuthorizationRequest,
  ): Promise<void>;
  getSessionChatIdentityKey(): Promise<Uint8Array | undefined>;
  getDeviceStatementKey(): Promise<Uint8Array | undefined>;
  getDeviceEncryptionKey(): Promise<Uint8Array>;
  getProductSubtreePublicKey(
    productId: string,
    timeoutMs?: number,
  ): Promise<Uint8Array | undefined>;
  /**
   * Take one reference on a product's worker for a modality holder that is on
   * screen or in flight. Pair every call with one
   * {@link WorkerPairingHostRuntime.releaseWorker}. The core counts; the
   * resulting level reaches
   * {@link WorkerPairingHostRuntime.subscribeWorkerDemand} listeners, and the
   * host runs and stops the worker executable itself.
   */
  acquireWorker(productId: string): void;
  /** Release one reference. Releasing with none held is a no-op. */
  releaseWorker(productId: string): void;
  /**
   * Observe which product workers the host should run. The listener first
   * receives `wanted: true` for every product wanted right now, then each
   * change as it happens, and `wanted: false` for every remaining product
   * when the runtime is disposed. Returns the unsubscribe.
   */
  subscribeWorkerDemand(
    listener: (change: WorkerDemandChange) => void,
  ): () => void;
  setLogLevel(level: LogLevel): void;
  dispose(): void;
}
export interface WorkerSigningHostRuntime extends Omit<
  WorkerPairingHostRuntime,
  | "cancelPairing"
  | "notifySessionStoreChanged"
  | "activateStoredSession"
  | "activateExternalSession"
  | "resetSessionState"
> {
  activateLocalSession(secret: Uint8Array): Promise<void>;
  activateLocalSessionWithIdentity(
    secret: Uint8Array,
    liteUsername?: string,
  ): Promise<void>;
  /** Read dotNS ownership and install verified metadata into the native session. */
  refreshLocalIdentity(): Promise<LocalIdentity>;
  /** Read-only wallet-wide inspection bound to the native local activation. */
  getWalletAllowanceSnapshot(
    productIds: string[],
  ): Promise<WalletAllowanceSnapshot>;
  /** Complete native UID auth/proofs and wait for on-chain ownership confirmation. */
  registerLocalLiteUsername(
    baseUsername: string,
    identityBackendBaseUrl: string,
    onProgress?: (progress: LocalIdentityProgress) => void,
  ): Promise<LocalIdentity>;
}

interface CoreState {
  coreId: number;
  productId: string;
  listeners: Set<(message: Uint8Array) => void>;
  closeListeners: Set<(error: Error) => void>;
  closedError: Error | null;
  disposed: boolean;
}

/**
 * One live render on the main thread. The core id rides along so disposing one
 * provider fails only its own renders.
 */
interface RenderEntry {
  coreId: number;
  onUpdate: (node: RendererNode) => void;
  onComplete: () => void;
  onError: (error: Error) => void;
}

interface RpcConnectionEntry {
  connection: ChainConnection | null;
  closed: boolean;
}

interface RuntimeState {
  worker: Worker;
  role: HostRole;
  networkSuffix: string | undefined;
  identityAccountId: string | null;
  identityGeneration: number;
  pendingAllowanceSnapshots: Map<number, PendingEntry<WalletAllowanceSnapshot>>;
  pendingNativeChatContacts: Map<
    number,
    PendingEntry<NativeChatContactsSnapshot>
  >;
  rawCallbacks: RawCallbacks;
  coreCallbacks: Map<number, RawCallbacks>;
  cores: Map<number, CoreState>;
  pendingCores: Map<
    number,
    {
      productId: string;
      resolve: (provider: TrUApiProductProvider) => void;
      reject: (error: Error) => void;
    }
  >;
  subscriptionDisposers: Map<number, () => void>;
  chatFileExports: Set<string>;
  disposeNativeChatFiles: () => void;
  /**
   * Open `worker.beginOperation` holds. A non-empty set defers `dispose()`.
   * Worker-wide rather than per-core: operation holds can outlive a product
   * connection. Entries are product-scoped because `OperationId` is only
   * unique per product and products sharing this worker may use the same id.
   */
  openOperations: Set<string>;
  /** A dispose() arrived while operations were open; run it once they drain. */
  disposePending: boolean;
  /**
   * Fires if those operations never drain. A worker that never sends its
   * `endOperation` would otherwise keep the core running for a product the
   * user has closed, still free to raise host prompts, with no way for the
   * caller to force teardown.
   */
  disposeGraceTimer: ReturnType<typeof setTimeout> | undefined;
  /** How long `dispose()` waits for open operations before forcing teardown. */
  operationGraceMs: number;
  mediaSubscriptionAcks: Map<number, () => void>;
  chainConnections: Map<number, RpcConnectionEntry>;
  pendingDisconnects: Map<
    number,
    { resolve: () => void; reject: (error: Error) => void }
  >;
  pendingSessionActivations: Map<
    number,
    { resolve: () => void; reject: (error: Error) => void }
  >;
  pendingLocalIdentities: Map<
    number,
    PendingEntry<LocalIdentity> & {
      onProgress?: (progress: LocalIdentityProgress) => void;
    }
  >;
  pendingPermissionAuthorizationStatuses: Map<
    number,
    {
      resolve: (status: PermissionAuthorizationStatus) => void;
      reject: (error: Error) => void;
    }
  >;
  pendingPermissionAuthorizationStatusBatches: Map<
    number,
    {
      resolve: (statuses: PermissionAuthorizationStatus[]) => void;
      reject: (error: Error) => void;
    }
  >;
  pendingSetPermissionAuthorizationStatuses: Map<
    number,
    { resolve: () => void; reject: (error: Error) => void }
  >;
  pendingPermissionAuthorizationRefreshes: Map<number, PendingEntry<void>>;
  pendingSessionChatIdentityKeys: Map<
    number,
    {
      resolve: (key: Uint8Array | undefined) => void;
      reject: (error: Error) => void;
    }
  >;
  pendingDeviceStatementKeys: Map<
    number,
    {
      resolve: (key: Uint8Array | undefined) => void;
      reject: (error: Error) => void;
    }
  >;
  pendingDeviceEncryptionKeys: Map<
    number,
    { resolve: (key: Uint8Array) => void; reject: (error: Error) => void }
  >;
  pendingProductSubtreePublicKeys: Map<
    number,
    {
      resolve: (key: Uint8Array | undefined) => void;
      reject: (error: Error) => void;
    }
  >;
  /** Host-authored Chat and Renderer actions awaiting the worker's response. */
  pendingActions: Map<
    number,
    { resolve: () => void; reject: (error: Error) => void }
  >;
  /** Sinks for live renders, keyed by render id. */
  renders: Map<number, RenderEntry>;
  /** Products whose worker the core currently wants, for late subscribers. */
  wantedWorkers: Set<string>;
  workerDemandListeners: Set<(change: WorkerDemandChange) => void>;
  closedError: Error | null;
  logLevel: LogLevel;
  disposed: boolean;
  nextCoreId: number;
  coreWireSchemaHash: string | undefined;
}

function debugLoggingEnabled(state: RuntimeState): boolean {
  return state.logLevel === "debug" || state.logLevel === "trace";
}

let nextDisconnectRequestId = 0;
let nextPermissionAuthorizationRequestId = 0;
let nextSessionChatIdentityKeyRequestId = 0;
let nextDeviceStatementKeyRequestId = 0;
let nextDeviceEncryptionKeyRequestId = 0;
let nextProductSubtreePublicKeyRequestId = 0;
let nextSessionActivationRequestId = 0;
let nextLocalIdentityRequestId = 0;
let nextAllowanceSnapshotRequestId = 0;
let nextNativeChatContactsRequestId = 0;
let nextActionRequestId = 0;
let nextRenderId = 0;

function encodePermissionAuthorizationRequest(
  request: PermissionAuthorizationRequest,
): Uint8Array {
  return PermissionAuthorizationRequestCodec.enc(request);
}

const DEV_LOG_LEVEL_KEY = "truapi:logLevel";

function readPersistedLogLevel(): LogLevel | null {
  return globalThis.localStorage?.getItem(DEV_LOG_LEVEL_KEY) ?? null;
}

/**
 * Why the wire debugger is (not) enabled, so a no-dial is never silent. No
 * browser store and no runtime switch: the host passes the dial in, the build's
 * value is the default, and it is resolved once.
 */
export type DebuggerEnablement = {
  readonly url: string | null;
  readonly reason:
    | "enabled-from-option"
    | "enabled-from-build"
    | "production-build"
    | "production-build-configured"
    | "refused-not-loopback"
    | "not-configured";
};

/**
 * Dial URL a dev build was compiled with, when it was given one. The default,
 * not the mechanism: it lets `make debugger` hand a local stack a working tap
 * with nothing to switch on, and an explicit `debugger` option overrides it.
 *
 * Same literal-token rule as the `DEV` read below, and the try/catch covers the
 * realms with no `import.meta.env` at all.
 */
function buildTimeDebuggerUrl(): string | null {
  let raw: unknown;
  try {
    raw = (
      import.meta as unknown as { env: { VITE_TRUAPI_DEBUGGER_URL?: unknown } }
    ).env.VITE_TRUAPI_DEBUGGER_URL;
  } catch {
    return null;
  }
  if (typeof raw !== "string") return null;
  const url = raw.trim();
  return url === "" ? null : url;
}

/**
 * Whether this build may carry a wire tap at all. A hard gate, not a convention:
 * a bundler replaces `import.meta.env.DEV` with a literal, so a production build
 * returns false and no option can turn the tap on.
 *
 * Keep the expression below the *literal* `import.meta.env.DEV`, with no alias
 * and no optional chaining. A bundler replaces that exact token; written any
 * other way it survives into the bundle and is evaluated against an
 * `import.meta.env` a plain module does not have, reading as `undefined` - which
 * refuses in every bundled host rather than only production ones, silently
 * disabling the tap everywhere. The try/catch covers where the access throws.
 */
function debuggerBuildAllows(): boolean {
  try {
    return (
      (import.meta as unknown as { env: { DEV?: boolean } }).env.DEV === true
    );
  } catch {
    return false;
  }
}

/**
 * Which of the two production verdicts applies. The build half is the one that
 * matters: the env var is substituted at build time, so it is still readable in
 * a production bundle, and a build made with it but without
 * `NODE_ENV=development` is exactly the case that must not go quiet.
 */
export function productionReason(
  fromOption: string | null | undefined,
  fromBuild: string | null,
): "production-build" | "production-build-configured" {
  // Same precedence as `resolveDebuggerEnablement`: an option settles it, so the
  // build is not consulted. As an OR this told a host that had refused to
  // rebuild in dev mode, which would still resolve to `not-configured`.
  const asked =
    fromOption === undefined
      ? fromBuild !== null
      : typeof fromOption === "string" && fromOption !== "";
  return asked ? "production-build-configured" : "production-build";
}

function readDebuggerEnablement(
  fromOption: string | null | undefined,
): DebuggerEnablement {
  const fromBuild = buildTimeDebuggerUrl();
  if (!debuggerBuildAllows()) {
    // Somebody asked for a dial this build cannot carry. Going quiet is the §9
    // failure: drop `NODE_ENV=development` from the build command and you get an
    // empty board with no error, which reads as a broken debugger rather than a
    // dial compiled out. Keyed on the build value too, since that is the half
    // that survives into a production bundle.
    return { url: null, reason: productionReason(fromOption, fromBuild) };
  }
  return resolveDebuggerEnablement(fromOption, fromBuild);
}

/**
 * Resolve the dev-build switches into one verdict. Exported so the precedence is
 * testable without a bundler.
 *
 * Precedence, where an omitted option is the only one that defers to the build:
 *
 *  - option set to a URL  -> dial it, whatever the build says
 *  - option set null/""   -> OFF, whatever the build says
 *  - option omitted       -> the build's value, if it carries one
 *
 * Folding `null` in with "omitted" is the easy mistake: it falls through to the
 * build, leaving an embedder that compiled a URL in no way to refuse the dial
 * short of rebuilding. A resolved URL is loopback `ws://` or it is refused (§6).
 */
export function resolveDebuggerEnablement(
  fromOption: string | null | undefined,
  fromBuild: string | null,
): DebuggerEnablement {
  if (typeof fromOption === "string" && fromOption !== "")
    return refuseUnlessLoopback(fromOption, "enabled-from-option");
  // Only an omitted option falls through to the build. Anything else the
  // embedder passed is a refusal, including the `false` a JS host or a
  // `wanted && url` expression yields, which must not turn the tap on.
  if (fromOption !== undefined) return { url: null, reason: "not-configured" };
  if (fromBuild !== null)
    return refuseUnlessLoopback(fromBuild, "enabled-from-build");
  return { url: null, reason: "not-configured" };
}

/** Let `url` through under `reason`, or refuse it for not being loopback `ws://`. */
function refuseUnlessLoopback(
  url: string,
  reason: "enabled-from-option" | "enabled-from-build",
): DebuggerEnablement {
  if (!isLoopbackWsUrl(url))
    return { url: null, reason: "refused-not-loopback" };
  return { url, reason };
}

/**
 * Say once whether the debugger will dial, and from where. The board's socket
 * count moves whether or not a host dialled (its own UI holds one), so without
 * this a host that never dialled reads as a broken debugger. Silent in a
 * production build, where nothing could be done about it anyway.
 */
function reportDebuggerEnablement(e: DebuggerEnablement): void {
  if (e.reason === "production-build") return;
  if (e.reason === "production-build-configured") {
    // Says "did not resolve true", not "this is a production build". The gate
    // cannot tell the two apart: a genuine production build and a bundler that
    // never substituted the token both leave the condition false, and asserting
    // production would send a developer on a dev build under webpack or plain
    // tsc off to rebuild in dev mode - the one case that would not help.
    console.info(
      "[truapi] wire debugger: off (a dial was configured, but " +
        "`import.meta.env.DEV` did not resolve true, so the tap is compiled out. " +
        "Either this is a production build - rebuild the host in dev mode - or " +
        "the bundler did not substitute that token.",
    );
    return;
  }
  const origin = globalThis.location?.origin ?? "(unknown origin)";
  if (e.reason === "enabled-from-option") {
    console.info(
      `[truapi] wire debugger: dialling ${e.url} from the host's option (origin ${origin})`,
    );
    return;
  }
  if (e.reason === "enabled-from-build") {
    console.info(
      `[truapi] wire debugger: dialling ${e.url} from the build (origin ${origin})`,
    );
    return;
  }
  if (e.reason === "refused-not-loopback") {
    console.warn(
      "[truapi] wire debugger: off (the configured dial is not a `ws://` URL on a " +
        "loopback host, so it was refused. The tap forwards frames verbatim, " +
        `payloads included, and never leaves this machine.) on origin ${origin}`,
    );
    return;
  }
  console.info(
    "[truapi] wire debugger: off (this host passed no `debugger` option and the " +
      `build carries no VITE_TRUAPI_DEBUGGER_URL) on origin ${origin}`,
  );
}

function persistLogLevel(level: LogLevel): void {
  globalThis.localStorage?.setItem(DEV_LOG_LEVEL_KEY, level);
}

let devLogLevelOverride: LogLevel | null = readPersistedLogLevel();
const devGlobalTargets = new Set<{ setLogLevel?: (level: LogLevel) => void }>();
/**
 * Deliberately carries no debugger control. The dial is decided once, by the
 * build or by the host, so whether frames are leaving is a property of how this
 * bundle was made rather than of something typed into a console afterwards. A
 * runtime toggle would also give console-paste - a live pattern against wallet
 * users - something worth pasting at.
 */
interface TrUApiDevConsole {
  setLogLevel(level: LogLevel): void;
  getLogLevel(): LogLevel | null;
}

const NATIVE_CHAT_FILE_CALLBACKS: Partial<Record<CallbackName, true>> = {
  pickChatFiles: true,
  readChatFile: true,
  releaseChatFile: true,
  beginChatFileExport: true,
  writeChatFileExport: true,
  finishChatFileExport: true,
  cancelChatFileExport: true,
};

/** Reclaim only undelivered selections; delivered sources belong to durable Host state. */
async function discardChatFileCallback(
  state: RuntimeState,
  name: CallbackName,
  value: unknown,
): Promise<void> {
  try {
    if (name === "pickChatFiles" && value instanceof Uint8Array) {
      await Promise.all(
        Vector(NativeChatPickedFile)
          .dec(value)
          .map((file) => state.rawCallbacks.releaseChatFile(file.sourceId)),
      );
    } else if (name === "beginChatFileExport" && typeof value === "string") {
      state.chatFileExports.delete(value);
      await state.rawCallbacks.cancelChatFileExport(value);
    }
  } catch {
    // A closed/failed backing store is unavailable; never log private handles or payloads.
  }
}

/**
 * Read the host-assigned id out of a `beginOperation` response. Returns null if
 * the response will not decode, so a hold that cannot be keyed is dropped
 * rather than escaping and leaving the worker's call unanswered.
 */
function operationIdFrom(value: unknown): number | null {
  if (!(value instanceof Uint8Array)) return null;
  try {
    return HostWorkerBeginOperationResponseCodec.dec(value).id;
  } catch {
    return null;
  }
}

/**
 * Key one pending-operation hold. `OperationId` is unique per product, not per
 * worker, so the product a `beginOperation`/`endOperation` arrived for has to be
 * part of the key. Returns null if the encoded product will not decode, which
 * drops the hold rather than letting it pin the worker forever.
 */
function operationHold(encodedProduct: unknown, id: number): string | null {
  if (!(encodedProduct instanceof Uint8Array)) return null;
  try {
    return `${ProductContextCodec.dec(encodedProduct).productId}\u0000${id}`;
  } catch {
    return null;
  }
}

function handleCallbackRequest(
  state: RuntimeState,
  msg: {
    requestId: number;
    coreId?: number;
    name: CallbackName;
    args: readonly unknown[];
  },
): void {
  if (state.disposed) return;
  const hostWalletCallback = COINAGE_WALLET_CALLBACKS[msg.name] === true;
  const callbacks =
    msg.coreId === undefined || hostWalletCallback
      ? state.rawCallbacks
      : state.coreCallbacks.get(msg.coreId);
  const fn =
    callbacks && Object.hasOwn(callbacks, msg.name)
      ? (
          callbacks as unknown as Record<
            string,
            (...args: readonly unknown[]) => unknown
          >
        )[msg.name]
      : undefined;
  if (!fn) {
    state.worker.postMessage({
      kind: "callbackResponse",
      requestId: msg.requestId,
      ok: false,
      error: `unknown callback: ${msg.name}`,
    } satisfies MainToWorker);
    return;
  }
  Promise.resolve()
    .then(() => {
      if (state.disposed) throw new Error("Host runtime is unavailable");
      if (
        !hostWalletCallback &&
        msg.coreId !== undefined &&
        !state.coreCallbacks.has(msg.coreId)
      )
        throw new Error("Product callbacks are unavailable");
      return fn(...msg.args);
    })
    .then(
      async (value) => {
        if (state.disposed) {
          await discardChatFileCallback(state, msg.name, value);
          return;
        }
        // Tracked in the success arm only: a rejected begin must not leave a
        // hold that nothing will ever release.
        if (msg.name === "beginOperation") {
          const id = operationIdFrom(value);
          const hold = id === null ? null : operationHold(msg.args[0], id);
          if (hold !== null) state.openOperations.add(hold);
        } else if (msg.name === "endOperation") {
          const id = msg.args[1];
          const hold =
            typeof id === "number" ? operationHold(msg.args[0], id) : null;
          if (hold !== null) state.openOperations.delete(hold);
          if (state.openOperations.size === 0 && state.disposePending) {
            state.disposePending = false;
            clearDisposeGrace(state);
            teardown(state, new Error("runtime disposed"), false);
          }
        }
        if (msg.name === "beginChatFileExport" && typeof value === "string") {
          state.chatFileExports.add(value);
        } else if (
          msg.name === "finishChatFileExport" ||
          msg.name === "cancelChatFileExport"
        ) {
          state.chatFileExports.delete(msg.args[0] as string);
        }
        try {
          state.worker.postMessage({
            kind: "callbackResponse",
            requestId: msg.requestId,
            ok: true,
            value,
          } satisfies MainToWorker);
        } catch {
          await discardChatFileCallback(state, msg.name, value);
          if (state.disposed) return;
          try {
            state.worker.postMessage({
              kind: "callbackResponse",
              requestId: msg.requestId,
              ok: false,
              error: "Host callback result could not be serialized",
            } satisfies MainToWorker);
          } catch {
            teardown(
              state,
              new Error("Host callback transport is unavailable"),
              true,
            );
          }
        }
      },
      (err) => {
        if (state.disposed) return;
        try {
          state.worker.postMessage({
            kind: "callbackResponse",
            requestId: msg.requestId,
            ok: false,
            error: hostWalletCallback
              ? "Native Coinage wallet operation failed"
              : NATIVE_CHAT_FILE_CALLBACKS[msg.name]
                ? "Native Chat file operation failed"
                : msg.name === "mediaBackendCapabilities" ||
                    msg.name === "mediaBackendCommand"
                  ? "media backend failure"
                  : errorMessage(err),
          } satisfies MainToWorker);
        } catch {
          teardown(
            state,
            new Error("Host callback transport is unavailable"),
            true,
          );
        }
      },
    );
}

function handleSubscriptionStart(
  state: RuntimeState,
  msg: {
    subId: number;
    coreId?: number;
    name: SubscriptionName;
    args: readonly unknown[];
  },
): void {
  const privateMedia = msg.name === "mediaBackendEvents";
  let closed = false;
  let outstanding = 0;
  let dispose: (() => void) | void;
  const close = (): void => {
    if (closed) return;
    closed = true;
    state.subscriptionDisposers.delete(msg.subId);
    state.mediaSubscriptionAcks.delete(msg.subId);
    if (typeof dispose === "function") {
      try {
        dispose();
      } catch (error) {
        throw privateMedia ? new Error("media backend failure") : error;
      }
    }
  };
  const sendError = (error: GenericError): void => {
    if (state.disposed || closed) return;
    state.worker.postMessage({
      kind: "subscriptionError",
      subId: msg.subId,
      error:
        privateMedia && error.reason !== "media event overflow"
          ? "media backend failure"
          : error.reason,
    } satisfies MainToWorker);
    if (privateMedia) {
      try {
        close();
      } catch {
        /* Private callback diagnostics are redacted. */
      }
    }
  };
  const sendItem = (value?: unknown): void => {
    if (state.disposed || closed) return;
    if (privateMedia && outstanding >= 128) {
      sendError({ reason: "media event overflow" });
      return;
    }
    if (privateMedia) outstanding++;
    state.worker.postMessage({
      kind: "subscriptionItem",
      subId: msg.subId,
      value,
    } satisfies MainToWorker);
  };
  state.subscriptionDisposers.set(msg.subId, close);
  if (privateMedia) {
    state.mediaSubscriptionAcks.set(msg.subId, () => {
      if (outstanding > 0) outstanding--;
    });
  }
  try {
    const callbacks =
      msg.coreId === undefined
        ? state.rawCallbacks
        : state.coreCallbacks.get(msg.coreId);
    if (!callbacks) throw new Error("Product callbacks are unavailable");
    dispose = startRawSubscription(
      callbacks,
      msg.name,
      msg.args,
      sendItem,
      sendError,
    );
    // A host can publish synchronously during subscribe, including overflow.
    if (closed && typeof dispose === "function") dispose();
  } catch (err) {
    sendError({
      reason: privateMedia ? "media backend failure" : errorMessage(err),
    });
    try {
      close();
    } catch {
      /* Subscription startup has already failed. */
    }
  }
}

function handleSubscriptionStop(
  state: RuntimeState,
  msg: { subId: number },
): void {
  const dispose = state.subscriptionDisposers.get(msg.subId);
  if (!dispose) return;
  state.subscriptionDisposers.delete(msg.subId);
  try {
    dispose();
  } catch (err) {
    console.warn("[truapi worker] subscription dispose threw:", err);
  }
}

async function handleChainConnectStart(
  state: RuntimeState,
  msg: Extract<WorkerToMain, { kind: "chainConnectStart" | "hopConnectStart" }>,
): Promise<void> {
  if (state.disposed) return;
  if (
    state.chainConnections.has(msg.connId) ||
    state.chainConnections.size >= MAX_JSON_RPC_CONNECTIONS
  ) {
    state.worker.postMessage({
      kind: "chainConnectAck",
      connId: msg.connId,
      ok: false,
      error: "JSON-RPC connection limit reached or duplicate connection id",
    } satisfies MainToWorker);
    return;
  }
  const entry: RpcConnectionEntry = { connection: null, closed: false };
  state.chainConnections.set(msg.connId, entry);
  const onResponse = (json: string): void => {
    if (state.disposed || entry.closed) return;
    state.worker.postMessage({
      kind: "chainResponse",
      connId: msg.connId,
      json,
    } satisfies MainToWorker);
  };
  const onClosed = (): void => {
    if (state.disposed || entry.closed) return;
    handleChainClose(state, msg);
    state.worker.postMessage({
      kind: "chainClosed",
      connId: msg.connId,
    } satisfies MainToWorker);
  };
  try {
    const conn = await (msg.kind === "hopConnectStart"
      ? state.rawCallbacks.hopConnect(
          msg.genesisHash,
          msg.endpoint,
          onResponse,
          onClosed,
        )
      : state.rawCallbacks.chainConnect(msg.genesisHash, onResponse, onClosed));
    if (state.disposed || entry.closed) {
      state.chainConnections.delete(msg.connId);
      conn?.close();
      return;
    }
    if (!conn) throw new Error(`${msg.kind} returned no connection`);
    entry.connection = conn;
    state.worker.postMessage({
      kind: "chainConnectAck",
      connId: msg.connId,
      ok: true,
    } satisfies MainToWorker);
  } catch (err) {
    state.chainConnections.delete(msg.connId);
    const report = !state.disposed && !entry.closed;
    entry.closed = true;
    try {
      entry.connection?.close();
    } catch {
      console.warn("[truapi worker] JSON-RPC close failed");
    } finally {
      if (report) {
        state.worker.postMessage({
          kind: "chainConnectAck",
          connId: msg.connId,
          ok: false,
          error:
            msg.kind === "hopConnectStart"
              ? "HOP connection unavailable"
              : errorMessage(err),
        } satisfies MainToWorker);
      }
    }
  }
}

function handleChainSend(
  state: RuntimeState,
  msg: { connId: number; request: string },
): void {
  const entry = state.chainConnections.get(msg.connId);
  if (!entry?.connection || entry.closed) return;
  try {
    if (debugLoggingEnabled(state)) {
      console.debug("[truapi worker] chainSend", msg.connId);
    }
    entry.connection.send(msg.request);
  } catch {
    console.warn("[truapi worker] JSON-RPC send failed");
    if (!entry.closed) {
      handleChainClose(state, msg);
      if (!state.disposed) {
        state.worker.postMessage({
          kind: "chainClosed",
          connId: msg.connId,
        } satisfies MainToWorker);
      }
    }
  }
}

function handleChainClose(state: RuntimeState, msg: { connId: number }): void {
  const entry = state.chainConnections.get(msg.connId);
  if (!entry || entry.closed) return;
  entry.closed = true;
  // Keep a closed opening entry counted until its late handle can be closed.
  if (!entry.connection) return;
  state.chainConnections.delete(msg.connId);
  try {
    entry.connection.close();
  } catch {
    console.warn("[truapi worker] JSON-RPC close failed");
  }
}

interface PendingEntry<T> {
  resolve: (value: T) => void;
  reject: (error: Error) => void;
}

function settlePending<T>(
  map: Map<number, PendingEntry<T>>,
  requestId: number,
  result: { ok: true; value: T } | { ok: false; error: string },
): void {
  const pending = map.get(requestId);
  if (!pending) return;
  map.delete(requestId);
  if (result.ok) pending.resolve(result.value);
  else pending.reject(new Error(result.error));
}

function rejectAll<T>(map: Map<number, PendingEntry<T>>, error: Error): void {
  for (const pending of map.values()) {
    pending.reject(error);
  }
  map.clear();
}

function handleDisconnectResponse(
  state: RuntimeState,
  msg:
    | { requestId: number; ok: true }
    | { requestId: number; ok: false; error: string },
): void {
  settlePending(
    state.pendingDisconnects,
    msg.requestId,
    msg.ok ? { ok: true, value: undefined } : { ok: false, error: msg.error },
  );
}

function handleSessionActivationResponse(
  state: RuntimeState,
  msg:
    | { requestId: number; ok: true }
    | { requestId: number; ok: false; error: string },
): void {
  settlePending(
    state.pendingSessionActivations,
    msg.requestId,
    msg.ok ? { ok: true, value: undefined } : { ok: false, error: msg.error },
  );
}

function handlePermissionAuthorizationStatusResponse(
  state: RuntimeState,
  msg:
    | {
        requestId: number;
        ok: true;
        status: PermissionAuthorizationStatus;
      }
    | { requestId: number; ok: false; error: string },
): void {
  settlePending(
    state.pendingPermissionAuthorizationStatuses,
    msg.requestId,
    msg.ok ? { ok: true, value: msg.status } : { ok: false, error: msg.error },
  );
}

function handlePermissionAuthorizationStatusesResponse(
  state: RuntimeState,
  msg:
    | {
        requestId: number;
        ok: true;
        statuses: PermissionAuthorizationStatus[];
      }
    | { requestId: number; ok: false; error: string },
): void {
  settlePending(
    state.pendingPermissionAuthorizationStatusBatches,
    msg.requestId,
    msg.ok
      ? { ok: true, value: msg.statuses }
      : { ok: false, error: msg.error },
  );
}

function handleSetPermissionAuthorizationStatusResponse(
  state: RuntimeState,
  msg:
    | { requestId: number; ok: true }
    | { requestId: number; ok: false; error: string },
): void {
  settlePending(
    state.pendingSetPermissionAuthorizationStatuses,
    msg.requestId,
    msg.ok ? { ok: true, value: undefined } : { ok: false, error: msg.error },
  );
}

function handleSessionChatIdentityKeyResponse(
  state: RuntimeState,
  msg:
    | { requestId: number; ok: true; key: Uint8Array | undefined }
    | { requestId: number; ok: false; error: string },
): void {
  settlePending(
    state.pendingSessionChatIdentityKeys,
    msg.requestId,
    msg.ok ? { ok: true, value: msg.key } : { ok: false, error: msg.error },
  );
}

function handleDeviceStatementKeyResponse(
  state: RuntimeState,
  msg:
    | { requestId: number; ok: true; key: Uint8Array | undefined }
    | { requestId: number; ok: false; error: string },
): void {
  settlePending(
    state.pendingDeviceStatementKeys,
    msg.requestId,
    msg.ok ? { ok: true, value: msg.key } : { ok: false, error: msg.error },
  );
}

function handleProductSubtreePublicKeyResponse(
  state: RuntimeState,
  msg:
    | { requestId: number; ok: true; key: Uint8Array | undefined }
    | { requestId: number; ok: false; error: string },
): void {
  settlePending(
    state.pendingProductSubtreePublicKeys,
    msg.requestId,
    msg.ok ? { ok: true, value: msg.key } : { ok: false, error: msg.error },
  );
}

function handleDeviceEncryptionKeyResponse(
  state: RuntimeState,
  msg:
    | { requestId: number; ok: true; key: Uint8Array }
    | { requestId: number; ok: false; error: string },
): void {
  settlePending(
    state.pendingDeviceEncryptionKeys,
    msg.requestId,
    msg.ok ? { ok: true, value: msg.key } : { ok: false, error: msg.error },
  );
}

function rejectPendingRuntimeRequests(state: RuntimeState, error: Error): void {
  rejectAll(state.pendingDisconnects, error);
  rejectAll(state.pendingSessionActivations, error);
  rejectAll(state.pendingLocalIdentities, error);
  rejectAll(state.pendingAllowanceSnapshots, error);
  rejectAll(state.pendingNativeChatContacts, error);
  rejectAll(state.pendingPermissionAuthorizationStatuses, error);
  rejectAll(state.pendingPermissionAuthorizationStatusBatches, error);
  rejectAll(state.pendingSetPermissionAuthorizationStatuses, error);
  rejectAll(state.pendingPermissionAuthorizationRefreshes, error);
  rejectAll(state.pendingSessionChatIdentityKeys, error);
  rejectAll(state.pendingDeviceStatementKeys, error);
  rejectAll(state.pendingDeviceEncryptionKeys, error);
  rejectAll(state.pendingProductSubtreePublicKeys, error);
  rejectAll(state.pendingActions, error);
  for (const renderId of [...state.renders.keys()]) {
    const sink = takeRender(state, renderId);
    if (sink) reportRenderFailure(sink, error);
  }
  for (const pending of state.pendingCores.values()) {
    pending.reject(error);
  }
  state.pendingCores.clear();
}

function sendWorkerRequest<T>(
  state: RuntimeState,
  pending: Map<number, PendingEntry<T>>,
  nextId: () => number,
  disposedFallback: T,
  buildMessage: (requestId: number) => MainToWorker,
): Promise<T> {
  if (state.disposed) return Promise.resolve(disposedFallback);
  return new Promise((resolve, reject) => {
    const requestId = nextId();
    pending.set(requestId, { resolve, reject });
    try {
      state.worker.postMessage(buildMessage(requestId));
    } catch (err) {
      pending.delete(requestId);
      reject(err instanceof Error ? err : new Error(String(err)));
    }
  });
}

/**
 * Send a session activation request, rejecting rather than resolving when the
 * runtime is already gone. A host awaits these to learn whether it is signed
 * in, so a silent success after a worker fault would route it as if the
 * activation had run.
 */
function sendSessionActivationRequest(
  state: RuntimeState,
  buildMessage: (requestId: number) => MainToWorker,
  changesIdentity = true,
): Promise<void> {
  if (state.disposed) {
    return Promise.reject(state.closedError ?? new Error("runtime disposed"));
  }
  if (changesIdentity) invalidateAllowanceIdentity(state);
  return sendWorkerRequest<void>(
    state,
    state.pendingSessionActivations,
    () => ++nextSessionActivationRequestId,
    undefined,
    buildMessage,
  );
}

function sendLocalIdentityRequest(
  state: RuntimeState,
  buildMessage: (requestId: number) => MainToWorker,
  onProgress?: (progress: LocalIdentityProgress) => void,
): Promise<LocalIdentity> {
  if (state.disposed) {
    return Promise.reject(state.closedError ?? new Error("runtime disposed"));
  }
  const generation = state.identityGeneration;
  const { promise, resolve, reject } = Promise.withResolvers<LocalIdentity>();
  const requestId = ++nextLocalIdentityRequestId;
  state.pendingLocalIdentities.set(requestId, {
    resolve(identity) {
      if (generation !== state.identityGeneration || state.disposePending) {
        reject(new Error("local identity activation changed"));
        return;
      }
      state.identityAccountId = identity.identityAccountId;
      resolve(identity);
    },
    reject,
    onProgress,
  });
  try {
    state.worker.postMessage(buildMessage(requestId));
  } catch (error) {
    state.pendingLocalIdentities.delete(requestId);
    reject(error);
  }
  return promise;
}

function invalidateAllowanceIdentity(state: RuntimeState): void {
  state.identityGeneration++;
  state.identityAccountId = null;
  rejectAll(
    state.pendingAllowanceSnapshots,
    new Error("local identity activation changed"),
  );
  rejectAll(
    state.pendingNativeChatContacts,
    new Error("local identity activation changed"),
  );
}

async function getWalletAllowanceSnapshot(
  state: RuntimeState,
  input: string[],
): Promise<WalletAllowanceSnapshot> {
  if (state.disposed || state.disposePending) {
    throw state.closedError ?? new Error("runtime disposed");
  }
  if (
    state.role !== "signing" ||
    state.pendingSessionActivations.size > 0 ||
    state.pendingDisconnects.size > 0
  ) {
    throw new Error(
      "allowance inspection requires a current local signing identity",
    );
  }
  const productIds = validateAllowanceProductIds(input);
  const accountId = state.identityAccountId;
  const generation = state.identityGeneration;
  const requestId = ++nextAllowanceSnapshotRequestId;
  const { promise, resolve, reject } =
    Promise.withResolvers<WalletAllowanceSnapshot>();
  const timeout = setTimeout(() => {
    state.pendingAllowanceSnapshots.delete(requestId);
    reject(new Error("wallet allowance inspection timed out after 30000ms"));
  }, 30_000);
  state.pendingAllowanceSnapshots.set(requestId, {
    resolve(snapshot) {
      clearTimeout(timeout);
      if (
        generation !== state.identityGeneration ||
        snapshot?.schemaVersion !== 1 ||
        (accountId !== null && snapshot.identityAccountId !== accountId) ||
        snapshot.networkSuffix !== state.networkSuffix
      ) {
        reject(
          new Error(
            "wallet allowance snapshot does not match the current identity",
          ),
        );
        return;
      }
      resolve(snapshot);
    },
    reject(error) {
      clearTimeout(timeout);
      reject(error);
    },
  });
  try {
    state.worker.postMessage({
      kind: "getWalletAllowanceSnapshot",
      requestId,
      productIds,
    } satisfies MainToWorker);
  } catch (error) {
    settlePending(state.pendingAllowanceSnapshots, requestId, {
      ok: false,
      error: errorMessage(error),
    });
  }
  return promise;
}

async function getNativeChatContacts(
  state: RuntimeState,
): Promise<NativeChatContactsSnapshot> {
  if (state.disposed || state.disposePending) {
    throw state.closedError ?? new Error("runtime disposed");
  }
  if (
    state.role !== "signing" ||
    state.pendingSessionActivations.size > 0 ||
    state.pendingDisconnects.size > 0
  ) {
    throw new Error(
      "native Chat contacts require a current local signing session",
    );
  }
  const generation = state.identityGeneration;
  const requestId = ++nextNativeChatContactsRequestId;
  const { promise, resolve, reject } =
    Promise.withResolvers<NativeChatContactsSnapshot>();
  state.pendingNativeChatContacts.set(requestId, {
    resolve(snapshot) {
      if (generation !== state.identityGeneration || state.disposePending) {
        reject(new Error("local identity activation changed"));
      } else {
        resolve(snapshot);
      }
    },
    reject,
  });
  try {
    state.worker.postMessage({
      kind: "getNativeChatContacts",
      requestId,
    } satisfies MainToWorker);
  } catch (error) {
    settlePending(state.pendingNativeChatContacts, requestId, {
      ok: false,
      error: errorMessage(error),
    });
  }
  return promise;
}

function closeCoreState(core: CoreState, error: Error): void {
  if (core.disposed) return;
  core.disposed = true;
  core.closedError = error;
  for (const listener of [...core.closeListeners]) listener(error);
  core.listeners.clear();
  core.closeListeners.clear();
}

/** Drop the ceiling armed by a deferred `dispose()`, if one is pending. */
function clearDisposeGrace(state: RuntimeState): void {
  if (state.disposeGraceTimer === undefined) return;
  clearTimeout(state.disposeGraceTimer);
  state.disposeGraceTimer = undefined;
}

function teardown(state: RuntimeState, error: Error, fault: boolean): void {
  if (state.disposed) return;
  state.disposed = true;
  clearDisposeGrace(state);
  state.closedError = error;
  rejectPendingRuntimeRequests(state, error);
  for (const core of state.cores.values()) {
    closeCoreState(core, error);
  }
  state.cores.clear();
  state.coreCallbacks.clear();
  for (const fn of state.subscriptionDisposers.values()) {
    try {
      fn();
    } catch {
      // ignore during teardown
    }
  }
  state.subscriptionDisposers.clear();
  state.mediaSubscriptionAcks.clear();
  for (const entry of state.chainConnections.values()) {
    entry.closed = true;
    try {
      entry.connection?.close();
    } catch {
      // ignore during teardown
    }
  }
  state.chainConnections.clear();
  for (const id of state.chatFileExports) {
    void state.rawCallbacks.cancelChatFileExport(id).catch(() => {});
  }
  state.chatFileExports.clear();
  state.disposeNativeChatFiles();
  // A worker nothing can call any more is not wanted.
  for (const productId of [...state.wantedWorkers]) {
    handleWorkerDemandChanged(state, productId, false);
  }
  state.workerDemandListeners.clear();
  releaseDebuggerDial(state);
  if (fault) {
    state.worker.terminate();
  } else {
    try {
      state.worker.postMessage({ kind: "dispose" } satisfies MainToWorker);
    } catch {
      // ignore if worker already gone
    }
    setTimeout(() => state.worker.terminate(), 0);
  }
}

interface CreateWebWorkerHostRuntimeOptions {
  logLevel?: LogLevel;
  hostConfig: WebWorkerHostConfig | WebWorkerSigningHostConfig;
  /**
   * Maximum inactivity during each worker startup phase. Loading the WASM and
   * constructing the runtime each get a full interval. Defaults to 30s.
   */
  initTimeoutMs?: number;
  /**
   * Dev-only: a loopback `ws://` wire debugger to stream tapped frames to.
   *
   * Omit to take what the build was compiled with
   * (`VITE_TRUAPI_DEBUGGER_URL`); pass `null` or `""` to refuse it even when the
   * build carries one. Ignored outside a dev build. Resolved once, at creation,
   * and not changeable from the page.
   */
  debugger?: string | null;
  /**
   * Dev-only: whether to show the built-in indicator while a dial is live.
   *
   * Defaults to `true`. Pass `false` only when this host renders its own visible
   * signal - the point is that a tap streaming frames off this host is never
   * invisible, not that this particular badge is used.
   */
  debuggerIndicator?: boolean;
  /**
   * Host role the worker constructs. Omitted means `"pairing"`.
   *
   * `"signing"` requires a worker loading the `testing` WASM bundle, the only
   * one built with a signing host in it.
   */
  role?: HostRole;
  /**
   * How long `dispose()` waits for open `worker.beginOperation` holds before
   * tearing down anyway. Defaults to 30s.
   */
  operationGraceMs?: number;
}

export interface CreateWebWorkerPairingHostRuntimeOptions extends CreateWebWorkerHostRuntimeOptions {
  hostConfig: WebWorkerHostConfig;
}

export interface CreateWebWorkerSigningHostRuntimeOptions extends CreateWebWorkerHostRuntimeOptions {
  hostConfig: WebWorkerSigningHostConfig;
}

export type WebWorkerHostCallbacks = RequiredHostCallbacks;

export function createWebWorkerPairingHostRuntime(
  worker: Worker,
  host: WebWorkerHostCallbacks,
  options: CreateWebWorkerPairingHostRuntimeOptions,
): Promise<WorkerPairingHostRuntime> {
  // No role default: a host that asks for none must put exactly what it put
  // on the wire before the field existed, and the worker reads absent as
  // "pairing".
  return createWebWorkerHostRuntime(worker, host, options);
}

export function createWebWorkerSigningHostRuntime(
  worker: Worker,
  host: WebWorkerHostCallbacks,
  options: CreateWebWorkerSigningHostRuntimeOptions,
): Promise<WorkerSigningHostRuntime> {
  return createWebWorkerHostRuntime(worker, host, {
    ...options,
    role: options.role ?? "signing",
  });
}

function createWebWorkerHostRuntime(
  worker: Worker,
  host: WebWorkerHostCallbacks,
  options: CreateWebWorkerHostRuntimeOptions,
): Promise<WorkerPairingHostRuntime & WorkerSigningHostRuntime> {
  const browserFiles = host.nativeChatFiles
    ? undefined
    : createBrowserNativeChatFilesHost();
  const callbacks = createWasmRawCallbacks({
    ...host,
    nativeChatFiles: host.nativeChatFiles ?? browserFiles!,
  });

  return new Promise((resolve, reject) => {
    const state: RuntimeState = {
      worker,
      role: options.role ?? "pairing",
      networkSuffix:
        "networkSuffix" in options.hostConfig
          ? options.hostConfig.networkSuffix
          : undefined,
      identityAccountId: null,
      identityGeneration: 0,
      pendingAllowanceSnapshots: new Map(),
      pendingNativeChatContacts: new Map(),
      rawCallbacks: callbacks,
      coreCallbacks: new Map(),
      cores: new Map(),
      pendingCores: new Map(),
      subscriptionDisposers: new Map(),
      openOperations: new Set(),
      disposePending: false,
      disposeGraceTimer: undefined,
      operationGraceMs: options.operationGraceMs ?? 30_000,
      mediaSubscriptionAcks: new Map(),
      chainConnections: new Map(),
      chatFileExports: new Set(),
      disposeNativeChatFiles: () => browserFiles?.dispose(),
      pendingDisconnects: new Map(),
      pendingSessionActivations: new Map(),
      pendingLocalIdentities: new Map(),
      pendingPermissionAuthorizationStatuses: new Map(),
      pendingPermissionAuthorizationStatusBatches: new Map(),
      pendingSetPermissionAuthorizationStatuses: new Map(),
      pendingPermissionAuthorizationRefreshes: new Map(),
      pendingSessionChatIdentityKeys: new Map(),
      pendingProductSubtreePublicKeys: new Map(),
      pendingDeviceStatementKeys: new Map(),
      pendingDeviceEncryptionKeys: new Map(),
      pendingActions: new Map(),
      renders: new Map(),
      wantedWorkers: new Set(),
      workerDemandListeners: new Set(),
      closedError: null,
      logLevel: devLogLevelOverride ?? options.logLevel ?? "off",
      disposed: false,
      nextCoreId: 0,
      coreWireSchemaHash: undefined,
    };

    let runtime: (WorkerPairingHostRuntime & WorkerSigningHostRuntime) | null =
      null;

    const notifyFault = (error: Error): void => {
      teardown(state, error, true);
    };

    const onMessage = (ev: MessageEvent<WorkerToMain>): void => {
      const msg = ev.data;
      switch (msg.kind) {
        case "loaded":
        case "ready":
          break;
        case "coreReady":
          handleCoreReady(state, msg.coreId, runtime);
          break;
        case "coreError":
          handleCoreError(state, msg.coreId, msg.error);
          break;
        case "fatalError":
          console.error("[truapi worker]", msg.error);
          notifyFault(new Error(`worker fatal error: ${msg.error}`));
          break;
        case "frameError":
          handleFrameError(state, msg.coreId, msg.error);
          break;
        case "disposeError":
          console.warn("[truapi worker] dispose:", msg.error);
          break;
        case "frame": {
          const core = state.cores.get(msg.coreId);
          if (!core || core.disposed) break;
          if (debugLoggingEnabled(state)) {
            console.debug("[truapi worker] frame <-", bytesToHex(msg.bytes));
          }
          for (const listener of [...core.listeners]) listener(msg.bytes);
          break;
        }
        case "disconnectSessionResponse":
          handleDisconnectResponse(state, msg);
          break;
        case "sessionActivationResponse":
          handleSessionActivationResponse(state, msg);
          break;
        case "localIdentityProgress":
          if (state.disposed) break;
          try {
            state.pendingLocalIdentities
              .get(msg.requestId)
              ?.onProgress?.(msg.progress);
          } catch {
            // UI observers cannot fail or settle an identity operation.
          }
          break;
        case "localIdentityResponse":
          settlePending(
            state.pendingLocalIdentities,
            msg.requestId,
            msg.ok
              ? { ok: true, value: msg.identity }
              : { ok: false, error: msg.error },
          );
          break;
        case "walletAllowanceSnapshotResponse":
          settlePending(
            state.pendingAllowanceSnapshots,
            msg.requestId,
            msg.ok
              ? { ok: true, value: msg.snapshot }
              : { ok: false, error: msg.error },
          );
          break;
        case "nativeChatContactsResponse":
          settlePending(
            state.pendingNativeChatContacts,
            msg.requestId,
            msg.ok
              ? { ok: true, value: msg.snapshot }
              : { ok: false, error: msg.error },
          );
          break;
        case "permissionAuthorizationStatusResponse":
          handlePermissionAuthorizationStatusResponse(state, msg);
          break;
        case "permissionAuthorizationStatusesResponse":
          handlePermissionAuthorizationStatusesResponse(state, msg);
          break;
        case "setPermissionAuthorizationStatusResponse":
          handleSetPermissionAuthorizationStatusResponse(state, msg);
          break;
        case "refreshPermissionAuthorizationResponse":
          settlePending(
            state.pendingPermissionAuthorizationRefreshes,
            msg.requestId,
            msg.ok
              ? { ok: true, value: undefined }
              : { ok: false, error: msg.error },
          );
          break;
        case "sessionChatIdentityKeyResponse":
          handleSessionChatIdentityKeyResponse(state, msg);
          break;
        case "deviceStatementKeyResponse":
          handleDeviceStatementKeyResponse(state, msg);
          break;
        case "deviceEncryptionKeyResponse":
          handleDeviceEncryptionKeyResponse(state, msg);
          break;
        case "productSubtreePublicKeyResponse":
          handleProductSubtreePublicKeyResponse(state, msg);
          break;
        case "workerDemandChanged":
          // Teardown has already reported every worker unwanted, so a level
          // still in flight would put one back that nothing can serve.
          if (!state.disposed) {
            handleWorkerDemandChanged(state, msg.productId, msg.wanted);
          }
          break;
        case "publishChatActionResponse":
        case "publishRendererActionResponse":
          settlePending(
            state.pendingActions,
            msg.requestId,
            msg.ok
              ? { ok: true, value: undefined }
              : { ok: false, error: msg.error },
          );
          break;
        case "renderItem": {
          const sink = state.renders.get(msg.renderId);
          if (!sink) break;
          // Escaping the listener would strand the render with no terminal.
          try {
            sink.onUpdate(RendererNodeCodec.dec(msg.node));
          } catch (err) {
            takeRender(state, msg.renderId);
            state.worker.postMessage({
              kind: "renderStop",
              renderId: msg.renderId,
            } satisfies MainToWorker);
            reportRenderFailure(sink, err);
          }
          break;
        }
        case "renderComplete": {
          const sink = takeRender(state, msg.renderId);
          try {
            sink?.onComplete();
          } catch (err) {
            console.warn("[truapi worker] render onComplete threw:", err);
          }
          break;
        }
        case "renderError": {
          const sink = takeRender(state, msg.renderId);
          if (sink) reportRenderFailure(sink, new Error(msg.error));
          break;
        }
        case "callbackRequest":
          if (debugLoggingEnabled(state)) {
            console.debug("[truapi worker] callbackRequest", msg.name);
          }
          handleCallbackRequest(state, msg);
          break;
        case "subscriptionStart":
          handleSubscriptionStart(state, msg);
          break;
        case "subscriptionStop":
          handleSubscriptionStop(state, msg);
          break;
        case "mediaSubscriptionAck":
          state.mediaSubscriptionAcks.get(msg.subId)?.();
          break;
        case "chainConnectStart":
        case "hopConnectStart":
          if (debugLoggingEnabled(state)) {
            console.debug("[truapi worker]", msg.kind, msg.connId);
          }
          void handleChainConnectStart(state, msg);
          break;
        case "chainSend":
          handleChainSend(state, msg);
          break;
        case "chainClose":
          handleChainClose(state, msg);
          break;
        default: {
          const { kind } = msg as { kind?: unknown };
          console.warn(
            `[truapi worker] unknown worker message kind: ${String(kind)}`,
          );
        }
      }
    };

    // A worker that never loads still installed its dial, and nothing will stream
    // on its account, so the badge comes off with it. Its own exit rather than
    // part of `cleanupInit`, which the `ready` path also calls.
    const failInit = (error: Error): void => {
      cleanupInit();
      releaseDebuggerDial(state);
      worker.terminate();
      reject(error);
    };

    const onError = (e: ErrorEvent): void => {
      failInit(new Error(`worker init failed: ${e.message}`));
    };

    const onInitMessageError = (): void => {
      failInit(
        new Error("worker message could not be deserialized during init"),
      );
    };

    const onRuntimeError = (e: ErrorEvent): void => {
      console.error("[truapi worker]", e.message);
      notifyFault(new Error(`worker error: ${e.message}`));
    };

    const onMessageError = (): void => {
      notifyFault(new Error("worker message could not be deserialized"));
    };

    // Decided here, once. With no attach-later path there is no second piece of
    // state to keep in step with this one.
    const debuggerDial = installDebuggerDial(
      state,
      readDebuggerEnablement(options.debugger),
      options.debuggerIndicator,
    );
    const timeoutMs = options.initTimeoutMs ?? 30_000;
    let initPhase = "loading WASM";
    let cancelInitTimeout = (): void => {};

    const onInitMessage = (ev: MessageEvent<WorkerToMain>): void => {
      const msg = ev.data;
      if (msg.kind === "loaded") {
        initPhase = "initializing the runtime";
        scheduleInitTimeout();
        worker.postMessage({
          kind: "init",
          logLevel: devLogLevelOverride ?? options.logLevel ?? "off",
          hostConfig: options.hostConfig,
          role: options.role,
          capabilities: {
            chat: host.chat !== undefined,
            permissionStatus: host.permissionStatus !== undefined,
            pocket: host.pocket !== undefined,
            identityBackend: host.identityBackend !== undefined,
            coinageWallet: callbacks.nativeCoinage !== undefined,
            contacts: host.contacts !== undefined,
            media:
              typeof host.media?.mediaBackendCapabilities === "function" &&
              typeof host.media?.mediaBackendEvents === "function" &&
              typeof host.media?.mediaBackendCommand === "function",
          },
          debuggerUrl: debuggerDial,
        } satisfies MainToWorker);
      } else if (msg.kind === "ready") {
        state.coreWireSchemaHash = msg.schema;
        cleanupInit();
        worker.addEventListener("message", onMessage);
        worker.addEventListener("error", onRuntimeError);
        worker.addEventListener("messageerror", onMessageError);
        runtime = buildRuntime(state);
        exposeDevGlobal(runtime);
        resolve(runtime);
      } else if (msg.kind === "fatalError") {
        failInit(new Error(`worker init reported error: ${msg.error}`));
      }
    };

    const cleanupInit = (): void => {
      cancelInitTimeout();
      worker.removeEventListener("error", onError);
      worker.removeEventListener("messageerror", onInitMessageError);
      worker.removeEventListener("message", onInitMessage);
    };

    const scheduleInitTimeout = (): void => {
      cancelInitTimeout();
      const timeout = setTimeout(() => {
        failInit(
          new Error(
            `worker init timed out after ${timeoutMs}ms while ${initPhase}`,
          ),
        );
      }, timeoutMs);
      cancelInitTimeout = () => clearTimeout(timeout);
    };

    scheduleInitTimeout();

    worker.addEventListener("error", onError);
    worker.addEventListener("messageerror", onInitMessageError);
    worker.addEventListener("message", onInitMessage);
  });
}

function handleCoreReady(
  state: RuntimeState,
  coreId: number,
  runtime: WorkerPairingHostRuntime | null,
): void {
  const pending = state.pendingCores.get(coreId);
  if (!pending || !runtime) return;
  state.pendingCores.delete(coreId);
  const core: CoreState = {
    coreId,
    productId: pending.productId,
    listeners: new Set(),
    closeListeners: new Set(),
    closedError: null,
    disposed: false,
  };
  state.cores.set(coreId, core);
  pending.resolve(buildProvider(state, core, runtime));
}

function handleCoreError(
  state: RuntimeState,
  coreId: number,
  error: string,
): void {
  const pending = state.pendingCores.get(coreId);
  if (!pending) return;
  state.pendingCores.delete(coreId);
  state.coreCallbacks.delete(coreId);
  pending.reject(new Error(error));
}

function handleFrameError(
  state: RuntimeState,
  coreId: number,
  error: string,
): void {
  console.error("[truapi worker]", error);
  const core = state.cores.get(coreId);
  if (!core) return;
  const failure = new Error(`worker frame error: ${error}`);
  closeCoreState(core, failure);
  state.cores.delete(coreId);
  state.coreCallbacks.delete(coreId);
  // Renders left registered would never settle: the worker cancels them with
  // the core, so nothing further arrives to complete the sink.
  failRendersForCore(state, coreId, failure);
  try {
    state.worker.postMessage({
      kind: "disposeCore",
      coreId,
    } satisfies MainToWorker);
  } catch {
    // ignore if worker is already gone
  }
}

function buildRuntime(
  state: RuntimeState,
): WorkerPairingHostRuntime & WorkerSigningHostRuntime {
  const runtime: WorkerPairingHostRuntime & WorkerSigningHostRuntime = {
    coreWireSchemaHash: state.coreWireSchemaHash,
    createProvider(product, callbacks): Promise<TrUApiProductProvider> {
      if (state.disposed) {
        return Promise.reject(
          state.closedError ?? new Error("runtime disposed"),
        );
      }
      return new Promise((resolve, reject) => {
        const coreId = ++state.nextCoreId;
        if (callbacks)
          state.coreCallbacks.set(coreId, createWasmRawCallbacks(callbacks));
        state.pendingCores.set(coreId, {
          productId: product.productId,
          resolve,
          reject,
        });
        try {
          if (callbacks)
            state.coreCallbacks.set(coreId, createWasmRawCallbacks(callbacks));
          state.worker.postMessage({
            kind: "createCore",
            coreId,
            product,
            ...(callbacks === undefined
              ? {}
              : {
                  capabilities: {
                    chat: callbacks.chat !== undefined,
                    contacts: callbacks.contacts !== undefined,
                    permissionStatus: callbacks.permissionStatus !== undefined,
                    pocket: callbacks.pocket !== undefined,
                    identityBackend: callbacks.identityBackend !== undefined,
                    coinageWallet:
                      state.rawCallbacks.nativeCoinage !== undefined,
                    media:
                      typeof callbacks.media?.mediaBackendCapabilities ===
                        "function" &&
                      typeof callbacks.media?.mediaBackendEvents ===
                        "function" &&
                      typeof callbacks.media?.mediaBackendCommand ===
                        "function",
                  },
                }),
          } satisfies MainToWorker);
        } catch (err) {
          state.pendingCores.delete(coreId);
          state.coreCallbacks.delete(coreId);
          reject(err instanceof Error ? err : new Error(String(err)));
        }
      });
    },
    disconnectSession(): Promise<void> {
      invalidateAllowanceIdentity(state);
      return sendWorkerRequest<void>(
        state,
        state.pendingDisconnects,
        () => ++nextDisconnectRequestId,
        undefined,
        (requestId) => ({ kind: "disconnectSession", requestId }),
      );
    },
    cancelPairing(): void {
      if (state.disposed) return;
      state.worker.postMessage({
        kind: "cancelPairing",
      } satisfies MainToWorker);
    },
    getSessionChatIdentityKey(): Promise<Uint8Array | undefined> {
      return sendWorkerRequest<Uint8Array | undefined>(
        state,
        state.pendingSessionChatIdentityKeys,
        () => ++nextSessionChatIdentityKeyRequestId,
        undefined,
        (requestId) => ({ kind: "getSessionChatIdentityKey", requestId }),
      );
    },
    getDeviceStatementKey(): Promise<Uint8Array | undefined> {
      return sendWorkerRequest<Uint8Array | undefined>(
        state,
        state.pendingDeviceStatementKeys,
        () => ++nextDeviceStatementKeyRequestId,
        undefined,
        (requestId) => ({ kind: "getDeviceStatementKey", requestId }),
      );
    },
    getDeviceEncryptionKey(): Promise<Uint8Array> {
      // A key has no safe empty value: callers encrypt with what they get back,
      // so a disposed runtime must fail rather than hand out a zero-length one.
      // The check is synchronous with the send, so the fallback is unreachable.
      if (state.disposed) {
        return Promise.reject(new Error("worker host runtime is disposed"));
      }
      return sendWorkerRequest<Uint8Array>(
        state,
        state.pendingDeviceEncryptionKeys,
        () => ++nextDeviceEncryptionKeyRequestId,
        new Uint8Array(),
        (requestId) => ({ kind: "getDeviceEncryptionKey", requestId }),
      );
    },
    getProductSubtreePublicKey(
      productId: string,
      timeoutMs?: number,
    ): Promise<Uint8Array | undefined> {
      return sendWorkerRequest<Uint8Array | undefined>(
        state,
        state.pendingProductSubtreePublicKeys,
        () => ++nextProductSubtreePublicKeyRequestId,
        undefined,
        (requestId) => ({
          kind: "getProductSubtreePublicKey",
          requestId,
          productId,
          timeoutMs,
        }),
      );
    },
    notifySessionStoreChanged(): void {
      if (state.disposed) return;
      state.worker.postMessage({
        kind: "notifySessionStoreChanged",
      } satisfies MainToWorker);
    },
    notifyContactsChanged(): void {
      rejectAll(
        state.pendingNativeChatContacts,
        new Error("native Chat contacts changed"),
      );
      postUnlessDisposed(state, { kind: "notifyContactsChanged" });
    },
    acquireWorker(productId: string): void {
      postUnlessDisposed(state, { kind: "acquireWorker", productId });
    },
    releaseWorker(productId: string): void {
      postUnlessDisposed(state, { kind: "releaseWorker", productId });
    },
    subscribeWorkerDemand(listener) {
      // Teardown cleared the listeners, so one added now would only be
      // retained, never called.
      if (state.disposed) return () => {};
      state.workerDemandListeners.add(listener);
      for (const productId of state.wantedWorkers) {
        deliverWorkerDemand(listener, { productId, wanted: true });
      }
      return () => {
        state.workerDemandListeners.delete(listener);
      };
    },
    activateStoredSession(): Promise<void> {
      return sendSessionActivationRequest(state, (requestId) => ({
        kind: "activateStoredSession",
        requestId,
      }));
    },
    activateExternalSession(blob: Uint8Array): Promise<void> {
      return sendSessionActivationRequest(state, (requestId) => ({
        kind: "activateExternalSession",
        requestId,
        blob,
      }));
    },
    activateLocalSession(
      secret: Uint8Array,
      liteUsername?: string,
    ): Promise<void> {
      return sendSessionActivationRequest(state, (requestId) => ({
        kind: "activateLocalSession",
        requestId,
        secret,
        liteUsername,
      }));
    },
    setGrantAllowancesUnchecked(granted: boolean): Promise<void> {
      return sendSessionActivationRequest(
        state,
        (requestId) => ({
          kind: "setGrantAllowancesUnchecked",
          requestId,
          granted,
        }),
        false,
      );
    },
    setWithheldResources(tags: string[]): Promise<void> {
      return sendSessionActivationRequest(state, (requestId) => ({
        kind: "setWithheldResources",
        requestId,
        tags,
      }));
    },
    resetSessionState(): Promise<void> {
      return sendSessionActivationRequest(state, (requestId) => ({
        kind: "resetSessionState",
        requestId,
      }));
    },
    activateLocalSessionWithIdentity(
      secret: Uint8Array,
      liteUsername?: string,
    ): Promise<void> {
      return sendSessionActivationRequest(state, (requestId) => ({
        kind: "activateLocalSessionWithIdentity",
        requestId,
        secret,
        liteUsername,
      }));
    },
    refreshLocalIdentity(): Promise<LocalIdentity> {
      return sendLocalIdentityRequest(state, (requestId) => ({
        kind: "refreshLocalIdentity",
        requestId,
      }));
    },
    getWalletAllowanceSnapshot(productIds): Promise<WalletAllowanceSnapshot> {
      return getWalletAllowanceSnapshot(state, productIds);
    },
    getNativeChatContacts(): Promise<NativeChatContactsSnapshot> {
      return getNativeChatContacts(state);
    },
    registerLocalLiteUsername(
      baseUsername,
      identityBackendBaseUrl,
      onProgress,
    ): Promise<LocalIdentity> {
      return sendLocalIdentityRequest(
        state,
        (requestId) => ({
          kind: "registerLocalLiteUsername",
          requestId,
          baseUsername,
          identityBackendBaseUrl,
        }),
        onProgress,
      );
    },
    getPermissionAuthorizationStatus(productId, request) {
      return sendWorkerRequest<PermissionAuthorizationStatus>(
        state,
        state.pendingPermissionAuthorizationStatuses,
        () => ++nextPermissionAuthorizationRequestId,
        "NotDetermined",
        (requestId) => ({
          kind: "getPermissionAuthorizationStatus",
          productId,
          requestId,
          request: encodePermissionAuthorizationRequest(request),
        }),
      );
    },
    getPermissionAuthorizationStatuses(productId, requests) {
      return sendWorkerRequest<PermissionAuthorizationStatus[]>(
        state,
        state.pendingPermissionAuthorizationStatusBatches,
        () => ++nextPermissionAuthorizationRequestId,
        requests.map(() => "NotDetermined"),
        (requestId) => ({
          kind: "getPermissionAuthorizationStatuses",
          productId,
          requestId,
          requests: requests.map(encodePermissionAuthorizationRequest),
        }),
      );
    },
    setPermissionAuthorizationStatus(productId, request, status) {
      if (state.disposed) {
        return Promise.reject(
          state.closedError ?? new Error("runtime disposed"),
        );
      }
      return sendWorkerRequest<void>(
        state,
        state.pendingSetPermissionAuthorizationStatuses,
        () => ++nextPermissionAuthorizationRequestId,
        undefined,
        (requestId) => ({
          kind: "setPermissionAuthorizationStatus",
          productId,
          requestId,
          request: encodePermissionAuthorizationRequest(request),
          status,
        }),
      );
    },
    refreshPermissionAuthorization(productId, request) {
      if (state.disposed) {
        return Promise.reject(
          state.closedError ?? new Error("runtime disposed"),
        );
      }
      return sendWorkerRequest<void>(
        state,
        state.pendingPermissionAuthorizationRefreshes,
        () => ++nextPermissionAuthorizationRequestId,
        undefined,
        (requestId) => ({
          kind: "refreshPermissionAuthorization",
          productId,
          requestId,
          request: encodePermissionAuthorizationRequest(request),
        }),
      );
    },
    setLogLevel(level): void {
      if (state.disposed) return;
      state.logLevel = level;
      state.worker.postMessage({
        kind: "setLogLevel",
        level,
      } satisfies MainToWorker);
    },
    dispose(): void {
      invalidateAllowanceIdentity(state);
      devGlobalTargets.delete(runtime);
      // Let a background task (e.g. a funding transaction) finish; the last
      // endOperation runs the teardown. Fault teardown is never deferred.
      if (state.openOperations.size > 0) {
        state.disposePending = true;
        state.disposeGraceTimer ??= setTimeout(() => {
          state.disposeGraceTimer = undefined;
          if (!state.disposePending) return;
          state.disposePending = false;
          teardown(state, new Error("runtime disposed"), false);
        }, state.operationGraceMs);
        return;
      }
      teardown(state, new Error("runtime disposed"), false);
    },
  };
  return runtime;
}

/** Post a fire-and-forget control message; a disposed runtime drops it. */
function postUnlessDisposed(state: RuntimeState, message: MainToWorker): void {
  if (state.disposed) return;
  state.worker.postMessage(message);
}

/** Hand one change to one listener, keeping its throw off the caller. */
function deliverWorkerDemand(
  listener: (change: WorkerDemandChange) => void,
  change: WorkerDemandChange,
): void {
  try {
    listener(change);
  } catch (err) {
    console.warn("[truapi worker] worker demand listener threw:", err);
  }
}

/** Record one product's wanted level and fan it out to every listener. */
function handleWorkerDemandChanged(
  state: RuntimeState,
  productId: string,
  wanted: boolean,
): void {
  if (wanted) state.wantedWorkers.add(productId);
  else state.wantedWorkers.delete(productId);
  // Delivery runs over a snapshot, and skips anyone no longer subscribed when
  // their turn comes: a listener that subscribes from inside a listener has
  // already had this change replayed to it, and one that unsubscribes, or
  // disposes the runtime, must hear nothing further.
  for (const listener of [...state.workerDemandListeners]) {
    if (!state.workerDemandListeners.has(listener)) continue;
    deliverWorkerDemand(listener, { productId, wanted });
  }
}

/** Deliver a render failure without letting the sink's own throw escape. */
function reportRenderFailure(
  sink: { onError?: (error: Error) => void },
  cause: unknown,
): void {
  try {
    sink.onError?.(toError(cause));
  } catch (err) {
    console.warn("[truapi worker] render onError threw:", err);
  }
}

/**
 * Drop one render from the ledger, returning its sink only the first time,
 * which is what keeps a render settled exactly once.
 */
function takeRender(
  state: RuntimeState,
  renderId: number,
): RenderEntry | undefined {
  const entry = state.renders.get(renderId);
  if (!entry) return undefined;
  state.renders.delete(renderId);
  return entry;
}

/** Settle and drop every render belonging to one product connection. */
function failRendersForCore(
  state: RuntimeState,
  coreId: number,
  error: Error,
): void {
  for (const [renderId, entry] of [...state.renders]) {
    if (entry.coreId !== coreId) continue;
    const sink = takeRender(state, renderId);
    if (sink) reportRenderFailure(sink, error);
  }
}

/**
 * Post one host-authored action to the worker and settle on its response.
 * Encoding runs before registering, so a payload the codec rejects leaves no
 * pending entry behind.
 */
function publishAction(
  state: RuntimeState,
  core: CoreState,
  kind: "publishChatAction" | "publishRendererAction",
  encode: () => Uint8Array,
): Promise<void> {
  if (state.disposed || core.disposed) {
    return Promise.reject(new Error("product connection is closed"));
  }
  let action: Uint8Array;
  try {
    action = encode();
  } catch (err) {
    return Promise.reject(toError(err));
  }
  return sendWorkerRequest<void>(
    state,
    state.pendingActions,
    () => nextActionRequestId++,
    undefined,
    (requestId) => ({ kind, coreId: core.coreId, requestId, action }),
  );
}

function buildProvider(
  state: RuntimeState,
  core: CoreState,
  runtime: WorkerPairingHostRuntime,
): TrUApiProductProvider {
  const provider: TrUApiProductProvider = {
    postMessage(bytes: Uint8Array): void {
      if (state.disposed || core.disposed) return;
      if (debugLoggingEnabled(state)) {
        console.debug("[truapi worker] frame ->", bytesToHex(bytes));
      }
      state.worker.postMessage({
        kind: "frame",
        coreId: core.coreId,
        bytes,
      } satisfies MainToWorker);
    },
    subscribe(callback) {
      core.listeners.add(callback);
      return () => {
        core.listeners.delete(callback);
      };
    },
    subscribeClose(callback) {
      const closed = core.closedError ?? state.closedError;
      if (closed) {
        callback(closed);
        return () => {};
      }
      core.closeListeners.add(callback);
      return () => {
        core.closeListeners.delete(callback);
      };
    },
    disconnectSession(): Promise<void> {
      if (core.disposed) return Promise.resolve();
      return runtime.disconnectSession();
    },
    async getSessionChatIdentityKey(): Promise<Bytes32 | undefined> {
      if (core.disposed) return undefined;
      const key = await runtime.getSessionChatIdentityKey();
      return key && bytesToHex(key);
    },
    async getDeviceStatementKey(): Promise<Uint8Array | undefined> {
      if (core.disposed) return undefined;
      return runtime.getDeviceStatementKey();
    },
    async getDeviceEncryptionKey(): Promise<Bytes32> {
      if (core.disposed) {
        throw new Error("product connection is closed");
      }
      return bytesToHex(await runtime.getDeviceEncryptionKey());
    },
    async getProductSubtreePublicKey(
      productId: string,
      timeoutMs?: number,
    ): Promise<Bytes32 | undefined> {
      if (core.disposed) return undefined;
      const key = await runtime.getProductSubtreePublicKey(
        productId,
        timeoutMs,
      );
      return key && bytesToHex(key);
    },
    getPermissionAuthorizationStatus(request) {
      if (core.disposed) return Promise.resolve("NotDetermined");
      return runtime.getPermissionAuthorizationStatus(core.productId, request);
    },
    getPermissionAuthorizationStatuses(requests) {
      if (core.disposed) {
        return Promise.resolve(requests.map(() => "NotDetermined"));
      }
      return runtime.getPermissionAuthorizationStatuses(
        core.productId,
        requests,
      );
    },
    setPermissionAuthorizationStatus(request, status) {
      if (core.disposed) {
        return Promise.reject(
          core.closedError ?? new Error("product connection is closed"),
        );
      }
      return runtime.setPermissionAuthorizationStatus(
        core.productId,
        request,
        status,
      );
    },
    refreshPermissionAuthorization(request) {
      if (core.disposed) {
        return Promise.reject(
          core.closedError ?? new Error("product connection is closed"),
        );
      }
      return runtime.refreshPermissionAuthorization(core.productId, request);
    },
    setLogLevel(level): void {
      if (core.disposed) return;
      runtime.setLogLevel(level);
    },
    publishChatAction(action: HostChatActionSubscribeItem): Promise<void> {
      return publishAction(state, core, "publishChatAction", () =>
        HostChatActionSubscribeItemCodec.enc(action),
      );
    },
    publishRendererAction(
      item: HostRendererActionSubscribeItem,
    ): Promise<void> {
      return publishAction(state, core, "publishRendererAction", () =>
        HostRendererActionSubscribeItemCodec.enc(item),
      );
    },
    render(request, sink) {
      if (state.disposed || core.disposed) {
        sink.onError?.(new Error("product connection is closed"));
        return () => {};
      }
      // Encode before registering, so a request the codec rejects leaves no
      // render behind that the worker was never told about.
      let encoded: Uint8Array;
      try {
        encoded = ProductRendererRenderRequestCodec.enc(request);
      } catch (err) {
        reportRenderFailure(sink, err);
        return () => {};
      }
      const renderId = nextRenderId++;
      // No worker reference is taken here: the core holds the one an open
      // render is worth and reports it through `workerDemandChanged`.
      state.renders.set(renderId, {
        coreId: core.coreId,
        onUpdate: (node) => sink.onUpdate(node),
        onComplete: () => sink.onComplete?.(),
        onError: (error) => sink.onError?.(error),
      });
      try {
        state.worker.postMessage({
          kind: "renderStart",
          coreId: core.coreId,
          renderId,
          request: encoded,
        } satisfies MainToWorker);
      } catch (err) {
        const failed = takeRender(state, renderId);
        if (failed) reportRenderFailure(failed, err);
        return () => {};
      }
      return () => {
        if (!takeRender(state, renderId)) return;
        state.worker.postMessage({
          kind: "renderStop",
          renderId,
        } satisfies MainToWorker);
      };
    },
    dispose(): void {
      if (core.disposed) return;
      closeCoreState(core, new Error("provider disposed"));
      state.cores.delete(core.coreId);
      state.coreCallbacks.delete(core.coreId);
      // Renders left registered would never settle: the worker cancels them
      // with the core, so nothing further arrives to complete the sink.
      failRendersForCore(state, core.coreId, new Error("provider disposed"));
      state.worker.postMessage({
        kind: "disposeCore",
        coreId: core.coreId,
      } satisfies MainToWorker);
    },
  };
  return provider;
}

/** Element id of the dial indicator, so a re-render finds the existing node. */
const DEBUGGER_INDICATOR_ID = "truapi-debugger-indicator";

/**
 * What the badge names: every live dial that asked to be shown, keyed by the
 * runtime owning it. A `debuggerIndicator: false` dial renders its own signal
 * and is absent, so this is not an inventory of live taps.
 *
 * One node at a fixed id serves every runtime, so keying by owner is what stops
 * one from taking down a badge another's tap is behind, and lets two live dials
 * both be named.
 */
const liveDebuggerDials = new Map<object, string>();

/** Whether a paint is already waiting on `DOMContentLoaded`. */
let indicatorRepaintQueued = false;

/**
 * Put `owner`'s debugger dial into service: say once whether it will dial, show
 * the endpoint in the page for as long as it does, and hand back the URL the
 * worker's `init` message carries.
 *
 * The three are one decision, so they are one function: a host that resolves a
 * dial and then reports, badges or forwards something else is the §9 failure.
 * Taking the resolved enablement as an argument is what makes it testable.
 */
export function installDebuggerDial(
  owner: object,
  enablement: DebuggerEnablement,
  indicator: boolean | undefined,
): string | null {
  reportDebuggerEnablement(enablement);
  if (enablement.url !== null && indicator !== false)
    liveDebuggerDials.set(owner, enablement.url);
  else liveDebuggerDials.delete(owner);
  paintDebuggerIndicator();
  return enablement.url;
}

/**
 * Take `owner`'s dial out of service. Its worker is gone, so nothing streams on
 * its account any more, and a badge naming an endpoint no frame reaches is the
 * silent-tap failure read backwards.
 */
export function releaseDebuggerDial(owner: object): void {
  if (!liveDebuggerDials.delete(owner)) return;
  paintDebuggerIndicator();
}

/**
 * Show in the page that frames are leaving, naming and linking every endpoint.
 * A console line scrolls away, so a tap left on from an earlier session is
 * invisible for the rest of the day. Default-on because the failure prevented is
 * a host forgetting; one with its own affordance passes `debuggerIndicator:
 * false`. Never throws: a badge must not stop a host starting.
 */
function paintDebuggerIndicator(): void {
  try {
    const doc = globalThis.document;
    if (doc === undefined) return;
    if (doc.body === null) {
      // A runtime created from a `<head>` script has no body yet, and returning
      // alone would leave the tap live and the badge permanently absent. The flag
      // keeps repeated paints from stacking listeners.
      if (!indicatorRepaintQueued) {
        indicatorRepaintQueued = true;
        doc.addEventListener(
          "DOMContentLoaded",
          () => {
            indicatorRepaintQueued = false;
            paintDebuggerIndicator();
          },
          { once: true },
        );
      }
      return;
    }
    const existing = doc.getElementById(DEBUGGER_INDICATOR_ID);
    const endpoints = [...new Set(liveDebuggerDials.values())];
    if (endpoints.length === 0) {
      existing?.remove();
      return;
    }
    const el = existing ?? doc.createElement("div");
    if (existing === null) {
      el.id = DEBUGGER_INDICATOR_ID;
      // Bottom-left: the ribbon and most host chrome live on the right, and a
      // very high z-index keeps it above a modal that would otherwise hide the
      // one signal saying frames are still leaving.
      el.style.cssText =
        "position:fixed;left:8px;bottom:8px;z-index:2147483647;" +
        "padding:4px 8px;border-radius:6px;pointer-events:none;" +
        "background:#7a1f3d;color:#fff;font:600 11px/1.4 ui-monospace,monospace;" +
        "box-shadow:0 2px 8px rgba(0,0,0,.4)";
      doc.body.appendChild(el);
    }
    // Each endpoint links to the board reading it, so noticing the tap and opening
    // it are one step. The badge keeps `pointer-events:none` so it never swallows
    // a click meant for the host; only the links take them back.
    el.textContent = "TrUAPI wire → ";
    endpoints.forEach((endpoint, i) => {
      if (i > 0) el.appendChild(doc.createTextNode(", "));
      const link = doc.createElement("a");
      // The board is served over HTTP on the port the dial streams to.
      link.href = endpoint.replace(/^ws/, "http");
      link.target = "_blank";
      link.rel = "noreferrer";
      link.textContent = endpoint;
      link.style.cssText =
        "color:inherit;text-decoration:underline;pointer-events:auto";
      el.appendChild(link);
    });
    el.title = "This host is streaming product wire frames to a debugger.";
  } catch {
    // A badge that cannot render must never disturb the host.
  }
}

function exposeDevGlobal(target: {
  setLogLevel?: (level: LogLevel) => void;
}): void {
  devGlobalTargets.add(target);
  if (devLogLevelOverride !== null) {
    target.setLogLevel?.(devLogLevelOverride);
  }
  publishDevGlobal();
}

function publishDevGlobal(): void {
  const target = globalThis as {
    __truapi?: TrUApiDevConsole;
  };
  target.__truapi = {
    setLogLevel(level: LogLevel): void {
      devLogLevelOverride = level;
      persistLogLevel(level);
      for (const provider of [...devGlobalTargets]) {
        provider.setLogLevel?.(level);
      }
      console.info(`[truapi worker] logLevel=${level}`);
    },
    getLogLevel(): LogLevel | null {
      return devLogLevelOverride;
    },
  };
}

publishDevGlobal();
