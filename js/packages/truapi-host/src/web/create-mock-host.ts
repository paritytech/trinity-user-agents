// A deterministic, in-memory mock host. `createMockHost` returns a complete
// `RequiredHostCallbacks` set (the JS sibling of `truapi::platform`'s
// `MockPlatform`) plus recordings for assertions. Hand `host.callbacks` to
// `createWebWorkerPairingHostRuntime` (or `createWasmRawCallbacks` directly) to
// run the real truapi WASM core against a mocked OS seam: storage is
// in-memory, permissions answer from a fixed policy, navigation/notifications
// are recorded, and the chain connection is silent (or replays canned frames).
//
// Signing and login park here because this mock backs a *pairing* host, which
// holds no key material of its own: both wait on a paired wallet answering
// over the statement-store channel, and the default silent chain never
// answers. The limit is the host role rather than the mock or the chain — a
// signing host's `sign_raw` completes against this same silent chain.
// Everything else (storage, permissions, features, theme, navigation,
// notifications, preimage lookup) works without a wallet.
//
// Preimage submission is core-owned on current core (the core builds, signs,
// and submits the Bulletin `TransactionStorage.store` transaction itself), so
// the mock only implements host-side content retrieval via `lookupPreimage`;
// seed retrievable content with the returned `insertPreimage`.

import { blake2b } from "@noble/hashes/blake2.js";
import { err, ok } from "neverthrow";

import { scale } from "@parity/truapi";

import type {
  ChatMessageContent,
  ChatRoom,
  GenericError,
  HostChatListSubscribeItem,
  HostChatPostMessageResponse,
  HostChatRegisterBotRequest,
  HostLocaleSubscribeItem,
  HostLocalStorageChangeItem,
  HostThemeSubscribeItem,
  Result,
  ThemeVariant,
} from "@parity/truapi";

import type {
  AuthState,
  CoreStorageKey,
  HostChainSet,
  JsonRpcConnection,
  PermissionDecision,
  RequiredHostCallbacks,
  UserConfirmationReview,
} from "../generated/host-callbacks.js";
import type { ProductRuntimeConfig } from "../runtime.js";

/** How the mock answers a permission prompt for one capability. */
export type PermissionPolicy = "allow-all" | "deny-all";

/**
 * The denying policy under the name `@parity/host-api-test-sdk` gives it.
 *
 * Recognised rather than merely unequal to `"allow-all"`: a policy name the
 * mock does not know is refused, so a typo says so instead of denying every
 * permission with nothing to explain the refusals.
 */
export type PermissionPolicyAlias = "reject-all";

/** Resolve a policy name, rejecting one that means nothing here. */
function normalizePermissionPolicy(
  behavior: PermissionPolicy | PermissionPolicyAlias,
): PermissionPolicy {
  if (behavior === "allow-all" || behavior === "deny-all") return behavior;
  if (behavior === "reject-all") return "deny-all";
  throw new Error(
    `testHost \`setPermissionBehavior\` does not know the policy ` +
      `"${String(behavior)}". Use "allow-all", or "deny-all" (which ` +
      `\`@parity/host-api-test-sdk\` spells "reject-all").`,
  );
}

/** The core's product-storage key prefix, up to and including its version. */
const CORE_PRODUCT_STORAGE_PREFIX = /^truapi:product-storage:v\d+:/;

/**
 * The product's own key inside a core-namespaced one, or `undefined`.
 *
 * Reading the tail rather than any `:key` suffix is what keeps a prefixed store
 * distinct from an unprefixed one: a product writing `demo:mykey` and `mykey`
 * produces two keys that both end in `:mykey`, so a suffix search for `mykey`
 * answers with whichever comes first and a test asserting they do not collide
 * can never fail.
 *
 * The product id is length-prefixed by the core precisely because it may hold
 * colons -- `localhost:3000` is an ordinary one -- so the length is what says
 * where the id ends, not the next separator.
 */
export function coreProductStorageKey(stored: string): string | undefined {
  const prefix = CORE_PRODUCT_STORAGE_PREFIX.exec(stored);
  if (!prefix) return undefined;
  const rest = stored.slice(prefix[0].length);
  const separator = rest.indexOf(":");
  if (separator < 0) return undefined;
  const length = Number(rest.slice(0, separator));
  if (!Number.isInteger(length) || length < 0) return undefined;
  const afterId = rest.slice(separator + 1 + length);
  return afterId.startsWith(":") ? afterId.slice(1) : undefined;
}

/**
 * A chain the host will proxy to, rather than answer from memory.
 *
 * Matched on `genesisHash`: the core asks for a chain by hash, so the hash here
 * must be the *real* one of the endpoint, not a {@link MOCK_GENESIS}
 * placeholder, and the runtime config must carry the same value.
 */
import {
  createLoopbackStatements,
  decodeStatement,
  encodeStatement,
  TOPIC_FIELD_TAGS,
} from "./loopback-statements.js";
import type {
  LoopbackStatements,
  RetainedStatement,
  StatementInput,
} from "./loopback-statements.js";

export interface ChainProxy {
  /**
   * Genesis hash to route on. Omit to take every request no hashed entry
   * claims.
   *
   * Omitting it is usually right for a single-chain suite, and is immune to a
   * reset: a public testnet's genesis changes when it is reset, which silently
   * breaks hash routing and is exactly how a pinned hash goes stale. Give a
   * hash only when routing between several chains, and expect to re-pin it.
   */
  genesisHash?: string;
  /** WebSocket endpoint, e.g. `wss://paseo-asset-hub-next-rpc.polkadot.io`. */
  rpcUrl: string;
  /**
   * Serve the statement store in-page for this chain rather than forwarding it.
   *
   * Everything else still goes to `rpcUrl`, so a chain can carry real reads and
   * a local statement store at once. A statement accepted here is registered
   * nowhere, so a real store would refuse it.
   */
  loopbackStatements?: boolean;
}

/** Optional error injection, mirroring the Rust `MockFaults`. */
export interface MockFaults {
  /** Product and core storage reads/writes/clears fail with this reason. */
  storageError?: string;
  /** `navigateTo` fails with this reason. */
  navigateError?: string;
  /** `pushNotification` fails with this reason. */
  notificationError?: string;
  /**
   * `confirmUserAction` fails with this reason instead of answering.
   *
   * Distinct from a declined confirmation: the host could not put the question
   * to the user at all, which the core must not read as a refusal.
   */
  confirmationError?: string;
  /** Device and remote permission prompts fail with this reason. */
  permissionError?: string;
  /** `featureSupported` and `supportedChains` fail with this reason. */
  featureError?: string;
  /** Chat room, bot, and message calls fail with this reason. */
  chatError?: string;
}

/** Which prompt surface a permission decision came from. */
export type PermissionKind = "device" | "remote";

/**
 * One notification the product pushed, recorded for assertions.
 *
 * Field names are `@parity/host-api-test-sdk`'s `NotificationLogEntry`, so an
 * assertion written against that shape reads this one. The entry outlives the
 * request:
 * `cancelled` flips in place when the product cancels by id, which is what a
 * suite asserts on rather than a separate cancellation list.
 */
