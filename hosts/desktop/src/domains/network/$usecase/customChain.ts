import { firstValueFrom } from 'rxjs';
import * as v from 'valibot';

import { getChains } from '../chain/resource';
import { genesisHash } from '../chain/schemas';
import { type Chain, type GenesisHash } from '../chain/types';
import { customChainGateway } from '../custom-chain/gateway';
import { addCustomChain, customChainsResource } from '../custom-chain/resource';
import { customChainService } from '../custom-chain/service';

export type AddCustomChainResult =
  | { status: 'added'; name: string; genesisHash: GenesisHash }
  | { status: 'duplicate-builtin' }
  | { status: 'duplicate-custom' }
  | { status: 'failed'; message: string };

// Discover a Substrate endpoint, reject it if its genesis already belongs to a
// curated or custom chain, then persist it. Composes the discovery gateway, the
// builtin + custom chain reads, and the persistence mutation — a single source
// of truth for "can this endpoint be added", returned as a result the caller
// maps to UI feedback.
async function discoverAndAddChain(endpoint: string, name: string): Promise<AddCustomChainResult> {
  let discovered;
  try {
    discovered = await customChainGateway.discoverChain(endpoint);
  } catch (error) {
    return { status: 'failed', message: error instanceof Error ? error.message : '' };
  }

  const builtinChains = await getChains();
  if (builtinChains.some(chain => chain.genesisHash === discovered.genesisHash)) {
    return { status: 'duplicate-builtin' };
  }

  const customChains = await firstValueFrom(customChainsResource.read$({}));
  if (customChains[discovered.genesisHash]) {
    return { status: 'duplicate-custom' };
  }

  const displayName = name.trim() || discovered.name;
  await firstValueFrom(addCustomChain({ chainId: discovered.genesisHash, entry: { name: displayName, endpoints: [endpoint] } }));

  return { status: 'added', name: displayName, genesisHash: discovered.genesisHash };
}

/**
 * Every chain the host serves — the configured set plus the user's own — keyed by
 * genesis hash.
 *
 * The imperative twin of `useAllChainsMap`, for callers outside React. Read on
 * demand rather than held: a caller that keeps the map sees a chain the user adds
 * only if it re-reads, and the host callbacks answer per request anyway.
 */
async function getAllChainsMap(): Promise<Record<GenesisHash, Chain>> {
  const [builtin, custom] = await Promise.all([getChains(), firstValueFrom(customChainsResource.read$({}))]);

  const map: Record<GenesisHash, Chain> = {};
  for (const chain of builtin ?? []) {
    map[chain.genesisHash] = chain;
  }

  for (const [hash, entry] of Object.entries(custom ?? {})) {
    const parsed = v.safeParse(genesisHash, hash);
    // A malformed key is one unusable custom chain, not a reason to serve none.
    if (parsed.success) map[parsed.output] = customChainService.buildChain(parsed.output, entry);
  }

  return map;
}

export const customChainUseCase = {
  discoverAndAddChain,
  getAllChainsMap,
};
