// Chains a test host serves, in `@parity/host-api-test-sdk`'s `networks`
// shape, and their expansion into the host settings that have to agree. Shared
// by the Playwright fixture and the node server, so a suite gets the same
// chains whichever one it configures.

import type { MockHostConfig } from "../web/create-mock-host.js";

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

/** The chain options a fixture or server accepts, plus explicit overrides. */
export interface NetworkOptions {
  networks?: NetworkConfig[];
  chain?: NetworkConfig;
  mock?: MockHostConfig;
  runtimeConfig?: Record<string, unknown>;
}

/**
 * The mock and runtime config `options` stand for: `networks` (or the single
 * `chain`) expanded, with explicit `mock` and `runtimeConfig` values winning
 * over anything derived from them.
 */
export function withNetworks(
  options: NetworkOptions,
  loopbackStatements: LoopbackStatements = false,
): { mock?: MockHostConfig; runtimeConfig?: Record<string, unknown> } {
  if (options.chain && options.networks) {
    throw new Error(
      "testHost: pass either `chain` (one chain, the older " +
        "`@parity/host-api-test-sdk` spelling) or `networks` (several), not " +
        "both -- `chain: x` is exactly `networks: [x]`.",
    );
  }
  const chains =
    options.networks ?? (options.chain ? [options.chain] : undefined);
  if (!chains) {
    return { mock: options.mock, runtimeConfig: options.runtimeConfig };
  }
  const expanded = fromNetworks(chains, loopbackStatements);
  return {
    mock: { ...expanded.mock, ...options.mock },
    runtimeConfig: { ...expanded.runtimeConfig, ...options.runtimeConfig },
  };
}