export interface NotificationLogEntry {
  /** Host-assigned id, the same one `cancelNotification` takes. */
  id: number;
  /** Notification text. */
  text: string;
  /** Optional URL to open on tap. */
  deeplink: string | undefined;
  /** Delivery time in epoch-ms, or undefined for immediate. */
  scheduledAt: bigint | undefined;
  /** Set once the product cancels this notification. */
  cancelled: boolean;
  /** When the mock recorded it. */
  timestamp: number;
}

/** A subscription that reports an injected fault instead of opening. */
async function* failedSubscription<T>(
  reason: string,
): AsyncGenerator<Result<T, GenericError>> {
  yield err({ reason });
}

/**
 * An open subscription seeded with `first`, live from the moment it is created.
 *
 * `register` is called here rather than from inside the generator, whose body
 * does not run until its first `next()`. A change landing before that would
 * reach no subscriber and is never re-sent, so the consumer would park on a
 * value that is already stale. The Rust `MockPlatform` registers its sender
 * synchronously for the same reason.
 */
function liveSubscription<T>(
  first: T,
  closers: Set<() => void>,
  register: (push: (item: T) => void) => () => void,
): AsyncGenerator<Result<T, GenericError>> {
  const pending: T[] = [first];
  let wake: (() => void) | undefined;
  let released = false;
  const unregister = register((item: T) => {
    pending.push(item);
    wake?.();
    wake = undefined;
  });
  // Idempotent because the two paths below overlap: closing a stream that has
  // been iterated runs the generator's `finally` as well as `return`.
  //
  // Waking is what lets a parked body finish. Unregistering alone would leave
  // it waiting on a push that can no longer arrive, so a `return()` queued
  // behind it, and the `next()` it is parked on, would both hang.
  const release = () => {
    if (released) return;
    released = true;
    closers.delete(release);
    unregister();
    wake?.();
    wake = undefined;
  };
  closers.add(release);
  const stream = (async function* () {
    try {
      for (;;) {
        while (pending.length > 0) yield ok(pending.shift()!);
        if (released) return;
        await new Promise<void>((resolve) => {
          wake = resolve;
        });
        if (released) return;
      }
    } finally {
      release();
    }
  })();
  // `return`/`throw` release directly rather than relying on that `finally`.
  // A generator whose body has never run has nothing to unwind, so closing a
  // subscription created but not yet iterated would otherwise leave it
  // registered and pushing into a queue no one reads.
  const close = stream.return.bind(stream);
  const fail = stream.throw.bind(stream);
  stream.return = (value) => {
    release();
    return close(value);
  };
  stream.throw = (error) => {
    release();
    return fail(error);
  };
  return stream;
}

/** One operation a product began and has not ended. */
export interface OpenOperation {
  /** Product that began it. */
  productId: string;
  /** Id the mock handed back, unique among this product's open operations. */
  id: number;
  /** Label the product gave, empty when it gave none. */
  label: string;
}

/**
 * One permission answer the mock gave, recorded for assertions.
 *
 * Field names are `@parity/host-api-test-sdk`'s `PermissionLogEntry`, so an
 * assertion written against that shape reads this one.
 */
/**
 * One statement the store holds, in the shape a suite reads it.
 *
 * Field names are `@parity/host-api-test-sdk`'s `StatementEntry`, so an
 * assertion written against that shape reads this one.
 */
export interface StatementEntry {
  /** Topics the statement carries, `0x`-hex, in the order encoded. */
  topics: string[];
  /** The statement's payload, or `undefined` when it carries none. */
  data: string | undefined;
  /** The signature proof, when the statement carries a signing one. */
  proof: { signature: string; signer: string } | undefined;
  /** True when the product submitted it, false when a test injected it. */
  fromProduct: boolean;
  /** When the store took it, as epoch milliseconds. */
  timestamp: number;
}

export interface PermissionLogEntry {
  /** The request's tag, the same key `grantPermission` takes. */
  tag: string;
  /** The full request, for a permission that carries one. */
  value: unknown;
  /** What the mock answered. */
  approved: boolean;
  /** Which prompt surface asked. */
  kind: PermissionKind;
  /**
   * The answer's lifetime, as the core records it.
   *
   * A mock policy is two-valued, so this is `AllowAlways` or `Deny`; a host
   * that offered `AllowOnce` would record that instead. Carried because an
   * assertion on a refusal reads the lifetime, not just the boolean: a suite
   * checking that a denial was durable has nothing else to look at.
   */
  decision: PermissionDecision;
  /** When the mock answered, as epoch milliseconds. */
  timestamp: number;
}

/**
 * One signing request the core put to the host.
 *
 * The signing-shaped view of {@link MockHost.reviews}: a TrUAPI host confirms
 * signatures rather than performing them, so what it sees is the review, and
 * `payload` is that review's own payload rather than a host-assembled one.
 */
export interface SigningLogEntry {
  /** Which signing request was reviewed. */
  type: "payload" | "raw" | "createTransaction";
  /** The reviewed request. */
  payload: unknown;
}

/** One chat message the product posted through the mock. */
export interface ChatMessageRecord {
  /** Id the mock assigned and returned to the product. */
  messageId: string;
  /** Room the message was posted to. */
  roomId: string;
  /** What was posted. */
  payload: ChatMessageContent;
}

/** State of the mock's chain connection, as the host sees it. */
export type ChainStatus = "Idle" | "Connected" | "Disconnected";

/**
 * A domain TrUAPI declares but no host implements.
 *
 * Reaching one throws a descriptive error rather than failing later with
 * `undefined is not a function`, and never fakes a success for a path the real
 * host cannot execute.
 */
const NOT_MODELLED_REASONS: Record<string, string> = {
  payment:
    "the protocol declares payments but no host implements them; " +
    "see docs/rfcs/0006-payments.md",
  coinPayment:
    "the protocol declares coin payments but no host implements them; " +
    "see docs/rfcs/0006-payments.md",
  statements:
    "the core owns the statement store and submits it over the people chain, " +
    "so there is no host seam for the mock to record or inject through",
};

function notModeled<T extends object>(domain: string): T {
  return new Proxy({} as T, {
    get(_target, property) {
      const reason =
        NOT_MODELLED_REASONS[domain] ?? "no host implements this domain";
      throw new Error(
        `${domain}.${String(property)} is not available in the TrUAPI mock ` +
          `host: ${reason}.`,
      );
    },
  });
}

/** Behavior knobs for {@link createMockHost}. */
export interface MockHostConfig {
  /** Answer for `devicePermission`. Default `"allow-all"`. */
  devicePermissions?: PermissionPolicy;
  /** Answer for `remotePermission`. Default `"allow-all"`. */
  remotePermissions?: PermissionPolicy;
  /** Whether `featureSupported` reports support. Default `true`. */
  featureSupported?: boolean;
  /** Theme emitted by `subscribeTheme`. Default `"Dark"`. */
  theme?: ThemeVariant;
  /** BCP 47 tag emitted by `subscribeLocale`. Default `"en"`. */
  languageTag?: string;
  /** Whether `confirmUserAction` confirms reviewed actions. Default `true`. */
  confirmUserActions?: boolean;
  /**
   * JSON-RPC response frames the chain connection replays, in order. Empty
   * (the default) means a silent connection: it records outbound requests and
   * never answers, so chain-dependent flows park.
   */
  chainResponses?: string[];
  /**
   * When `true`, the chain response stream ends immediately instead of parking,
   * so disconnect/timeout paths can be asserted (fail-fast). Ignored when
   * `chainResponses` is non-empty.
   */
  chainClosed?: boolean;
  /**
   * Error injection. When a field is set, the matching host call rejects with
   * that reason instead of succeeding.
   */
  faults?: MockFaults;
  /**
   * Chains to proxy to a real node instead of answering from memory.
   *
   * Empty by default, which keeps the host hermetic: nothing reaches the
   * network. Supplying an entry trades that away for real chain behaviour --
   * inclusion, finalization, live state -- and inherits the flakiness that
   * comes with it, including state other runs left behind and contracts that
   * were reaped. Proxy only the chains a suite genuinely needs.
   *
   * Connections open lazily, on the first request for a matching hash, so a
   * declared proxy that is never used opens no socket.
   */
  chainProxies?: ChainProxy[];
  /**
   * Chains the host reports serving (RFC 0026). Defaults to the three
   * {@link MOCK_GENESIS} chains, which are what {@link mockRuntimeConfig}
   * declares.
   *
   * An empty set type-checks and then fails every chain-routed call, so
   * override this only to assert that failure.
   */
  supportedChains?: HostChainSet;
}

