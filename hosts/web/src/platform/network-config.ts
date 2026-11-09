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
 * - dotkit's `assets/envs.toml` holds the dotNS TLD, resolver contract and
 *   gateways, verified against the chain by its authors;
 * - `paritytech/triangle-status` `environments.json` names each
 *   environment's identity backend (`backend`) and its signed Trinity stack
 *   descriptor, whose `chains[].rpc` lists each chain's RPC nodes (read
 *   2026-11-04).
 *
 * `PreviewNet` is the product preview network. Its dotNS TLD is `.testnet`,
 * which is also the identity suffix the core derives product accounts under.
 */

import { DEFAULT_NETWORK as DEFAULT_NETWORK_ID } from "./network-scope.js";

/** Stable id of a network this host can be configured for. */
export type NetworkId = "paseo" | "previewnet";

/** A chain of a network that this host connects to. */
export type ChainRole = "relay" | "assetHub" | "people" | "bulletin";

/** Every chain a network has, in the order the host connects them. */
export const CHAIN_ROLES: readonly ChainRole[] = [
  "relay",
  "assetHub",
  "people",
  "bulletin",
];

/**
 * How the host reaches one chain: through the chain's RPC node, which it
 * trusts, or through the embedded light client, which checks what it reads.
 */
export type ChainBackend = "rpc" | "light-client";

/** One chain's RPC node and the backend the host uses for it unless told otherwise. */
export interface ChainAccess {
  /** The chain's RPC node (`wss`), from the network's Trinity stack descriptor. */
  rpc: string;
  backend: ChainBackend;
}

/**
 * The backend each chain of `config` is reached through: its default, unless
 * `overrides` names another.
 */
export function chainBackends(
  config: NetworkConfig,
  overrides: Partial<Record<ChainRole, ChainBackend>> = {},
): Record<ChainRole, ChainBackend> {
  return Object.fromEntries(
    CHAIN_ROLES.map((role) => [
      role,
      overrides[role] ?? config.chains[role].backend,
    ]),
  ) as Record<ChainRole, ChainBackend>;
}

/** Where this host reads a network's DotNS records and fetches product content. */
export interface DotnsEndpoints {
  /**
   * The `DotnsContentResolver` contract on Asset Hub, as a bare lower-case
   * H160 without `0x`. A wrong value reads as "name not found" rather than
   * failing loudly, so it must come from the canonical source.
   */
  contentResolver: string;
  /** Slot of the `contenthash` mapping in that contract. */
  contenthashSlot: number;
  /**
   * Slot of the `text` mapping in that contract, `node => key => value`,
   * which holds a name's text records such as `executable`.
   */
  textSlot: number;
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
  /**
   * Each chain's RPC node and default backend. DotNS reads always go to the
   * Asset Hub node, over the `https` form of its URL.
   */
  chains: Record<ChainRole, ChainAccess>;
  /** Read-only DotNS and content endpoints on this network. */
  dotns: DotnsEndpoints;
  /**
   * The identity backend (DUB) origin, which issues TURN relay credentials
   * for a personhood proof. Null where the environment names none.
   */
  identityBackend: string | null;
  /**
   * The People chain whose Statement Store accounts may post once their
   * claim's block has a child at the best block, rather than once their
   * allowance holds at the finalized head (`allowance-usable.ts`). Null: they
   * wait for the finalized head.
   *
   * Set only for a People chain whose RPC nodes were checked to keep a
   * statement whose allowance is not final yet (polkadot-sdk #13437). The
   * genesis only permits a candidate. Each best-block read must also verify
   * the actual RPC connection's genesis, version and required methods.
   */
  bestBlockPosting: { peopleGenesis: string } | null;
}

/** Every network this host can be configured for, by id. */
export const NETWORKS: Record<NetworkId, NetworkConfig> = {
  paseo: {
    id: "paseo",
    displayName: "Paseo Next v2",
    catalogNetwork: "paseo-next-v2",
    networkSuffix: "paseo",
    webGateway: "paseo.li",
    chains: {
      relay: { rpc: "wss://paseo-rpc.n.dwellir.com", backend: "light-client" },
      assetHub: {
        rpc: "wss://paseo-asset-hub-next-rpc.polkadot.io",
        backend: "rpc",
      },
      people: {
        rpc: "wss://paseo-people-next-system-rpc.polkadot.io",
        backend: "rpc",
      },
      bulletin: {
        rpc: "wss://paseo-bulletin-next-rpc.polkadot.io",
        backend: "rpc",
      },
    },
    dotns: {
      contentResolver: "7f74d7cd50f5a834270e2ad395a01b01891ab37d",
      contenthashSlot: 0,
      textSlot: 1,
      contentGateway: "https://paseo-bulletin-next-ipfs.polkadot.io",
    },
    identityBackend: "https://identity.dotspark.app",
    // This genesis is a candidate; the connected node must pass the RPC identity check.
    bestBlockPosting: {
      peopleGenesis:
        "0x4a2b5b737de1da59e209b0000a876ec2fa20035dc34fd292a848da32d255ad48",
    },
  },
  previewnet: {
    id: "previewnet",
    displayName: "PreviewNet",
    catalogNetwork: "previewnet",
    networkSuffix: "testnet",
    webGateway: "testnet.li",
    chains: {
      relay: {
        rpc: "wss://previewnet.substrate.dev/relay/alice",
        backend: "light-client",
      },
      assetHub: {
        rpc: "wss://previewnet.substrate.dev/asset-hub",
        backend: "rpc",
      },
      people: { rpc: "wss://previewnet.substrate.dev/people", backend: "rpc" },
      bulletin: {
        rpc: "wss://previewnet.substrate.dev/bulletin",
        backend: "rpc",
      },
    },
    dotns: {
      contentResolver: "7f74d7cd50f5a834270e2ad395a01b01891ab37d",
      contenthashSlot: 0,
      textSlot: 1,
      contentGateway: "https://previewnet.substrate.dev",
    },
    identityBackend: "https://identity-previewnet.dotspark.app",
    // Its People node (1.24.3, 2026-09-16) predates #13437.
    bestBlockPosting: null,
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
