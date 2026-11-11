import initProvider, {
  ChainProviderBuilder,
  type ConnectionTypes,
} from "@parity/truapi-provider";
import providerWasmUrl from "@parity/truapi-provider/truapi_provider_bg.wasm?url";
import { bytesToHex } from "@parity/truapi/scale";
import type { HexString } from "@parity/truapi/scale";
import type { ChainProvider, JsonRpcConnection } from "@parity/truapi-host";
import { spreadFollows } from "./follow-spread.js";
import { checkedPostingConnection } from "./statement-store.js";
import {
  CHAIN_ROLES,
  type ChainBackend,
  type ChainRole,
  chainBackends,
  type NetworkConfig,
} from "./network-config.js";

/**
 * Peer connection kinds the browser light client dials.
 *
 * In production on public HTTPS, restrict to secure WebSockets so peers
 * advertising private or loopback addresses cannot trigger Chrome's Local
 * Network Access permission prompt. A dev server keeps loopback and unsecure
 * sockets allowed so local nodes still work.
 */
export function hostConnectionTypes(
  dev: boolean = import.meta.env.DEV,
): ConnectionTypes {
  return dev
    ? { secure: true, localhost: true, unsecure: true }
    : { secure: true, localhost: false, unsecure: false };
}

/** Genesis hashes of the chains this host serves. */
export interface NetworkGenesis {
  relay: HexString;
  assetHub: HexString;
  people: HexString;
  bulletin: HexString;
}

/** The network this host runs against and the chain transport that serves it. */
export interface Network {
  /** dotNS TLD the signing host derives identities under. */
  networkSuffix: string;
  genesis: NetworkGenesis;
  chain: ChainProvider;
  /** The backend each chain is reached through. */
  backends: Record<ChainRole, ChainBackend>;
  /** Whether the connections lent to the core passed the best-posting node checks. */
  postingReady?(): boolean;
  /**
   * Release the light client and close every connection handed out. The
   * provider handle is a WASM resource; without this a page that boots more
   * than one provider leaks the earlier one.
   */
  dispose(): void;
}

/**
 * The chains the light client can serve for `config`: those whose catalog
 * chain spec has the stack's genesis hash, on a relay that does too. After a
 * stack restarts from genesis, the bundled catalog describes chains that are
 * gone until the provider ships new specs.
 */
export function lightClientChains(
  config: NetworkConfig,
  catalog: NetworkGenesis,
): Set<ChainRole> {
  const same = (role: ChainRole) =>
    catalog[role].toLowerCase() === config.genesis[role].toLowerCase();
  return new Set(same("relay") ? CHAIN_ROLES.filter((role) => same(role)) : []);
}

/**
 * Connect `config`'s network, each chain through the backend `backends` names.
 *
 * Genesis hashes come from the stack's descriptor, so the core config, the
 * chain set reported to products and the chains the provider routes cannot
 * disagree. The light client starts from the provider's bundled catalog,
 * which must hold the same chains: a chain on `light-client` whose catalog
 * genesis differs, or of a stack with no catalog entry, is refused (see
 * {@link lightClientChains}). A chain on
 * `rpc` is served by its RPC node, and its answers are trusted from that node.
 */
export async function connectNetwork(
  config: NetworkConfig,
  backends: Record<ChainRole, ChainBackend> = chainBackends(config),
): Promise<Network> {
  await initProvider({ module_or_path: providerWasmUrl });
  const builder = new ChainProviderBuilder();
  builder.setConnectionTypes(hostConnectionTypes());
  let servable = new Set<ChainRole>();
  if (config.catalogNetwork !== null) {
    const chains = builder.addNetwork(config.catalogNetwork);
    servable = lightClientChains(config, {
      relay: chains.relay as HexString,
      assetHub: chains.assethub as HexString,
      people: chains.people as HexString,
      bulletin: chains.bulletin as HexString,
    });
    chains.free();
  }
  const genesis: NetworkGenesis = { ...config.genesis };
  for (const role of CHAIN_ROLES) {
    if (backends[role] === "rpc")
      builder.addRpcChain(genesis[role], config.chains[role].rpc);
    else if (!servable.has(role)) {
      builder.free();
      throw new Error(
        `The light client has no chain spec for ${config.displayName}'s ${role} chain ${genesis[role]}; run it on its RPC node.`,
      );
    }
  }
  const provider = builder.build();

  const open = new Set<JsonRpcConnection>();
  let postingChecked = false;
  let postingUnknown = false;
  return {
    networkSuffix: config.networkSuffix,
    genesis,
    backends,
    postingReady: () => postingChecked && !postingUnknown,
    chain: {
      // The core multiplexes every product's follows over the connection it
      // opens here, and one light-client connection holds two.
      async connect(genesisHash) {
        const lane = () => openConnection(bytesToHex(genesisHash));
        let first = await lane();
        if (
          bytesToHex(genesisHash).toLowerCase() ===
            genesis.people.toLowerCase() &&
          backends.people === "rpc" &&
          config.bestBlockPosting !== null
        ) {
          try {
            first = await checkedPostingConnection(
              first,
              config.bestBlockPosting.peopleGenesis,
            );
            postingChecked = true;
          } catch {
            postingUnknown = true;
            first = await lane();
          }
        }
        return spreadFollows(first, lane);
      },
    },
    dispose() {
      for (const connection of open) connection.close();
      open.clear();
      provider.free();
    },
  };

  async function openConnection(
    genesisHash: string,
  ): Promise<JsonRpcConnection> {
    const connection = await provider.connect(genesisHash);
    const wrapped: JsonRpcConnection = {
      send: (request) => connection.send(request),
      async *responses() {
        for (
          let response = await connection.nextResponse();
          response !== undefined;
          response = await connection.nextResponse()
        ) {
          yield response;
        }
      },
      close: () => {
        open.delete(wrapped);
        connection.close();
      },
    };
    open.add(wrapped);
    return wrapped;
  }
}