/** A mock host: the callbacks to wire into a provider, plus assertion oracles. */
export interface MockHost {
  /**
   * The nested host-callback surface. Pass to `createWasmRawCallbacks` or hand
   * to `createWebWorkerPairingHostRuntime` (both accept `RequiredHostCallbacks`).
   */
  callbacks: RequiredHostCallbacks;
  /** URLs the core asked the host to open, in order. */
  getNavigationLog(): string[];
  /** Notifications the core asked the host to show, in order. */
  getNotificationLog(): NotificationLogEntry[];
  /**
   * Deliver a statement to the product as a chain notification, answering the
   * entry the store retained.
   *
   * Takes the topics and payload as a structure, or the SCALE wire bytes the
   * chain would have sent. Retained either way, so a suite that injects before
   * its product subscribes has the statement replayed to it on subscribe
   * rather than losing it.
   */
  injectStatement(statement: StatementInput | Uint8Array | string): StatementEntry;
  /** Statements injected so far, in order, as `0x` hex. */
  getInjectedStatements(): string[];
  /**
   * Every statement the store holds, submitted or injected, in order.
   *
   * Decoded, because the wire form is the core's business: a suite asserting on
   * a topic or a payload should not have to know the codec to read one back.
   */
  getStatements(): StatementEntry[];
  /**
   * Statements the product submitted, decoded, read off the chain transport.
   *
   * The core sends `statement_submit` without consulting an allowance, so a
   * product that signs its own statements is observable here. One that asks the
   * host to sign first (`createProofAuthorized`) needs a statement allowance to
   * get that far, and this stays empty until it has one.
   */
  getSubmittedStatements(): StatementEntry[];
  /** Forget the injected statements. Delivered ones cannot be recalled. */
  clearStatements(): void;
  /** Raw JSON-RPC the core sent over the chain connection, in order. */
  sentRpc(): string[];
  /** Auth-state transitions the core emitted, in order. */
  authStates(): AuthState[];
  /**
   * Full confirmation reviews the core requested, in order.
   *
   * Carries the reviewed payload, not just its kind: a `SignRaw` review holds
   * the bytes the product asked to have signed, so a test can assert *what*
   * was put to the user rather than only that something was.
   */
  reviews(): UserConfirmationReview[];
  /** Confirmation kinds the core requested (review `tag`s), in order. */
  confirmations(): string[];
  /**
   * Signing requests the core put to the host, in order.
   *
   * A filtered view of {@link MockHost.reviews}: only the reviews that gate a
   * signature, shaped the way a signing log is usually read.
   */
  getSigningLog(): SigningLogEntry[];
  /**
   * How many calls the core has made into the host.
   *
   * Non-zero is the first observable evidence that frames are crossing the
   * wire, which is what a harness waits on before asserting anything.
   */
  getHostCallCount(): number;
  /** Whether the core has reported an authenticated session. */
  getIsAuthenticated(): boolean;
  /**
   * Whether the product-host link is up.
   *
   * The mock has no transport of its own, so this reports the chain-side
   * connection it does model; a harness owning the real product link should
   * report that instead.
   */
  getConnectionStatus(): ChainStatus;
  /** Switch the answer both permission prompts fall back to. */
  setPermissionBehavior(behavior: PermissionPolicy | PermissionPolicyAlias): void;
  /**
   * Release the mock's state and drop every live subscription.
   *
   * {@link MockHost.reset} leaves subscriptions open and tells them what
   * changed; this ends them, so a host kept across a suite does not carry a
   * previous case's subscribers.
   */
  dispose(): void;
  /** Permission answers the mock gave, in order. */
  getPermissionLog(): PermissionLogEntry[];
  /** Operations a product began and has not ended, in the order they began. */
  getOpenOperations(): OpenOperation[];
  /** Permissions with an explicit grant, in key order. */
  getGrantedPermissions(): string[];
  /** Answer `permission` with a grant, whatever the configured policy says. */
  grantPermission(permission: string): void;
  /** Answer `permission` with a denial, whatever the configured policy says. */
  revokePermission(permission: string): void;
  /** Drop the explicit answer for `permission`, restoring policy fallback. */
  resetPermission(permission: string): void;
  /**
   * When enforcing, deny every permission without an explicit grant instead of
   * falling back to the configured policy. Off by default.
   */
  setEnforcePermissions(enforce: boolean): void;
  /** The theme the mock currently reports. */
  getTheme(): ThemeVariant;
  /** Replace the reported theme. */
  setTheme(variant: ThemeVariant): void;
  /** State of the mock's chain connection. */
  getChainStatus(): ChainStatus;
  /** Mark the chain disconnected, as a dropped transport would. */
  simulateDisconnect(): void;
  /** Allow connections again after a simulated disconnect. */
  simulateReconnect(): void;
  /** Chat rooms the product registered. */
  getChatRooms(): ChatRoom[];
  /** Chat bots the product registered. */
  getChatBots(): HostChatRegisterBotRequest[];
  /** Messages the product posted, with the ids the mock assigned. */
  getChatMessageLog(): ChatMessageRecord[];
  /**
   * Product-scoped storage the core has written, keyed without the internal
   * namespace prefix.
   *
   * A test asserting what the product stored should read it here rather than
   * reach into whatever the host keeps underneath: the namespacing is an
   * implementation detail and tying a suite to it is what makes a host
   * impossible to replace.
   */
  getProductStorage(): Record<string, Uint8Array>;
  /**
   * The value the product stored under `key`, decoded as UTF-8.
   *
   * Synchronous, because `@parity/host-api-test-sdk` publishes it that way and
   * a suite compares the result inside a `page.waitForFunction` predicate.
   * The core namespaces the key before the host sees it, so this reads the
   * product's own key out of that shape rather than matching the whole string.
   *
   * `undefined` for a value that is not UTF-8, the same answer a key nothing
   * wrote gets: read those through {@link MockHost.getProductStorage}, which
   * hands back bytes.
   */
  getProductStorageValue(key: string): string | undefined;
  /** Seeded preimage values. */
  getPreimages(): Uint8Array[];
  /** Drop the recorded navigations. */
  clearNavigationLog(): void;
  /** Drop the recorded shown and cancelled notifications. */
  clearNotificationLog(): void;
  /** Drop the recorded confirmation reviews. */
  clearSigningLog(): void;
  /** Drop the recorded permission answers, keeping explicit grants. */
  clearPermissionLog(): void;
  /** Drop every explicit permission grant and denial. */
  clearPermissionDecisions(): void;
  /** Drop the recorded auth-state transitions. */
  clearAuthStates(): void;
  /** Drop the recorded outbound JSON-RPC. */
  clearSentRpc(): void;
  /** Drop the seeded preimages. */
  clearPreimages(): void;
  /** Drop the product and core storage contents. */
  clearStorage(): void;
  /** Drop the registered rooms and bots and the posted-message log. */
  clearChatState(): void;
  /**
   * Return the mock to its freshly-constructed state, keeping its config.
   *
   * Tests reset between cases; doing it in one call is what keeps a recording
   * from one case out of the assertions of the next.
   */
  reset(): void;
  /**
   * The statement store, which the core owns and submits over the people
   * chain. Every access throws: there is no host seam to record or inject
   * through, so the mock cannot model it without chain support.
   */
  statements: never;
  /**
   * Payments, which TrUAPI declares but no host implements. Every access
   * throws; see {@link notModeled}.
   */
  payment: never;
  /** Coin payments, unimplemented in the same way as {@link MockHost.payment}. */
  coinPayment: never;
  /** Notification ids the core asked the host to cancel, in order. */
  cancelledNotifications(): number[];
  /**
   * Seed a preimage so a later `preimage.lookupPreimage` resolves it, and
   * return the deterministic lookup key. The core (not the host) owns Bulletin
   * submission on current core; this is the host-side content store the mock's
   * `lookupPreimage` reads from.
   */
  seedPreimage(value: Uint8Array): Uint8Array;
}

