// Playwright fixture over the test host.
//
// The node half of the pair described in `host-page.ts`: every method here is
// a `page.evaluate` against `window.__TRUAPI_TEST_HOST__`, so the control
// surface a test drives is the same object the core is answering through.
//
// The method names are `@parity/host-api-test-sdk`'s `TestHost` names, so a
// suite written against either reads the same. Where a name is absent it is
// because TrUAPI has no seam for it -- see `notModeled` in
// `create-mock-host.ts`; those throw rather than silently pass.

import type { Page, FrameLocator } from "@playwright/test";

import {
  coreProductStorageKey,
  type ChainStatus,
  type ChatMessageRecord,
  type MockHostConfig,
  type NotificationLogEntry,
  type PermissionLogEntry,
  type PermissionPolicy,
  type PermissionPolicyAlias,
  type SigningLogEntry,
  type StatementEntry,
} from "../web/create-mock-host.js";
import type { StatementInput } from "../web/loopback-statements.js";

// The statement shapes `@parity/host-api-test-sdk` names, so a suite that
// annotates what it injects or reads back compiles unchanged.
export type { StatementEntry } from "../web/create-mock-host.js";
export type { StatementInput } from "../web/loopback-statements.js";

// The address a product account will be given, derivable without a host: a
// suite funding that account does it once, in setup, not per test.
export { productAccountAddress } from "./product-account.js";
export type { ProductAccountQuery } from "./product-account.js";

import { PRODUCT_FRAME_ID } from "./host-page.js";
import { createTestHostServer } from "./server.js";
import { hostPageUrl } from "./host-page-url.js";
import type { TestHostServer } from "./server.js";

// Re-exported so a test file imports the fixture and the chain constants from
// one path, the way `@parity/host-api-test-sdk/playwright` does.
export {
  DEFAULT_CHAIN,
  DEV_ACCOUNTS,
  LIVE_CHAINS,
  PASEO_ASSET_HUB,
  liveChain,
} from "./dev-accounts.js";
export { DEV_ACCOUNT_NAMES } from "./dev-accounts.js";
export type { DevAccount, DevAccountName } from "./dev-accounts.js";

// A suite that annotates a `genesisHash` reaches for this name from the same
// path `@parity/host-api-test-sdk/playwright` offers it on.
export type { HexString } from "@parity/truapi";

// Log entry types under the names `@parity/host-api-test-sdk` exports them by,
// so a suite that annotates a control-surface result compiles unchanged.
export type {
  ChatMessageRecord as ChatMessageLogEntry,
  NotificationLogEntry,
  PermissionLogEntry,
  PermissionPolicy as PermissionBehavior,
  SigningLogEntry,
} from "../web/create-mock-host.js";
export type { LoginBehavior } from "./host-page.js";

/** Selector for the product iframe the host page creates. */
const PRODUCT_FRAME = `#${PRODUCT_FRAME_ID}`;

