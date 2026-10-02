import initProvider, { ChainProviderBuilder } from "@parity/truapi-provider";
import providerWasmUrl from "@parity/truapi-provider/truapi_provider_bg.wasm?url";
import { bytesToHex } from "@parity/truapi/scale";
import type { HexString } from "@parity/truapi/scale";
import type {
  ChainProvider,
  HostChainSet,
  JsonRpcConnection,
} from "@parity/truapi-host";
import { type NetworkConfig } from "./network-config.js";

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
  /**
   * Release the light client and close every connection handed out. The
   * provider handle is a WASM resource; without this a page that boots more
   * than one provider leaks the earlier one.
   */
  dispose(): void;
}

/**
 * Start the embedded light client for `config`'s network.
 *
 * Genesis hashes come from the provider's bundled catalog rather than from
 * constants kept here, so the core config, the chain set reported to products
 * and the chains the light client syncs cannot disagree.
 */
export async function connectNetwork(config: NetworkConfig): Promise<Network> {
  await initProvider({ module_or_path: providerWasmUrl });
  const builder = new ChainProviderBuilder();
  const chains = builder.addNetwork(config.catalogNetwork);
  const genesis: NetworkGenesis = {
    relay: chains.relay as HexString,
    assetHub: chains.assethub as HexString,
    people: chains.people as HexString,
    bulletin: chains.bulletin as HexString,
  };
  chains.free();
  const provider = builder.build();

  const open = new Set<JsonRpcConnection>();
  return {
    networkSuffix: config.networkSuffix,
    genesis,
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
      async connect(genesisHash) {
        const connection = await provider.connect(bytesToHex(genesisHash));
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
      },
    },
    dispose() {
      for (const connection of open) connection.close();
      open.clear();
      provider.free();
    },
  };
}