/** Lowercase hex without `0x`, so a hash compares equal however it was written. */
function normalizeHash(hash: string | Uint8Array): string {
  if (typeof hash !== "string") {
    return Array.from(hash, (b) => b.toString(16).padStart(2, "0")).join("");
  }
  return hash.replace(/^0x/i, "").toLowerCase();
}

/**
 * Open a real WebSocket to a chain and adapt it to `JsonRpcConnection`.
 *
 * Requests are still recorded in `sentRpc`, so a test can assert what the core
 * asked for even when a real node answers it.
 *
 * One socket per lease, never shared. Production opens a fresh connection for
 * every chain connect -- `truapi-provider`'s `connect` and the CLI's
 * `WsJsonRpcConnection::connect` both do -- and JSON-RPC ids are numbered per
 * connection from 1. Two leases on one socket therefore put two id spaces on
 * the same wire, and every inbound frame reaches every reader, so a lease sees
 * traffic it never asked for. That is transport behaviour no real host has and
 * the core can observe it, which makes sharing a fidelity bug rather than an
 * optimisation. Do not reintroduce it to save connections.
 *
 * This is not a multi-chain concern. Two leases on one chain are enough: they
 * would cross-talk, and releasing one would leave its listeners on a socket the
 * other still holds. A single-chain suite is not safe from it.
 */
/** Record a `statement_submit` the core sent to a real chain. */
function recordChainSubmission(request: string, into: RetainedStatement[]): void {
  try {
    const frame = JSON.parse(request) as { method?: string; params?: unknown[] };
    if (frame.method !== "statement_submit") return;
    const [statement] = frame.params ?? [];
    if (typeof statement !== "string") return;
    into.push({ encoded: statement, fromProduct: true, timestamp: Date.now() });
  } catch {
    // A frame that is not JSON is not a submission.
  }
}

function connectToChain(
  proxy: ChainProxy,
  sentRpc: string[],
  statementSubscriptions?: Set<string>,
  loopback?: LoopbackStatements,
  // Injectors are held beside the connection rather than on it: the connection
  // type is generated from the protocol and must not grow test-only members.
  injectors?: Set<(frame: string) => void>,
  disconnectors?: Set<() => void>,
  submissions?: RetainedStatement[],
): JsonRpcConnection {
  const socket = new WebSocket(proxy.rpcUrl);
  const queued: string[] = [];
  const waiting: ((value: IteratorResult<string>) => void)[] = [];
  let closed = false;
  // Ids of `statement_subscribeStatement` requests, so the chain's reply to one
  // can be recognised and its subscription id recorded. Injection needs that
  // id: a notification carrying any other one is dropped by the core.
  const pendingStatementRequests = new Set<string>();

  const open = new Promise<void>((resolve, reject) => {
    socket.addEventListener("open", () => resolve(), { once: true });
    socket.addEventListener(
      "error",
      () => reject(new Error(`chain proxy failed to connect to ${proxy.rpcUrl}`)),
      { once: true },
    );
  });

  const deliver = (text: string) => {
    const next = waiting.shift();
    if (next) next({ value: text, done: false });
    else queued.push(text);
  };

  socket.addEventListener("message", (event: MessageEvent) => {
    const text = typeof event.data === "string" ? event.data : "";
    if (!text) return;
    if (statementSubscriptions) recordStatementSubscription(text);
    deliver(text);
  });

  /** Record the subscription id the chain assigned to a statement subscribe. */
  const ownStatementSubscriptions = new Set<string>();
  const recordStatementSubscription = (text: string) => {
    try {
      const frame = JSON.parse(text) as {
        id?: string;
        result?: unknown;
      };
      if (
        typeof frame.id === "string" &&
        pendingStatementRequests.delete(frame.id) &&
        typeof frame.result === "string"
      ) {
        statementSubscriptions?.add(frame.result);
        ownStatementSubscriptions.add(frame.result);
      }
    } catch {
      // A frame that is not JSON is not a subscribe reply; the core still gets
      // it, because parsing here must never drop chain traffic.
    }
  };
  const inject = (frame: string) => {
    if (!closed) deliver(frame);
  };
  injectors?.add(inject);

  const finish = () => {
    closed = true;
    injectors?.delete(inject);
    disconnectors?.delete(finish);
    // A closed connection's subscriptions are gone with it, and leaving the
    // ids behind makes `injectStatement` report deliveries to nobody.
    for (const id of ownStatementSubscriptions)
      statementSubscriptions?.delete(id);
    ownStatementSubscriptions.clear();
    loopback?.release(deliver);
    // Release every reader, so a stream ends instead of hanging on a drop.
    while (waiting.length > 0) waiting.shift()?.({ value: undefined, done: true });
  };
  disconnectors?.add(finish);
  socket.addEventListener("close", finish, { once: true });

  return {
    send(request) {
      sentRpc.push(request);
      if (submissions) recordChainSubmission(request, submissions);
      // Served here rather than forwarded, so the statement flows work with no
      // chain behind them. Everything else still goes out.
      if (loopback?.handle(request, deliver)) return;
      if (statementSubscriptions) {
        try {
          const frame = JSON.parse(request) as { id?: string; method?: string };
          if (
            frame.method === "statement_subscribeStatement" &&
            typeof frame.id === "string"
          ) {
            pendingStatementRequests.add(frame.id);
          }
        } catch {
          // Not JSON: nothing to track, and the send still goes out.
        }
      }
      // Sends before the socket is up are queued by the promise, not dropped.
      void open.then(() => {
        if (!closed) socket.send(request);
      });
    },
    responses(): AsyncIterable<string> {
      return {
        [Symbol.asyncIterator]() {
          return {
            next(): Promise<IteratorResult<string>> {
              const buffered = queued.shift();
              if (buffered !== undefined) {
                return Promise.resolve({ value: buffered, done: false });
              }
              if (closed) return Promise.resolve({ value: undefined, done: true });
              return new Promise((resolve) => waiting.push(resolve));
            },
          };
        },
      };
    },
    close() {
      // The socket belongs to this lease alone, so ending the lease ends it.
      // Leaving it open would leak the connection and this lease's listeners.
      finish();
      socket.close();
    },
  };
}