/** Options for {@link createTestHostFixture}. */
export interface TestHostFixtureOptions {
  /** URL of the product under test. */
  productUrl: string;
  /**
   * Base URL of a running host page server.
   *
   * Optional. Omit it and the fixture starts one itself, lazily, and shares it
   * across every test in the file -- bundling is not cached, so one server per
   * test would pay esbuild each time. Supply it to control the lifetime.
   */
  hostUrl?: string;
  /**
   * Chains to serve, in `@parity/host-api-test-sdk`'s `NetworkConfig` shape.
   *
   * A convenience over spelling out `mock.chainProxies`, `mock.supportedChains`
   * and `runtimeConfig` separately, which have to agree. The chain's role is
   * read from the `id` suffix (`-asset-hub`, `-people`, `-bulletin`, else the
   * relay) and the rest of the id is the network name.
   *
   * Explicit `mock` or `runtimeConfig` values win, so a suite can start here
   * and override one piece.
   */
  networks?: NetworkConfig[];
  /**
   * A single chain, as older `@parity/host-api-test-sdk` versions spell it.
   *
   * Exactly `networks: [chain]`, under the name that vintage uses. Passing
   * both is refused rather than merged, because guessing which one the author
   * meant is worse than saying so.
   */
  chain?: NetworkConfig;
  /**
   * Accepted only to fail with an explanation. See the error text: TrUAPI
   * derives a product account from the session root, so it cannot be pinned.
   */
  productAccounts?: Record<string, string>;
  /**
   * dotNS identifier the host runs the product under.
   *
   * Must match the identifier the product signs with: the core rejects a
   * signing request whose account is scoped to a different product, so a
   * mismatch surfaces as `PermissionDenied` rather than as a config error.
   */
  productId?: string;
  /** Behaviour knobs forwarded to the mock host, including `chainProxies`. */
  mock?: MockHostConfig;
  /**
   * Overrides merged into the host's runtime config.
   *
   * Proxying a real chain needs this: the core asks for a chain by the genesis
   * hash the config declares, so that hash has to be the real one rather than
   * a {@link MOCK_GENESIS} placeholder.
   */
  runtimeConfig?: Record<string, unknown>;
  /**
   * Accounts the host can sign as. Defaults to `["alice"]`.
   *
   * Objects are accepted for `@parity/host-api-test-sdk` compatibility, but a
   * `uri` is rejected: a TrUAPI session activates from 32 bytes of entropy,
   * not a `//Alice`-style derivation path.
   */
  accounts?: (string | { name: string; uri?: string; entropy?: Uint8Array })[];
  /** Whether the host starts signed in. Defaults to `"auto"`. */
  loginBehavior?: "auto" | "manual";
  /**
   * Where the core runs. Defaults to `"worker"`, the production topology.
   *
   * Switch to `"main-thread"` to debug: the core's log output then reaches the
   * page console, where `page.on("console")` can read it. Under `"worker"` it
   * goes to the worker console, which Playwright does not observe -- so a
   * failing call looks like a bare outcome with no reason attached.
   */
  topology?: "worker" | "main-thread";
  /**
   * How resource allocation is answered. Defaults to `"granted"`.
   *
   * `"granted"` answers every request as allocated without performing it, so a
   * suite exercises its product's allowance-dependent paths with no on-chain
   * personhood identity. Nothing is allocated, so a pass says the product
   * handles a grant, not that a host would have given one.
   *
   * `"chain"` runs the real allocation against the chains the host serves:
   * ring membership, slot selection, proof and extrinsic. It fails where a
   * real host would, which is the point of choosing it.
   */
  allowances?: "granted" | "chain";
  /**
   * Decisions applied before the product loads.
   *
   * `resourceAllocation` names resources the host refuses; anything unlisted
   * stays granted. A boot option rather than a call, because a product asks for
   * its resources on connect and a later call would land after that.
   *
   * A refusal here is answered as refused whatever {@link allowances} would
   * otherwise say, which is what makes a product's refusal path reachable: with
   * allocation granted there is nothing a suite could do to see one.
   */
  behaviors?: { resourceAllocation?: Record<string, boolean> };
  /**
   * Serve the statement store in-page instead of forwarding it to the chains
   * the host proxies. Defaults to on when `allowances` is `"granted"`, so the
   * two halves of a statement flow agree: a product that is handed an
   * unregistered allowance key can still submit with it.
   *
   * The store rides on the People chain's proxy, or on the single proxy of a
   * one-chain suite. Several chains with no People among them carry no store:
   * left to the default the fixture serves none, and `true` is refused, because
   * attaching it to a hub would answer reads a real hub refuses.
   *
   * Nothing submitted this way leaves the page, and a real store would refuse
   * it. Set `false` to send statements to the chain and see what it says.
   */
  loopbackStatements?: boolean;
  /**
   * Core log level (`off`/`error`/`warn`/`info`/`debug`/`trace`).
   *
   * The core logs why a call failed before mapping it to a protocol answer,
   * so this is what turns an opaque outcome into its cause. Pair it with
   * `topology: "main-thread"` to read those lines from a test.
   */
  logLevel?: string;
  /** How long to wait for the host page to publish its control surface. */
  readyTimeoutMs?: number;
}

/** The fixture a test receives. */
export interface TestHost {
  /** The Playwright page running the host. */
  page: Page;
  /** Locator for the embedded product. */
  productFrame(): FrameLocator;

