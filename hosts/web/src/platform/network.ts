import initProvider, {
  ChainProviderBuilder,
  type ConnectionTypes,
} from "@parity/truapi-provider";
import providerWasmUrl from "@parity/truapi-provider/truapi_provider_bg.wasm?url";
import { bytesToHex } from "@parity/truapi/scale";
import type { HexString } from "@parity/truapi/scale";
import type {
  ChainProvider,
  HostChainSet,
  JsonRpcConnection,
} from "@parity/truapi-host";
import { spreadFollows } from "./follow-spread.js";
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
  supportedChains: HostChainSet;
  chain: ChainProvider;
  /** The backend each chain is reached through. */
  backends: Record<ChainRole, ChainBackend>;
  /**
   * Release the light client and close every connection handed out. The
   * provider handle is a WASM resource; without this a page that boots more
   * than one provider leaks the earlier one.
   */
  dispose(): void;
}

/**
 * Connect `config`'s network, each chain through the backend `backends` names.
 *
 * Genesis hashes come from the provider's bundled catalog rather than from
 * constants kept here, so the core config, the chain set reported to products
 * and the chains the light client syncs cannot disagree. Every chain starts
 * on the light client; a chain on `rpc` is then served by its RPC node
 * instead, and its answers are trusted from that node.
 */
export async function connectNetwork(
  config: NetworkConfig,
  backends: Record<ChainRole, ChainBackend> = chainBackends(config),
): Promise<Network> {
  await initProvider({ module_or_path: providerWasmUrl });
  const builder = new ChainProviderBuilder();
  builder.setConnectionTypes(hostConnectionTypes());
  const chains = builder.addNetwork(config.catalogNetwork);
  const genesis: NetworkGenesis = {
    relay: chains.relay as HexString,
    assetHub: chains.assethub as HexString,
    people: chains.people as HexString,
    bulletin: chains.bulletin as HexString,
  };
  chains.free();
  for (const role of CHAIN_ROLES)
    if (backends[role] === "rpc")
      builder.addRpcChain(genesis[role], config.chains[role].rpc);
  const provider = builder.build();

  const open = new Set<JsonRpcConnection>();
  return {
    networkSuffix: config.networkSuffix,
    genesis,
    backends,
    supportedChains: {
      network: config.id,
      chains: [
        { identifier: "Relay", genesisHash: genesis.relay },
        { identifier: "AssetHub", genesisHash: genesis.assetHub },
        { identifier: "People", genesisHash: genesis.people },
        { identifier: "Bulletin", genesisHash: genesis.bulletin },
      ],
    },
    chain: {
      // The core multiplexes every product's follows over the connection it
      // opens here, and one light-client connection holds two.
      async connect(genesisHash) {
        const lane = () => openConnection(bytesToHex(genesisHash));
        return spreadFollows(await lane(), lane);
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