/**
 * Content address of a preimage value: blake2b-256 of the raw bytes.
 *
 * This is the key the core derives before it asks the host to look a preimage
 * up, and it discards any value whose hash does not match the key it asked
 * for. A key computed any other way is unreachable through the core, however
 * well it round-trips against the mock alone.
 */
function preimageKey(value: Uint8Array): Uint8Array {
  return blake2b(value, { dkLen: 32 });
}

function hex(bytes: Uint8Array): string {
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

/**
 * Build an in-memory mock host. The returned `callbacks` implement every
 * `RequiredHostCallbacks` capability; the accessor methods expose what the core
 * did.
 */
export function createMockHost(config: MockHostConfig = {}): MockHost {
  const {
    devicePermissions: devicePermissionsInitial = "allow-all",
    remotePermissions: remotePermissionsInitial = "allow-all",
    featureSupported = true,
    theme = "Dark",
    confirmUserActions = true,
    chainResponses = [],
    chainClosed = false,
    chainProxies = [],
    languageTag = "en",
    faults = {},
    supportedChains = {
      network: "mock",
      chains: [
        { identifier: "People", genesisHash: MOCK_GENESIS.people },
        { identifier: "Bulletin", genesisHash: MOCK_GENESIS.bulletin },
        { identifier: "AssetHub", genesisHash: MOCK_GENESIS.assetHub },
      ],
    },
  } = config;

  const storage = new Map<string, Uint8Array>();
  const preimages = new Map<string, Uint8Array>();
  const navigations: string[] = [];
  const pushedNotifications: NotificationLogEntry[] = [];
  // Subscription ids the chain assigned to statement subscribes, and the live
  // chain connections a synthesized notification can be delivered through.
  const statementSubscriptions = new Set<string>();
  const chainInjectors = new Set<(frame: string) => void>();
  const chainDisconnectors = new Set<() => void>();
  const injectedStatements: string[] = [];
  // Submissions and injections seen on a real chain transport, so the readers
  // answer the same shape whether or not the store is served in-page.
  const chainStatements: RetainedStatement[] = [];
  const loopbackStatements = createLoopbackStatements();
  const usingLoopback = (chainProxies ?? []).some(
    (proxy) => proxy.loopbackStatements,
  );
  const sentRpc: string[] = [];
  const authStates: AuthState[] = [];
  const reviews: UserConfirmationReview[] = [];
  const cancelledNotifications: number[] = [];
  const permissionLog: PermissionLogEntry[] = [];
  const openOperations: OpenOperation[] = [];
  let nextOperationId = 0;
  const permissionDecisions = new Map<string, boolean>();
  const chatRooms = new Map<string, ChatRoom>();
  const chatBots = new Map<string, HostChatRegisterBotRequest>();
  const chatMessages: ChatMessageRecord[] = [];
  // Ids start at 1, not 0: the id is what the product cancels by, and a
  // product that treats 0 as "no id" cannot cancel the first notification it
  // ever schedules.
  let nextNotificationId = 1;
  let nextChatMessageId = 0;
  let devicePermissions = devicePermissionsInitial;
  let remotePermissions = remotePermissionsInitial;
  let enforcePermissions = false;
  let currentTheme = theme;
  // Live theme subscriptions, so `setTheme` reaches a subscribed product the way
  // the Rust mock's `theme_subscribers` does. A generator that ended after the
  // first value would make every `setTheme` in a migrating suite a no-op.
  /** Ends one live subscription each, so `dispose` can close them all. */
  const subscriptionClosers = new Set<() => void>();
  const themeSubscribers = new Set<(item: HostThemeSubscribeItem) => void>();
  const chatRoomSubscribers = new Set<
    (item: HostChatListSubscribeItem) => void
  >();
  /**
   * Entries in key order, which is the order the Rust mock's `BTreeMap`
   * yields. Insertion order would put a different list in the subscription
   * payload than the sibling host sends for the same rooms.
   */
  const byKey = <T>(entries: Map<string, T>): T[] =>
    [...entries.entries()]
      .sort(([left], [right]) => (left < right ? -1 : left > right ? 1 : 0))
      .map(([, value]) => value);
  const publishChatRooms = (): void => {
    const item = { rooms: byKey(chatRooms) };
    for (const push of chatRoomSubscribers) push(item);
  };
  let chainStatus: ChainStatus = "Idle";
  /**
   * One socket per proxied endpoint.
   *
   * A chainHead handshake is expensive, and the core opens a connection per
   * product. Pooling by URL means the second one costs nothing.
   */

  /**
   * Answer one permission prompt and record it.
   *
   * The key is the request's tag -- `"Camera"`, `"ChainSubmit"`. This scheme is
   * internal to the JS mock and deliberately independent of the Rust
   * `MockPlatform`'s `Display` keys: no state crosses that boundary, and only
   * the method names have to agree.
   */
  const decidePermission = (
    kind: PermissionKind,
    tag: string,
    value: unknown,
    policy: PermissionPolicy,
  ): boolean => {
    const explicit = permissionDecisions.get(tag);
    const approved =
      explicit !== undefined
        ? explicit
        : enforcePermissions
          ? false
          : granted(policy);
    permissionLog.push({
      tag,
      value,
      approved,
      kind,
      decision: decision(approved),
      timestamp: Date.now(),
    });
    return approved;
  };

  // Product keys are namespaced from core slots so neither can shadow the other.
  // This in-JS key scheme is internal and independent from the Rust MockPlatform's
  // (state never crosses the boundary), so the two need not match byte-for-byte.
  // What they do have to share is which slots are distinct: keying on the tag
  // alone would put every product's manifest in one slot, so a test writing one
  // product's and reading another's reads back the wrong one here and a miss on
  // Rust.
  const productKey = (key: string): string => `product:${key}`;
  const coreKey = (key: CoreStorageKey): string =>
    key.value === undefined
      ? `core:${key.tag}`
      : // Entries sorted, so a payload built field-by-field in a different
        // order still addresses the slot it addressed before.
        `core:${key.tag}:${JSON.stringify(key.value, (_, inner: unknown) =>
          inner !== null && typeof inner === "object" && !Array.isArray(inner)
            ? Object.fromEntries(
                Object.entries(inner as Record<string, unknown>).sort(
                  ([left], [right]) => left.localeCompare(right),
                ),
              )
            : inner,
        )}`;
  /**
   * Drop the core's stored answer for `permission`, so the next request asks
   * again.
   *
   * The core answers a settled permission from its own storage without calling
   * the host, which is the real behaviour. It also means that changing the
   * mock's answer after the first request changes nothing a product can see:
   * the decision it is now going to get was recorded before. A suite setting an
   * answer is saying what the host should reply, so the recorded one has to go
   * with it.
   *
   * Matched on the serialised key because the mock holds keys as strings: every
   * permission is named in its own key, a device one as `"Camera"` and a remote
   * one as `"ChainSubmit"`, so the quoted name selects that permission's slots
   * and no others.
   */
  const forgetStoredAuthorization = (permission: string): void => {
    const prefix = "core:PermissionAuthorization:";
    const needle = JSON.stringify(permission);
    for (const key of [...storage.keys()]) {
      if (key.startsWith(prefix) && key.includes(needle)) storage.delete(key);
    }
  };

  /** Decode a retained statement into the shape a suite reads. */
  const asEntry = (statement: RetainedStatement): StatementEntry => {
    // An undecodable statement is still reported, with nothing claimed about
    // its contents: a suite chasing one it injected by hand has something to
    // see, where dropping it looks like the injection never happened.
    const fields = decodeStatement(statement.encoded) ?? [];
    const proof = fields.find((field) => field.tag === "Proof")?.value as
      | { tag: string; value: { signature: string; signer: string } }
      | undefined;
    return {
      topics: fields
        .filter((field) => TOPIC_FIELD_TAGS.includes(field.tag))
        .map((field) => String(field.value).toLowerCase()),
      data: fields.find((field) => field.tag === "Data")?.value as
        | string
        | undefined,
      proof:
        proof && proof.tag !== "OnChain"
          ? { signature: proof.value.signature, signer: proof.value.signer }
          : undefined,
      fromProduct: statement.fromProduct,
      timestamp: statement.timestamp,
    };
  };

  const granted = (policy: PermissionPolicy): boolean => policy === "allow-all";
  // A mock policy is two-valued, so a grant is durable and a refusal is
  // durable. `AllowOnce` is a host answer the mock has no knob to ask for.
  const decision = (approved: boolean): PermissionDecision =>
    approved ? "AllowAlways" : "Deny";
  // Per key, not one broadcast: a subscriber woken by every write would make a
  // test asserting "no change" pass for the wrong reason.
  const storageSubscribers = new Map<
    string,
    Set<(item: HostLocalStorageChangeItem) => void>
  >();
  const publishStorage = (key: string, value?: Uint8Array): void => {
    const item = { value: value && scale.bytesToHex(value) };
    for (const push of storageSubscribers.get(key) ?? []) push(item);
  };

  let hostCallCount = 0;

  /**
   * Wrap every callback in a namespace so each core->host call is counted.
   *
   * A test needs to know the wire is live before asserting on anything, and
   * the only honest evidence of that is the core actually having called the
   * host. Counting is cheap and needs no per-capability bookkeeping.
   */
  const countCallsIn = (namespace: Record<string, unknown>): void => {
    for (const [name, value] of Object.entries(namespace)) {
      if (typeof value !== "function") continue;
      const original = value as (...a: unknown[]) => unknown;
      namespace[name] = (...args: unknown[]) => {
        hostCallCount += 1;
        return original.apply(namespace, args);
      };
    }
  };

  // `RequiredHostCallbacks` (each capability wrapped in `Required<…>`): every
  // optional callback must be present, so a capability added to the generated
  // surface fails `tsc` here until the mock covers it. This is the load-bearing
  // coverage guarantee. `createWasmRawCallbacks` accepts this nested shape.
  const callbacks: RequiredHostCallbacks = {
    productStorage: {
      async read(key) {
        if (faults.storageError) throw new Error(faults.storageError);
        return storage.get(productKey(key));
      },
      async write(key, value) {
        if (faults.storageError) throw new Error(faults.storageError);
        storage.set(productKey(key), value);
        publishStorage(key, value);
      },
      async clear(key) {
        if (faults.storageError) throw new Error(faults.storageError);
        storage.delete(productKey(key));
        publishStorage(key, undefined);
      },
      subscribeStorage(key) {
        const current = storage.get(productKey(key));
        return liveSubscription<HostLocalStorageChangeItem>(
          { value: current && scale.bytesToHex(current) },
          subscriptionClosers,
          (push) => {
            const subscribers =
              storageSubscribers.get(key) ??
              new Set<(item: HostLocalStorageChangeItem) => void>();
            storageSubscribers.set(key, subscribers);
            subscribers.add(push);
            return () => {
              subscribers.delete(push);
              if (subscribers.size === 0) storageSubscribers.delete(key);
            };
          },
        );
      },
    },

    productOperations: {
      async beginOperation(product, label) {
        const id = nextOperationId++;
        openOperations.push({ productId: product.productId, id, label });
        return { id };
      },
      async endOperation(product, id) {
        // Idempotent by contract, so an unknown or already-ended id is fine.
        // Per product too: another product holding the same id must not drop
        // this one's demand.
        const at = openOperations.findIndex(
          (open) => open.id === id && open.productId === product.productId,
        );
        if (at !== -1) openOperations.splice(at, 1);
      },
    },

    coreStorage: {
      async readCoreStorage(key) {
        if (faults.storageError) throw new Error(faults.storageError);
        return storage.get(coreKey(key));
      },
      async writeCoreStorage(key, value) {
        if (faults.storageError) throw new Error(faults.storageError);
        storage.set(coreKey(key), value);
      },
      async clearCoreStorage(key) {
        if (faults.storageError) throw new Error(faults.storageError);
        storage.delete(coreKey(key));
      },
    },

    navigation: {
      async navigateTo(url) {
        if (faults.navigateError) throw new Error(faults.navigateError);
        navigations.push(url);
      },
    },

    notifications: {
      async pushNotification(notification) {
        if (faults.notificationError) throw new Error(faults.notificationError);
        const id = nextNotificationId++;
        pushedNotifications.push({
          id,
          text: notification.text,
          deeplink: notification.deeplink,
          scheduledAt: notification.scheduledAt,
          cancelled: false,
          timestamp: Date.now(),
        });
        return { id };
      },
      async cancelNotification(id) {
        cancelledNotifications.push(id);
        const entry = pushedNotifications.find((n) => n.id === id);
        if (entry) entry.cancelled = true;
      },
    },

    permissions: {
      async devicePermission(_product, request) {
        if (faults.permissionError) throw new Error(faults.permissionError);
        return decision(
          decidePermission("device", request, request, devicePermissions),
        );
      },
      async remotePermission(_product, request) {
        if (faults.permissionError) throw new Error(faults.permissionError);
        return decision(
          decidePermission(
            "remote",
            request.permission.tag,
            request.permission,
            remotePermissions,
          ),
        );
      },
    },

    features: {
      async featureSupported() {
        if (faults.featureError) throw new Error(faults.featureError);
        return { supported: featureSupported };
      },
      async supportedChains() {
        if (faults.featureError) throw new Error(faults.featureError);
        return supportedChains;
      },
    },

    chain: {
      async connect(genesisHash): Promise<JsonRpcConnection> {
        // A simulated disconnect blocks reconnect until `simulateReconnect`,
        // the way the Rust mock does, so a suite testing recovery sees the
        // failure it is testing for rather than a connection that succeeds.
        if (chainStatus === "Disconnected") {
          throw new Error("mock chain is disconnected");
        }
        // A hashed entry wins; an unhashed one takes whatever is left.
        const proxy =
          chainProxies.find(
            (candidate) =>
              candidate.genesisHash !== undefined &&
              normalizeHash(candidate.genesisHash) === normalizeHash(genesisHash),
          ) ?? chainProxies.find((candidate) => candidate.genesisHash === undefined);
        if (proxy) {
          // After the dial, not before: a proxy that fails to open must leave
          // the status alone rather than report a connection that is not there.
          const connection = connectToChain(
            proxy,
            sentRpc,
            statementSubscriptions,
            proxy.loopbackStatements ? loopbackStatements : undefined,
            chainInjectors,
            chainDisconnectors,
            chainStatements,
          );
          chainStatus = "Connected";
          return connection;
        }
        chainStatus = "Connected";
        // A proxied connection registers its disconnector inside
        // `connectToChain`; this one is held by nothing else, so it registers
        // its own. Without it `simulateDisconnect` would leave the stream below
        // parked and only the reported status would change.
        let dropped: (() => void) | undefined;
        const transportDropped = new Promise<void>((resolve) => {
          dropped = resolve;
        });
        const disconnect = () => dropped?.();
        chainDisconnectors.add(disconnect);
        return {
          send(request) {
            sentRpc.push(request);
            recordChainSubmission(request, chainStatements);
          },
          async *responses(): AsyncGenerator<string> {
            try {
              for (const frame of chainResponses) {
                yield frame;
              }
              if (chainResponses.length === 0 && !chainClosed) {
                // Silent: yields nothing, so chain-dependent flows park until
                // the transport drops. `chainClosed` instead ends the stream
                // here for fail-fast disconnect tests.
                await transportDropped;
              }
            } finally {
              chainDisconnectors.delete(disconnect);
            }
          },
          // The mock holds no real transport, so releasing the lease is a no-op.
          // Note: a Silent connection whose `responses()` stream is already parked
          // stays parked after close() — tests that need the stream to terminate use
          // `simulateDisconnect`, `chainClosed`, or scripted frames, not close().
          close() {},
        };
      },
    },

    auth: {
      authStateChanged(state) {
        authStates.push(state);
      },
    },

    userConfirmation: {
      async confirmUserAction(review) {
        reviews.push(review);
        if (faults.confirmationError) throw new Error(faults.confirmationError);
        return confirmUserActions;
      },
      // The Rust trait answers this from `confirm_user_action` by default, so
      // a review is recorded here too and one knob still answers both.
      async confirmPermission(review) {
        reviews.push(review);
        if (faults.confirmationError) throw new Error(faults.confirmationError);
        return decision(confirmUserActions);
      },
    },

    theme: {
      subscribeTheme() {
        return liveSubscription<HostThemeSubscribeItem>(
          { name: { tag: "Default" }, variant: currentTheme },
          subscriptionClosers,
          (push) => {
            themeSubscribers.add(push);
            return () => themeSubscribers.delete(push);
          },
        );
      },
    },

    chat: {
      async createChatRoom(_product, request) {
        if (faults.chatError) throw new Error(faults.chatError);
        if (chatRooms.has(request.roomId)) return { status: "Exists" };
        chatRooms.set(request.roomId, {
          roomId: request.roomId,
          // A product that creates a room hosts it; a product reaching a room
          // as a bot registers the bot instead.
          participatingAs: "RoomHost",
        });
        publishChatRooms();
        return { status: "New" };
      },
      async registerChatBot(_product, request) {
        if (faults.chatError) throw new Error(faults.chatError);
        if (chatBots.has(request.botId)) return { status: "Exists" };
        chatBots.set(request.botId, request);
        return { status: "New" };
      },
      async postChatMessage(
        _product,
        request,
      ): Promise<HostChatPostMessageResponse> {
        if (faults.chatError) throw new Error(faults.chatError);
        // Posting to a room the product never registered is a product bug,
        // and a mock that silently accepted it would hide one.
        if (!chatRooms.has(request.roomId)) {
          throw new Error(`unknown chat room ${request.roomId}`);
        }
        const messageId = `mock-message:${nextChatMessageId++}`;
        chatMessages.push({
          messageId,
          roomId: request.roomId,
          payload: request.payload,
        });
        return { messageId };
      },
      subscribeChatRooms() {
        // The other chat calls fail with this reason, so the subscription
        // reports it too rather than handing back a stream that looks healthy
        // and never carries the rooms a failing host would refuse to list.
        if (faults.chatError) {
          return failedSubscription<HostChatListSubscribeItem>(faults.chatError);
        }
        return liveSubscription<HostChatListSubscribeItem>(
          { rooms: byKey(chatRooms) },
          subscriptionClosers,
          (push) => {
            chatRoomSubscribers.add(push);
            return () => chatRoomSubscribers.delete(push);
          },
        );
      },
    },

    locale: {
      async *subscribeLocale(): AsyncGenerator<
        Result<HostLocaleSubscribeItem, GenericError>
      > {
        yield ok({ languageTag });
        // A live subscription never ends, matching `subscribeTheme`.
        await new Promise<never>(() => {});
      },
    },

    preimage: {
      async *lookupPreimage(
        key,
      ): AsyncGenerator<Result<Uint8Array | undefined, GenericError>> {
        yield ok(preimages.get(hex(key)));
        // Stay open for future updates (none, in the mock).
        await new Promise<never>(() => {});
      },
    },
  };

  // Wrapped in place so the declared `RequiredHostCallbacks` type is preserved
  // rather than cast back on.
  for (const namespace of Object.values(callbacks)) {
    countCallsIn(namespace as Record<string, unknown>);
  }

  return {
    callbacks,
    getNavigationLog: () => [...navigations],
    getNotificationLog: () => pushedNotifications.map((n) => ({ ...n })),
    injectStatement: (statement) => {
      const encoded =
        typeof statement === "string"
          ? statement.startsWith("0x")
            ? statement
            : `0x${statement}`
          : statement instanceof Uint8Array
            ? `0x${hex(statement)}`
            : encodeStatement(statement);
      injectedStatements.push(encoded);
      if (usingLoopback) return asEntry(loopbackStatements.inject(encoded));

      const entry = { encoded, fromProduct: false, timestamp: Date.now() };
      chainStatements.push(entry);
      for (const subscription of statementSubscriptions) {
        // The envelope the chain sends, not the bare statement: the core reads
        // `result.data.statements`, so a bare value decodes to nothing.
        const frame = JSON.stringify({
          jsonrpc: "2.0",
          method: "statement_subscribeStatement",
          params: {
            subscription,
            result: {
              event: "newStatements",
              data: { statements: [encoded], remaining: 0 },
            },
          },
        });
        for (const injector of chainInjectors) injector(frame);
      }
      return asEntry(entry);
    },
    getInjectedStatements: () => [...injectedStatements],
    getStatements: () =>
      (usingLoopback ? loopbackStatements.statements() : chainStatements).map(
        asEntry,
      ),
    getSubmittedStatements: () =>
      (usingLoopback ? loopbackStatements.submitted() : chainStatements)
        .filter((entry) => entry.fromProduct)
        .map(asEntry),
    clearStatements: () => {
      injectedStatements.length = 0;
      loopbackStatements.clear();
      // The chain path retains its own list, which `getStatements` and
      // `getSubmittedStatements` read when the loopback store is off. Leaving it
      // would carry one case's statements into the next.
      chainStatements.length = 0;
    },
    sentRpc: () => [...sentRpc],
    authStates: () => [...authStates],
    reviews: () => [...reviews],
    confirmations: () => reviews.map((review) => review.tag),
    getSigningLog: () =>
      reviews.flatMap((review) => {
        const type =
          review.tag === "SignRaw"
            ? ("raw" as const)
            : review.tag === "SignPayload"
              ? ("payload" as const)
              : review.tag === "CreateTransaction"
                ? ("createTransaction" as const)
                : undefined;
        return type === undefined
          ? []
          : [{ type, payload: (review as { value: unknown }).value }];
      }),
    getHostCallCount: () => hostCallCount,
    getIsAuthenticated: () =>
      authStates.at(-1)?.tag === "Connected",
    getConnectionStatus: () => chainStatus,
    setPermissionBehavior: (behavior) => {
      const policy = normalizePermissionPolicy(behavior);
      devicePermissions = policy;
      remotePermissions = policy;
    },
    dispose() {
      this.reset();
      // Ending each stream rather than just dropping its registration: a
      // consumer parked on `next()` has to learn the host is gone, and a
      // registration dropped underneath it would leave it waiting forever.
      for (const close of [...subscriptionClosers]) close();
      for (const disconnect of [...chainDisconnectors]) disconnect();
    },
    cancelledNotifications: () => [...cancelledNotifications],
    getPermissionLog: () => [...permissionLog],
    getGrantedPermissions: () =>
      [...permissionDecisions.entries()]
        .filter(([, isGranted]) => isGranted)
        .map(([permission]) => permission)
        .sort(),
    grantPermission: (permission) => {
      permissionDecisions.set(permission, true);
      forgetStoredAuthorization(permission);
    },
    revokePermission: (permission) => {
      permissionDecisions.set(permission, false);
      forgetStoredAuthorization(permission);
    },
    resetPermission: (permission) => {
      permissionDecisions.delete(permission);
      forgetStoredAuthorization(permission);
    },
    setEnforcePermissions: (enforce) => {
      enforcePermissions = enforce;
    },
    getTheme: () => currentTheme,
    setTheme: (variant) => {
      currentTheme = variant;
      const item: HostThemeSubscribeItem = {
        name: { tag: "Default" },
        variant,
      };
      for (const push of themeSubscribers) push(item);
    },
    getChainStatus: () => chainStatus,
    simulateDisconnect: () => {
      chainStatus = "Disconnected";
      // Ending each live stream is what makes this a dropped transport rather
      // than a relabelled one: a product waiting on responses learns, and the
      // Rust mock does the same by dropping its disconnectors.
      for (const disconnect of [...chainDisconnectors]) disconnect();
    },
    simulateReconnect: () => {
      chainStatus = "Idle";
    },
    getOpenOperations: () => [...openOperations],
    getChatRooms: () => byKey(chatRooms),
    getChatBots: () => byKey(chatBots),
    getChatMessageLog: () => [...chatMessages],
    seedPreimage(value) {
      const key = preimageKey(value);
      preimages.set(hex(key), value);
      return key;
    },
    getProductStorage: () => {
      const prefix = productKey("");
      const entries: Record<string, Uint8Array> = {};
      for (const [key, value] of storage) {
        if (key.startsWith(prefix)) entries[key.slice(prefix.length)] = value;
      }
      return entries;
    },
    getProductStorageValue: (key: string) => {
      const prefix = productKey("");
      for (const [stored, value] of storage) {
        if (!stored.startsWith(prefix)) continue;
        const local = stored.slice(prefix.length);
        const namespaced = coreProductStorageKey(local);
        // Falls back to the whole key for a value written straight through the
        // host seam, which never passed through the core's namespacing.
        if ((namespaced ?? local) !== key) continue;
        try {
          return new TextDecoder("utf-8", { fatal: true }).decode(value);
        } catch {
          // A lenient decode answers replacement characters, which read as a
          // value the product wrote. An absence instead sends a suite to
          // `getProductStorage`, which hands back the bytes themselves.
          return undefined;
        }
      }
      return undefined;
    },
    getPreimages: () => [...preimages.values()],
    clearNavigationLog: () => {
      navigations.length = 0;
    },
    clearNotificationLog: () => {
      pushedNotifications.length = 0;
      cancelledNotifications.length = 0;
    },
    clearSigningLog: () => {
      reviews.length = 0;
    },
    clearPermissionLog: () => {
      permissionLog.length = 0;
    },
    clearPermissionDecisions: () => permissionDecisions.clear(),
    clearAuthStates: () => {
      authStates.length = 0;
    },
    clearSentRpc: () => {
      sentRpc.length = 0;
    },
    clearPreimages: () => preimages.clear(),
    clearStorage: () => storage.clear(),
    clearChatState: () => {
      chatRooms.clear();
      chatBots.clear();
      chatMessages.length = 0;
      // Live subscriptions stay open and see the emptied list, as on Rust.
      publishChatRooms();
    },
    reset() {
      this.clearNavigationLog();
      this.clearNotificationLog();
      this.clearSigningLog();
      this.clearPermissionLog();
      this.clearPermissionDecisions();
      this.clearAuthStates();
      this.clearSentRpc();
      this.clearPreimages();
      this.clearStorage();
      this.clearChatState();
      this.clearStatements();
      openOperations.length = 0;
      nextOperationId = 0;
      // Through the setter, so a subscribed product is told rather than left
      // believing the theme it last saw while `getTheme` reports the default.
      this.setTheme(theme);
      // The policies a `setPermissionBehavior` call replaced: leaving them in
      // place lets one case govern the next, which is what reset is for.
      devicePermissions = devicePermissionsInitial;
      remotePermissions = remotePermissionsInitial;
      chainStatus = "Idle";
      enforcePermissions = false;
      nextNotificationId = 1;
      nextChatMessageId = 0;
      hostCallCount = 0;
    },
    statements: notModeled("statements"),
    payment: notModeled("payment"),
    coinPayment: notModeled("coinPayment"),
  };
}

/**
 * Genesis hashes the mock host serves, one distinct non-zero value per chain.
 *
 * Distinct matters: chain routing is keyed on the genesis hash, so equal
 * hashes make the chains indistinguishable and a chain-routed call resolves to
 * whichever entry is found first. Non-zero matters for the same reason -- an
 * all-zero hash is also the natural placeholder a caller passes by accident.
 */
export const MOCK_GENESIS = {
  people:
    "0x1111111111111111111111111111111111111111111111111111111111111111",
  bulletin:
    "0x2222222222222222222222222222222222222222222222222222222222222222",
  assetHub:
    "0x3333333333333333333333333333333333333333333333333333333333333333",
} as const;

/**
 * A default {@link ProductRuntimeConfig} for a mock host. Override any field;
 * the genesis hashes and product id are placeholders suitable for tests.
 */
export function mockRuntimeConfig(
  overrides: Partial<ProductRuntimeConfig> = {},
): ProductRuntimeConfig {
  return {
    productId: "mock.dot",
    host: {
      name: "Mock Host",
      icon: "https://example.invalid/mock.png",
      version: "0.0.0",
    },
    platform: {
      type: "node",
      version: "0",
    },
    people: { genesisHash: MOCK_GENESIS.people },
    bulletin: { genesisHash: MOCK_GENESIS.bulletin },
    assetHub: { genesisHash: MOCK_GENESIS.assetHub },
    pairing: {
      deeplinkScheme: "polkadotapp",
    },
    // A signing host refuses to start without this; a pairing host ignores it.
    // Setting it unconditionally keeps one config usable for both roles.
    networkSuffix: "paseo",
    ...overrides,
  };
}