  getNavigationLog(): Promise<string[]>;
  clearNavigationLog(): Promise<void>;
  getNotificationLog(): Promise<NotificationLogEntry[]>;
  clearNotificationLog(): Promise<void>;
  getSigningLog(): Promise<SigningLogEntry[]>;
  clearSigningLog(): Promise<void>;
  getPermissionLog(): Promise<PermissionLogEntry[]>;
  clearPermissionLog(): Promise<void>;
  getGrantedPermissions(): Promise<string[]>;
  grantPermission(permission: string): Promise<void>;
  revokePermission(permission: string): Promise<void>;
  setEnforcePermissions(enforce: boolean): Promise<void>;
  setPermissionBehavior(
    behavior: PermissionPolicy | PermissionPolicyAlias,
  ): Promise<void>;
  getChatRooms(): Promise<unknown[]>;
  getChatBots(): Promise<unknown[]>;
  getChatMessageLog(): Promise<ChatMessageRecord[]>;
  clearChatState(): Promise<void>;
  /** Product-scoped storage the core has written, keyed without the prefix. */
  getProductStorage(): Promise<Record<string, Uint8Array>>;
  getPreimages(): Promise<Uint8Array[]>;
  seedPreimage(value: Uint8Array): Promise<Uint8Array>;
  /**
   * Find the value the product stored under `key`.
   *
   * The core namespaces product storage keys before the host ever sees them,
   * so this reads the product's own key back out of that shape rather than
   * matching the internal string. Answers the bytes, where
   * {@link TestHost.getProductStorageValue} answers them decoded as UTF-8.
   */
  findProductStorage(key: string): Promise<Uint8Array | undefined>;
  /**
   * The SS58 address of a product account, at the prefix the core mandates.
   *
   * A product account is derived from the active session's root, so this
   * answers `undefined` while the host is signed out and a different address
   * after `switchAccount`. Read it here rather than out of the product's own
   * UI: a suite funding that account, or asserting on it, should not depend on
   * the product rendering it.
   *
   * Encoded at the prefix the core mandates, which is not necessarily the one
   * the product displays: a product rendering at another prefix shows a
   * different string for the same account. Compare against a product's own
   * rendering by decoding both, and pass this to a faucet or a transfer as it
   * stands.
   */
  getProductAccountAddress(
    productId?: string,
    index?: number,
  ): Promise<string | undefined>;
  /**
   * The value the product stored under `key`, decoded as UTF-8.
   *
   * `@parity/host-api-test-sdk` spells this `getProductStorageValue` and
   * returns a string, so a migrating suite's storage assertions compile
   * unchanged. Use {@link findProductStorage} for a value that is not text.
   */
  getProductStorageValue(key: string): Promise<string | undefined>;
  clearPreimages(): Promise<void>;
  getTheme(): Promise<string>;
  setTheme(variant: string): Promise<void>;
  getIsAuthenticated(): Promise<boolean>;
  getChainStatus(): Promise<ChainStatus>;
  getConnectionStatus(): Promise<ChainStatus>;
  /**
   * Raw JSON-RPC the core sent over the chain connection, in order.
   *
   * A chain-path failure is otherwise invisible from a suite: the product
   * shows a stalled UI and the fixture reports nothing about what the host
   * asked the chain. This is what tells you whether a request was made at
   * all, and what came back after it.
   */
  getSentRpc(): Promise<string[]>;
  /** Drop the recorded RPC, so one case does not read another's traffic. */
  clearSentRpc(): Promise<void>;
  simulateDisconnect(): Promise<void>;
  simulateReconnect(): Promise<void>;
  /**
   * Wait until the product has an open channel to the host.
   *
   * Resolves once the core has answered at least one product call, which is
   * the first observable evidence that frames are crossing the wire in both
   * directions.
   */
  waitForConnection(timeoutMs?: number): Promise<void>;
  /** Names the host can currently sign as. */
  getAccounts(): Promise<string[]>;
  /** The account the current session is activated from, if any. */
  getActiveAccount(): Promise<string | undefined>;
  /** Re-activate the session as `name`. */
  switchAccount(name: string): Promise<void>;
  /** Replace the roster, activating the first entry. */
  setAccounts(names: string[]): Promise<void>;
  /** Drop the session, leaving the host signed out. */
  signOut(): Promise<void>;
  /** Return the host to its constructed state between cases. */
  reset(): Promise<void>;

