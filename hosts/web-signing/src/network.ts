import initProvider, { ChainProviderBuilder } from "@parity/truapi-provider";
import providerWasmUrl from "@parity/truapi-provider/truapi_provider_bg.wasm?url";
import { bytesToHex } from "@parity/truapi/scale";
import type { HexString } from "@parity/truapi/scale";
import type { ChainProvider, HostChainSet } from "@parity/truapi-host";

/** The Paseo network as the bundled `truapi-provider` catalog names it. */
const CATALOG_NETWORK = "paseo-next-v2";

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
}

/**
 * Start the embedded light client for Paseo.
 *
 * Genesis hashes come from the provider's bundled catalog rather than from
 * constants kept here, so the core config, the chain set reported to products
 * and the chains the light client syncs cannot disagree.
 */
export async function connectPaseo(): Promise<Network> {
  await initProvider({ module_or_path: providerWasmUrl });
  const builder = new ChainProviderBuilder();
  const chains = builder.addNetwork(CATALOG_NETWORK);
  const genesis: NetworkGenesis = {
    relay: chains.relay as HexString,
    assetHub: chains.assethub as HexString,
    people: chains.people as HexString,
    bulletin: chains.bulletin as HexString,
  };
  chains.free();
  const provider = builder.build();

  return {
    networkSuffix: "paseo",
    genesis,
    supportedChains: {
      network: "paseo",
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
        return {
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
          close: () => connection.close(),
        };
      },
    },
  };
}
