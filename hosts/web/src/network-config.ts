/**
 * The networks this host can be configured for, and what each needs to reach
 * its chains and read DotNS names on it.
 *
 * A network is chosen once, before boot, and holds for the life of the page:
 * the host never switches network while it runs. The values are not invented
 * here, they come from the canonical sources:
 *
 * - the bundled `truapi-provider` catalog
 *   (`rust/crates/truapi-provider/src/networks.rs`) names each network and
 *   supplies its chains' genesis hashes and specs, so none are copied into
 *   this host;
 * - dotkit's `assets/envs.toml` holds the dotNS TLD, resolver contract, RPC
 *   endpoints and gateways, verified against the chain by its authors.
 *
 * `PreviewNet` is the product preview network. Its dotNS TLD is `.testnet`,
 * which is also the identity suffix the core derives product accounts under.
 */

import { DEFAULT_NETWORK as DEFAULT_NETWORK_ID } from "./network-scope.js";

/** Stable id of a network this host can be configured for. */
export type NetworkId = "paseo" | "previewnet";

/** Where this host reads a network's DotNS records and fetches product content. */
export interface DotnsEndpoints {
  /**
   * Asset Hub RPC as dotkit lists it (`ws`/`wss`). Reads go over the `https`
   * form of this URL.
   */
  assetHubRpc: string;
  /**
   * The `DotnsContentResolver` contract on Asset Hub, as a bare lower-case
   * H160 without `0x`. A wrong value reads as "name not found" rather than
   * failing loudly, so it must come from the canonical source.
   */
  contentResolver: string;
  /** Slot of the `contenthash` mapping in that contract. */
  contenthashSlot: number;
  /**
   * The Bulletin IPFS gateway base. The host appends the `/ipfs/<cid>/` path
   * form to it, so this is a gateway origin (with any deployment path), not a
   * content URL. On Paseo Next v2 that is dotkit's `ipfs_gateway`; on
   * PreviewNet it is the environment origin, whose content URL is
   * `<origin>/ipfs/<cid>`. The public shell gateway does not serve content.
   */
  contentGateway: string;
}

/** How this host reaches one network's chains and reads its DotNS names. */
export interface NetworkConfig {
  /** Stable id used to select the network. */
  id: NetworkId;
  /** Human name used in messages, e.g. `Paseo Next v2`. */
  displayName: string;
  /** The network's name in the bundled `truapi-provider` catalog. */
  catalogNetwork: string;
  /**
   * The network's dotNS TLD and the identity suffix the core derives product
   * accounts under, e.g. `paseo`, `testnet`. The two are one value: the core
   * lists the same strings in `DOTNS_TLDS`.
   */
  networkSuffix: string;
  /**
   * The public browser shell gateway for this network's TLD, e.g. `paseo.li`.
   * It serves the Polkadot browser shell and refuses framing, so it is never
   * opened as a product.
   */
  webGateway: string;
  /** Read-only DotNS and content endpoints on this network. */
  dotns: DotnsEndpoints;
}

/** Every network this host can be configured for, by id. */
export const NETWORKS: Record<NetworkId, NetworkConfig> = {
  paseo: {
    id: "paseo",
    displayName: "Paseo Next v2",
    catalogNetwork: "paseo-next-v2",
    networkSuffix: "paseo",
    webGateway: "paseo.li",
    dotns: {
      assetHubRpc: "wss://paseo-asset-hub-next-rpc.polkadot.io",
      contentResolver: "7f74d7cd50f5a834270e2ad395a01b01891ab37d",
      contenthashSlot: 0,
      contentGateway: "https://paseo-bulletin-next-ipfs.polkadot.io",
    },
  },
  previewnet: {
    id: "previewnet",
    displayName: "PreviewNet",
    catalogNetwork: "previewnet",
    networkSuffix: "testnet",
    webGateway: "testnet.li",
    dotns: {
      assetHubRpc: "wss://previewnet.substrate.dev/asset-hub",
      contentResolver: "7f74d7cd50f5a834270e2ad395a01b01891ab37d",
      contenthashSlot: 0,
      contentGateway: "https://previewnet.substrate.dev",
    },
  },
};

/**
 * The default network's config, for callers that have not chosen one.
 *
 * The default id itself is `DEFAULT_NETWORK` in `network-scope.ts`, shared
 * with the per-tab selection and the storage keys, so there is one answer to
 * which network a tab runs on.
 */
export const DEFAULT_NETWORK_CONFIG: NetworkConfig = networkConfig();

function isNetworkId(id: string): id is NetworkId {
  return Object.hasOwn(NETWORKS, id);
}

/**
 * The config for `id`, or the default when `id` is empty or absent. Throws for
 * an unknown id rather than silently serving the default, so a typo in the
 * selection is reported instead of connecting to the wrong network.
 */
export function networkConfig(id?: string | null): NetworkConfig {
  const trimmed = id?.trim() ?? "";
  const chosen = trimmed === "" ? DEFAULT_NETWORK_ID : trimmed;
  if (isNetworkId(chosen)) return NETWORKS[chosen];
  throw new Error(
    `Unknown network ${JSON.stringify(id)}. Known networks: ${Object.keys(NETWORKS).join(", ")}.`,
  );
}