  /**
   * Statements the product submitted.
   *
   * Always throws. The statement store is core-owned and submission is
   * rejected inside the core before any RPC is emitted, so there is nothing
   * for the host to record -- not a host seam the mock declined to implement.
   */
  /**
   * Statements the product submitted, decoded, read off the chain transport
   * rather than a host-side log.
   *
   * Empty for a product that asks the host to sign
   * (`createProofAuthorized`): that needs a statement allowance, and without
   * one no statement is ever built to submit. A product that signs its own
   * statements is observable here.
   */
  getSubmittedStatements(): Promise<StatementEntry[]>;
  /** Every statement the store holds, submitted or injected, in order. */
  getStatements(): Promise<StatementEntry[]>;
  /**
   * Deliver a statement to the product as a chain notification, answering the
   * entry the store retained.
   *
   * Takes the topics and payload as a structure, or the SCALE wire bytes the
   * chain would have sent. Retained either way, so a suite that injects before
   * its product subscribes has the statement replayed to it on subscribe
   * rather than losing it.
   */
  injectStatement(
    statement: StatementInput | Uint8Array | string,
  ): Promise<StatementEntry>;
  /** Statements injected so far, in order. */
  getInjectedStatements(): Promise<string[]>;
  /** Forget the injected statements. */
  clearStatements(): Promise<void>;

  /**
   * Release the host.
   *
   * A no-op. Playwright owns the page's lifetime and closes it when the test
   * ends, so a suite has nothing to release. Declared because
   * `@parity/host-api-test-sdk`'s surface has it, so a teardown call written
   * against that surface is still valid here.
   */
  dispose(): Promise<void>;

  /** Set the spendable balance. Always throws; payments are unimplemented. */
  setPaymentBalance(amount: bigint): Promise<never>;
  /** Payment operations the product performed. Always throws; see above. */
  getPaymentLog(): Promise<never>;
  /** Drop the payment log. Always throws; see above. */
  clearPaymentLog(): Promise<never>;
  /** How top-ups resolve. Always throws; see above. */
  setPaymentTopUpBehavior(behavior: unknown): Promise<never>;
  /** Force a payment's status. Always throws; see above. */
  simulatePaymentStatus(
    paymentId: string,
    status: { tag: string; value?: string },
  ): Promise<never>;

  /**
   * Deliver a peer's activation of a chat action to the product.
   *
   * Always throws. `ChatPlatform` is create/register/post/subscribe-rooms only,
   * so a peer activating an action has no way into the core. `ChatAction`
   * exists as message *content* a product posts, not as an inbound event.
   */
  /**
   * Deliver a host-authored Chat action to the product -- the path a posted
   * message or a tapped `Actions` button takes back to it.
   *
   * Takes the `HostChatActionSubscribeItem` the core publishes, not
   * `@parity/host-api-test-sdk`'s `{roomId, peer, payload}`: this goes through
   * the core's own action stream, so it carries the core's value.
   */
  injectChatAction(action: unknown): Promise<void>;

  /**
   * Change the login behaviour after boot.
   *
   * Always throws, and says what to do instead: the host page reads it once at
   * start, so it is a `loginBehavior` option on `createTestHostFixture`.
   */
  setLoginBehavior(behavior: "auto" | "manual"): Promise<never>;
}

/**
 * Why the payment controls cannot be served.
 *
 * Not a seam the mock declined to implement: every method in the core's
 * `capabilities/payment.rs` returns an error and ignores its arguments, so
 * there is no behaviour for a host to model or a test to observe.
 */
const NO_PAYMENT_SEAM =
  "is not available in the TrUAPI test host: the protocol declares payments " +
  "but no host implements them -- every method in the core's payment " +
  "capability returns an error and ignores its arguments, so there is nothing " +
  "to record or simulate. See docs/rfcs/0006-payments.md.";

/** Why login behaviour is fixed once the host page has booted. */
const LOGIN_BEHAVIOR_IS_CONSTRUCTION_TIME =
  "cannot be changed after boot in the TrUAPI test host: the host page reads " +
  "it once at start. Pass `loginBehavior: \"auto\" | \"manual\"` to " +
  "createTestHostFixture instead.";

/** One chain, in `@parity/host-api-test-sdk`'s shape. */
export interface NetworkConfig {
  /** e.g. `paseo-asset-hub`; the suffix names the chain's role. */
  id: string;
  name?: string;
  genesisHash: string;
  rpcUrl: string;
  tokenSymbol?: string;
  tokenDecimals?: number;
}

