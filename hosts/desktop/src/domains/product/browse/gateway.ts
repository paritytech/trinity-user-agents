import { type AppListing, type Modality } from '@parity/browse-sdk';
import { createBrowseSdk, isKnownGenesis, selectNetwork } from '@parity/browse-sdk';
import { getWsProvider } from '@polkadot-api/ws-provider';

// Pure wire I/O: the caller resolves the active environment and passes the dotNS
// chain genesis in — this gateway resolves nothing.
async function listPublishedByModality(genesisHash: string, modality: Modality): Promise<AppListing[]> {
  if (!isKnownGenesis(genesisHash)) {
    return [];
  }

  const network = selectNetwork(genesisHash);
  const rpcUrl = network.ASSETHUB_RPCS[0];
  if (!rpcUrl) {
    return [];
  }

  const provider = getWsProvider(rpcUrl);
  const sdk = createBrowseSdk(network, provider);

  try {
    return await sdk.listAppsByModality(modality);
  } finally {
    sdk.destroy();
  }
}

// Dashboard-widget catalog: apps published under the `widget` modality subname.
async function listPublishedWidgets(genesisHash: string): Promise<AppListing[]> {
  return listPublishedByModality(genesisHash, 'widget');
}

// Fullscreen-SPA catalog (Favorites): apps published under the `app` modality subname.
async function listPublishedApps(genesisHash: string): Promise<AppListing[]> {
  return listPublishedByModality(genesisHash, 'app');
}

export const browseGateway = {
  listPublishedWidgets,
  listPublishedApps,
};
