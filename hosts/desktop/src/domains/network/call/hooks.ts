import * as v from 'valibot';

import { dependentRead, useRead } from '@/shared/hooks';
import { useChainsMap } from '../chain/hooks';
import { genesisHash as genesisHashSchema } from '../chain/schemas';

import { decodedCallResource } from './resource';
import { type DecodedCall } from './types';

type Params = { genesisHash: string; callData: string };

// Typed so `useRead` infers `DecodedCall | null` from the default rather than `null`.
const NO_DECODED_CALL: DecodedCall | null = null;

/**
 * The call these bytes describe on this chain. `null` while it loads, when the chain
 * is unknown, and when the decode fails.
 *
 * The chains map has to settle before the read can start, so the two are folded with
 * `dependentRead` — an unstarted read reports `pending: false`, which a caller would
 * otherwise render as "there is no decode" for the moment before the map arrives.
 */
export const useDecodedCall = (params: Nullable<Params>) => {
  const chains = useChainsMap();

  const parsed = params ? v.safeParse(genesisHashSchema, params.genesisHash) : null;
  const chain = parsed?.success ? chains.data[parsed.output] : undefined;

  const result = useRead(decodedCallResource, {
    params: params && chain ? { chain, callData: params.callData } : null,
    defaultValue: NO_DECODED_CALL,
    map: (cache, { chain: readChain, callData }) => cache[`${readChain.genesisHash}:${callData}`] ?? null,
  });

  return dependentRead(result, { pending: chains.pending, error: null }, { enabled: params !== null });
};