/** Why a product account cannot be pinned to a chosen key. */
const NO_PINNED_PRODUCT_ACCOUNT =
  "is not supported by the TrUAPI test host: a product account is DERIVED " +
  "from (session root, product id), so it cannot be mapped to a chosen dev " +
  "account. `@parity/host-api-test-sdk` could pin one because it reimplements " +
  "the protocol with no core behind it. Read the address back from the host " +
  "instead of pinning it, and expect `switchAccount` to change it: the next " +
  "call derives from the new session root, with no reconnect needed. That is " +
  "the real behaviour, not a test-host limitation.";

/** Why a derivation URI cannot name an account. */
const NO_DERIVATION_URI =
  "is not supported by the TrUAPI test host: a session activates from 32 " +
  "bytes of BIP-39 entropy, not a `//Alice`-style derivation path, so the " +
  "addresses differ from polkadot-js's by construction. Use a built-in name " +
  "(alice, bob, charlie, dave) or pass explicit entropy.";

/** Role and network implied by a `NetworkConfig.id`. */
function splitChainId(id: string): {
  network: string;
  identifier: "AssetHub" | "People" | "Bulletin" | "Relay";
  configKey?: "assetHub" | "people" | "bulletin";
} {
  const suffixes = [
    ["-asset-hub", "AssetHub", "assetHub"],
    ["-people", "People", "people"],
    ["-bulletin", "Bulletin", "bulletin"],
  ] as const;
  for (const [suffix, identifier, configKey] of suffixes) {
    if (id.endsWith(suffix)) {
      return { network: id.slice(0, -suffix.length), identifier, configKey };
    }
  }
  // No suffix: the id names the network and the chain is its relay. The
  // runtime config has no relay slot, so there is no key to declare it under.
  return { network: id, identifier: "Relay" };
}

/**
 * The product-storage entry `key` names, or `undefined` when nothing holds it.
 *
 * Reads the product's own key out of each stored one with the parse
 * `getProductStorageValue` uses, so the byte reader and the string reader
 * cannot disagree about which entry a key names. Falls back to the whole key
 * for a value written straight through the host seam, which never passed
 * through the core's namespacing.
 */
export function productStorageEntry(
  stored: Record<string, Uint8Array>,
  key: string,
): Uint8Array | undefined {
  const match = Object.entries(stored).find(
    ([entry]) => (coreProductStorageKey(entry) ?? entry) === key,
  );
  return match?.[1];
}

/**
 * Whether the loopback statement store is served, and on whose say-so.
 *
 * `"default"` is the store turned on because allocation is granted rather than
 * because a suite named it. The distinction decides what happens when no
 * declared proxy can carry the store: a suite that asked for it is told, a
 * suite that never mentioned it gets no store and builds.
 */
export type LoopbackStatements = boolean | "default";

/**
 * Expand `networks` into the three settings that have to agree.
 *
 * A single proxy carries no genesis hash: an unhashed proxy takes every
 * request, so routing survives a chain reset while only the DECLARED hash --
 * which the product checks against its descriptor bundle -- needs re-pinning.
 *
 * Several proxies have to be hashed, because an unhashed one would answer the
 * other chain's requests too. That is not hypothetical: with a hub and a People
 * chain both unhashed, the hub took the People reads and allowance registration
 * failed looking for personhood collections on a chain that has none.
 */
