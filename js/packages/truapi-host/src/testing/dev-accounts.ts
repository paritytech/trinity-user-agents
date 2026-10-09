// Named dev accounts for the test host.
//
// A TrUAPI signing host establishes a session from raw BIP-39 entropy, so a
// "dev account" here is just a fixed 32-byte value. The bytes below are
// arbitrary but stable, which is what makes a test's addresses reproducible
// across runs and machines.
//
// THESE ARE NOT POLKADOT-JS DEV ACCOUNTS. `//Alice` and friends are derived
// from a seed phrase through sr25519 soft/hard junctions; TrUAPI derives a root
// keypair from entropy and then a per-product subtree from that. The names are
// borrowed for familiarity, the addresses are not the same, and anything that
// asserts a literal `5Grw...` address from polkadot-js will not match. Fund or
// pin addresses by reading them back from the host, never by hard-coding a
// well-known one.

/** Name of a built-in dev account. */
export type DevAccountName = "alice" | "bob" | "charlie" | "dave";

/** A dev account: a name and the entropy its session is activated from. */
export interface DevAccount {
  /** The name a test refers to it by. */
  name: string;
  /** 32 bytes of BIP-39 entropy. */
  entropy: Uint8Array;
}

/** Fill 32 bytes with a marker, so each account is visibly distinct in a dump. */
function entropyFor(marker: number): Uint8Array {
  return new Uint8Array(32).fill(marker);
}

/**
 * The built-in dev accounts.
 *
 * Distinct markers rather than sequential values: a one-byte difference is easy
 * to miss when comparing two hex dumps in a failing test.
 */
export const DEV_ACCOUNTS: Record<DevAccountName, Uint8Array> = {
  alice: entropyFor(0xa1),
  bob: entropyFor(0xb2),
  charlie: entropyFor(0xc3),
  dave: entropyFor(0xd4),
};

/**
 * The built-in dev account names.
 *
 * Named as `@parity/host-api-test-sdk` names it, so a suite iterating the
 * roster reads the same export from either.
 */
export const DEV_ACCOUNT_NAMES = Object.keys(DEV_ACCOUNTS) as DevAccountName[];

/** Whether `name` is one of the built-in dev accounts. */
export function isDevAccountName(name: string): name is DevAccountName {
  return name in DEV_ACCOUNTS;
}

/**
 * Resolve an account spec to the entropy its session activates from.
 *
 * Accepts a built-in name or an explicit `{ name, entropy }`, so a suite that
 * needs a specific key is not forced to use one of the four.
 */
export function resolveAccount(spec: DevAccountName | DevAccount): DevAccount {
  if (typeof spec !== "string") {
    if (spec.entropy.length !== 32) {
      throw new Error(
        `dev account ${spec.name} needs 32 bytes of entropy, got ${spec.entropy.length}`,
      );
    }
    return spec;
  }
  const entropy = DEV_ACCOUNTS[spec];
  if (!entropy) {
    throw new Error(
      `unknown dev account "${spec}"; known: ${Object.keys(DEV_ACCOUNTS).join(", ")}`,
    );
  }
  return { name: spec, entropy };
}

/**
 * Real public networks a suite can proxy to.
 *
 * Genesis hashes are the chains' own, not {@link MOCK_GENESIS} placeholders:
 * the core asks for a chain by hash, so proxying only works when the hash the
 * runtime config carries is the real one.
 *
 * Using these makes a run non-hermetic. It inherits whatever the public chain
 * is doing -- accumulated state from other runs, contracts that were reaped,
 * endpoint outages -- which is the cost of testing against real inclusion.
 */
export const LIVE_CHAINS = {
  /** Paseo Asset Hub. */
  paseoAssetHub: {
    rpcUrl: "wss://paseo-asset-hub-next-rpc.polkadot.io",
    /**
     * Declare this in the host's runtime config when proxying to this chain.
     *
     * Not used for proxy routing -- an unhashed proxy takes every request, so
     * routing survives a reset. This value is needed because the *product*
     * checks it: `@parity/product-sdk-descriptors` refuses a host whose
     * declared genesis disagrees with the descriptor it was built against.
     *
     * It goes stale when the chain is reset, and it has more than once. When a
     * product reports a genesis mismatch, read the chain's current hash with
     * `chain_getBlockHash(0)` and re-pin it here.
     */
    genesisHash:
      "0x4349b00e54897e21196fd331015fc5be0f14e118beb0375ed2bb1793737bb57a",
  },
} as const;

/**
 * Paseo Asset Hub in `@parity/host-api-test-sdk`'s `NetworkConfig` shape.
 *
 * Named as `@parity/host-api-test-sdk` names it, so `networks: [PASEO_ASSET_HUB]`
 * means the same thing here. The genesis hash is the one the chain reports
 * today, NOT the
 * value that package ships -- its own constant went stale across a chain reset
 * and no longer matches this endpoint, so copying it would import a known-bad
 * value. Re-pin from `chain_getBlockHash(0)` after a reset.
 *
 * The other networks that package declares are deliberately not mirrored:
 * nothing here uses them and their genesis hashes have not been checked
 * against a live endpoint, so exporting them would ship unverified values.
 */
export const PASEO_ASSET_HUB = {
  id: "paseo-asset-hub",
  name: "Paseo Asset Hub",
  genesisHash: LIVE_CHAINS.paseoAssetHub.genesisHash,
  rpcUrl: LIVE_CHAINS.paseoAssetHub.rpcUrl,
  tokenSymbol: "PAS",
  tokenDecimals: 10,
} as const;

/** The chain a suite gets when it names none. */
export const DEFAULT_CHAIN = PASEO_ASSET_HUB;

/**
 * Fixture settings for proxying one real chain.
 *
 * A live chain has to agree in three places -- what the host proxies to, what
 * it reports serving, and what its runtime config declares -- and the product
 * checks the last two against its own descriptor bundle. Getting one of them
 * wrong fails as a genesis mismatch that names neither the setting nor the
 * file, so this builds all three from one value.
 *
 * ```ts
 * createTestHostFixture({
 *   productUrl,
 *   hostUrl: server.url,
 *   ...liveChain(LIVE_CHAINS.paseoAssetHub),
 * });
 * ```
 */
export function liveChain(chain: { rpcUrl: string; genesisHash: string }): {
  mock: {
    chainProxies: { rpcUrl: string }[];
    supportedChains: {
      network: string;
      chains: { identifier: "AssetHub"; genesisHash: `0x${string}` }[];
    };
  };
  runtimeConfig: Record<string, unknown>;
} {
  const genesisHash = chain.genesisHash as `0x${string}`;
  return {
    mock: {
      // No hash on the proxy: it takes every request, so routing survives a
      // reset even while the declared hash below has to be re-pinned.
      chainProxies: [{ rpcUrl: chain.rpcUrl }],
      supportedChains: {
        network: "paseo",
        chains: [{ identifier: "AssetHub", genesisHash }],
      },
    },
    runtimeConfig: { assetHub: { genesisHash } },
  };
}

/**
 * Check a product account's derivation index, refusing one `u32` would reshape.
 *
 * The index is SCALE-encoded as a `u32`, which wraps: `-1` encodes as
 * `4294967295` and `1.5` as `1`. Either names a different account than the
 * caller asked for, and a suite meets that as an address it funded which turns
 * out not to be the one its product signs with.
 */
export function checkDerivationIndex(index: number): number {
  if (!Number.isInteger(index) || index < 0 || index > 0xff_ff_ff_ff) {
    throw new RangeError(
      `product account index ${index} is not a u32: a derivation index is a whole number from 0 to 4294967295.`,
    );
  }
  return index;
}