export function fromNetworks(
  networks: NetworkConfig[],
  loopbackStatements: LoopbackStatements = false,
): {
  mock: Pick<MockHostConfig, "chainProxies" | "supportedChains">;
  runtimeConfig: Record<string, unknown>;
} {
  const runtimeConfig: Record<string, unknown> = {};
  const chains: { identifier: string; genesisHash: string }[] = [];
  let network = "paseo";
  for (const entry of networks) {
    const split = splitChainId(entry.id);
    network = split.network || network;
    chains.push({
      identifier: split.identifier,
      genesisHash: entry.genesisHash,
    });
    if (split.configKey) {
      runtimeConfig[split.configKey] = { genesisHash: entry.genesisHash };
    }
  }
  // Only the People chain carries the statement store, so serving it locally on
  // the hub as well would claim a store where none exists. A suite that declares
  // no People chain has no such proxy to carry it, and a single unhashed proxy
  // takes every request -- so that one serves the store instead. With several
  // chains and no People among them there is no request this could attach to,
  // and attaching it to a hub would answer statement reads a real hub refuses.
  const peopleChains = networks.filter(
    (entry) => splitChainId(entry.id).identifier === "People",
  );
  const carried = peopleChains.length > 0 || networks.length === 1;
  // Loud only for the suite that named the store: it asked for something these
  // networks cannot give. The default is on for every granted-allocation suite,
  // including the many that never submit a statement, so there it serves no
  // store rather than refusing to build a fixture over an unrelated option.
  if (loopbackStatements === true && !carried) {
    throw new Error(
      "testHost `loopbackStatements` needs a People chain in `networks`, or a " +
        "single chain whose proxy takes every request. Several chains are " +
        "declared and none is a People chain, so there is no proxy the " +
        "statement store belongs on: the store would answer reads that the " +
        "declared chains refuse. Add the People chain, or drop to one chain.",
    );
  }
  const servesStatements = (entry: NetworkConfig): boolean =>
    loopbackStatements !== false &&
    carried &&
    (peopleChains.length > 0
      ? splitChainId(entry.id).identifier === "People"
      : true);

  return {
    mock: {
      chainProxies: networks.map((entry) => ({
        ...(networks.length > 1 ? { genesisHash: entry.genesisHash } : {}),
        rpcUrl: entry.rpcUrl,
        ...(servesStatements(entry) ? { loopbackStatements: true } : {}),
      })),
      supportedChains: { network, chains },
    } as Pick<MockHostConfig, "chainProxies" | "supportedChains">,
    runtimeConfig,
  };
}

/** Account names for the page URL, rejecting anything the host cannot honour. */
function accountNames(
  accounts: (string | { name: string; uri?: string; entropy?: Uint8Array })[],
): (string | { name: string; entropy: Uint8Array })[] {
  return accounts.map((account) => {
    if (typeof account === "string") return account;
    if (account.uri !== undefined) {
      throw new Error(
        `testHost account "${account.name}": \`uri\` ${NO_DERIVATION_URI}`,
      );
    }
    // Entropy is carried through rather than reduced to a name: it is the only
    // way to sign as an identity that is not one of the built-ins.
    if (account.entropy !== undefined) {
      if (account.entropy.length !== 32) {
        throw new Error(
          `testHost account "${account.name}": entropy must be 32 bytes, got ${account.entropy.length}`,
        );
      }
      return { name: account.name, entropy: account.entropy };
    }
    return account.name;
  });
}

/**
 * Build the `testHost` fixture.
 *
 * ```ts
 * export const test = base.extend(createTestHostFixture({
 *   productUrl: "http://127.0.0.1:5173",
 *   hostUrl: server.url,
 * }));
 * ```
 */
export function createTestHostFixture(defaults: TestHostFixtureOptions) {
  if (defaults.productAccounts) {
    throw new Error(`testHost \`productAccounts\` ${NO_PINNED_PRODUCT_ACCOUNT}`);
  }
  // Started at most once and shared by every test in the file. Held as the
  // promise, not the server, so concurrent first tests await one start rather
  // than racing two.
  // Validated here rather than in the fixture body: a bad option should fail
  // when the suite is constructed, not inside the first test that runs.
  const accounts = defaults.accounts
    ? accountNames(defaults.accounts)
    : undefined;
  let ownServer: Promise<TestHostServer> | undefined;
  const hostBase = async (): Promise<string> => {
    if (defaults.hostUrl) return defaults.hostUrl;
    ownServer ??= createTestHostServer({ unref: true });
    return (await ownServer).url;
  };

  if (defaults.chain && defaults.networks) {
    throw new Error(
      "testHost fixture: pass either `chain` (one chain, the older " +
        "`@parity/host-api-test-sdk` spelling) or `networks` (several), not " +
        "both -- `chain: x` is exactly `networks: [x]`.",
    );
  }
  const chains = defaults.networks ?? (defaults.chain ? [defaults.chain] : undefined);
  // Defaults to on when allocation is granted unchecked, so a product handed
  // an unregistered allowance key has somewhere its statements are accepted.
  // Carried as "default" rather than `true` so the networks the suite declared
  // decide the rest: a derived store steps aside where none can be served, a
  // named one says so.
  const loopbackStatements: LoopbackStatements =
    defaults.loopbackStatements ??
    ((defaults.allowances ?? "granted") === "granted" ? "default" : false);
  // Only the refused entries travel: `true` is what every unlisted resource
  // already is, so carrying it would say something the host does not act on.
  const withheldResources = Object.entries(
    defaults.behaviors?.resourceAllocation ?? {},
  )
    .filter(([, allowed]) => !allowed)
    .map(([resource]) => resource);
  const expanded = chains
    ? fromNetworks(chains, loopbackStatements)
    : undefined;
  // Explicit settings win over anything derived from `networks`.
  const mock = expanded ? { ...expanded.mock, ...defaults.mock } : defaults.mock;
  const runtimeConfig = expanded
    ? { ...expanded.runtimeConfig, ...defaults.runtimeConfig }
    : defaults.runtimeConfig;

  return {
    testHost: async (
      { page }: { page: Page },
      use: (fixture: TestHost) => Promise<void>,
    ) => {
      const url = hostPageUrl(await hostBase(), {
        productUrl: defaults.productUrl,
        productId: defaults.productId,
        mock,
        runtimeConfig,
        accounts,
        loginBehavior: defaults.loginBehavior,
        topology: defaults.topology,
        allowances: defaults.allowances,
        withheldResources,
        logLevel: defaults.logLevel,
      });
      await page.goto(url);

      // The page publishes its control surface only once the WASM core is up
      // and the product has its port, so this doubles as the wire's ready gate.
      await page.waitForFunction(() => !!window.__TRUAPI_TEST_HOST__, {
        timeout: defaults.readyTimeoutMs ?? 30_000,
      });

      /** Call one control method in the page and return its result. */
      const call = <T>(method: string, ...args: unknown[]): Promise<T> =>
        page.evaluate(
          ([name, callArgs]: [string, unknown[]]) => {
            const host = window.__TRUAPI_TEST_HOST__;
            if (!host) throw new Error("test host is not running on this page");
            const fn = (host as unknown as Record<string, unknown>)[name];
            if (typeof fn !== "function") {
              throw new Error(`test host has no control method ${name}`);
            }
            return (fn as (...a: unknown[]) => unknown).apply(host, callArgs);
          },
          [method, args] as [string, unknown[]],
        ) as Promise<T>;

      const testHost: TestHost = {
        page,
        productFrame: () => page.frameLocator(PRODUCT_FRAME),

        getNavigationLog: () => call("getNavigationLog"),
        clearNavigationLog: () => call("clearNavigationLog"),
        getNotificationLog: () => call("getNotificationLog"),
        clearNotificationLog: () => call("clearNotificationLog"),
        getSigningLog: () => call("getSigningLog"),
        clearSigningLog: () => call("clearSigningLog"),
        getPermissionLog: () => call("getPermissionLog"),
        clearPermissionLog: () => call("clearPermissionLog"),
        getGrantedPermissions: () => call("getGrantedPermissions"),
        grantPermission: (permission) => call("grantPermission", permission),
        revokePermission: (permission) => call("revokePermission", permission),
        setEnforcePermissions: (enforce) =>
          call("setEnforcePermissions", enforce),
        setPermissionBehavior: (behavior) =>
          call("setPermissionBehavior", behavior),
        getChatRooms: () => call("getChatRooms"),
        getChatBots: () => call("getChatBots"),
        getChatMessageLog: () => call("getChatMessageLog"),
        clearChatState: () => call("clearChatState"),
        // `page.evaluate` serialises a Uint8Array as a plain index object, so
        // binary values are converted to arrays in the page and rebuilt here.
        // Without this a caller gets `{0: 114, …}` and any decode of it
        // silently yields "".
        getProductStorage: async () => {
          const raw = await page.evaluate(() => {
            const host = window.__TRUAPI_TEST_HOST__;
            if (!host) throw new Error("test host is not running on this page");
            return Object.fromEntries(
              Object.entries(host.getProductStorage()).map(([key, value]) => [
                key,
                Array.from(value),
              ]),
            );
          });
          return Object.fromEntries(
            Object.entries(raw).map(([key, value]) => [
              key,
              Uint8Array.from(value),
            ]),
          );
        },
        getProductAccountAddress: (productId?: string, index?: number) =>
          call("getProductAccountAddress", productId, index),
        findProductStorage: async (key: string) =>
          productStorageEntry(await testHost.getProductStorage(), key),
        getProductStorageValue: async (key: string) =>
          page.evaluate((storageKey) => {
            const host = window.__TRUAPI_TEST_HOST__;
            if (!host) throw new Error("test host is not running on this page");
            return host.getProductStorageValue(storageKey);
          }, key),
        getPreimages: async () => {
          const raw = await page.evaluate(() => {
            const host = window.__TRUAPI_TEST_HOST__;
            if (!host) throw new Error("test host is not running on this page");
            return host.getPreimages().map((value) => Array.from(value));
          });
          return raw.map((value) => Uint8Array.from(value));
        },
        seedPreimage: (value) => call("seedPreimage", value),
        clearPreimages: () => call("clearPreimages"),
        getTheme: () => call("getTheme"),
        setTheme: (variant) => call("setTheme", variant),
        getIsAuthenticated: () => call("getIsAuthenticated"),
        getChainStatus: () => call("getChainStatus"),
        getConnectionStatus: () => call("getConnectionStatus"),
        getSentRpc: () => call("sentRpc"),
        clearSentRpc: () => call("clearSentRpc"),
        simulateDisconnect: () => call("simulateDisconnect"),
        simulateReconnect: () => call("simulateReconnect"),
        async waitForConnection(timeoutMs = 30_000) {
          // A live wire means the core has actually called the host, not
          // merely that the page finished loading.
          await page.waitForFunction(
            () => (window.__TRUAPI_TEST_HOST__?.getHostCallCount() ?? 0) > 0,
            { timeout: timeoutMs },
          );
        },
        getAccounts: () => call("getAccounts"),
        getActiveAccount: () => call("getActiveAccount"),
        // Switching account re-activates the session, which reloads the
        // product iframe; wait for it so the next action does not race it.
        switchAccount: async (name) => {
          await call("switchAccount", name);
          await page
            .frameLocator(PRODUCT_FRAME)
            .locator("body")
            .waitFor({ state: "attached" });
        },
        setAccounts: async (names) => {
          await call("setAccounts", names);
          await page
            .frameLocator(PRODUCT_FRAME)
            .locator("body")
            .waitFor({ state: "attached" });
        },
        signOut: () => call("signOut"),
        reset: () => call("reset"),

        // Sent as hex, not bytes: `page.evaluate` serialises a Uint8Array as a
        // plain index object, which would inject a statement of nothing.
        injectStatement: (statement) =>
          page.evaluate(
            (value) => {
              const host = window.__TRUAPI_TEST_HOST__;
              if (!host) throw new Error("test host is not running on this page");
              return host.injectStatement(value);
            },
            // A `Uint8Array` is reduced to hex here because `page.evaluate`
            // serialises it as a plain index object, which would inject a
            // statement of nothing. The decoded shape survives as it is.
            statement instanceof Uint8Array
              ? `0x${Array.from(statement, (byte) => byte.toString(16).padStart(2, "0")).join("")}`
              : statement,
          ),
        getInjectedStatements: () => call("getInjectedStatements"),
        clearStatements: () => call("clearStatements"),

        getSubmittedStatements: () => call("getSubmittedStatements"),
        getStatements: () => call("getStatements"),

        // Playwright closes the page after the fixture yields, so there is
        // genuinely nothing to do -- not a silent stub standing in for work.
        dispose: () => Promise.resolve(),

        setPaymentBalance: () => {
          throw new Error(`testHost.setPaymentBalance ${NO_PAYMENT_SEAM}`);
        },
        getPaymentLog: () => {
          throw new Error(`testHost.getPaymentLog ${NO_PAYMENT_SEAM}`);
        },
        clearPaymentLog: () => {
          throw new Error(`testHost.clearPaymentLog ${NO_PAYMENT_SEAM}`);
        },
        setPaymentTopUpBehavior: () => {
          throw new Error(`testHost.setPaymentTopUpBehavior ${NO_PAYMENT_SEAM}`);
        },
        simulatePaymentStatus: () => {
          throw new Error(`testHost.simulatePaymentStatus ${NO_PAYMENT_SEAM}`);
        },
        injectChatAction: (action) =>
          page.evaluate((value) => {
            const host = window.__TRUAPI_TEST_HOST__;
            if (!host) throw new Error("test host is not running on this page");
            return host.injectChatAction(value);
          }, action as never),
        setLoginBehavior: () => {
          throw new Error(
            `testHost.setLoginBehavior ${LOGIN_BEHAVIOR_IS_CONSTRUCTION_TIME}`,
          );
        },
      };

      await use(testHost);
    },
  };
}
