import * as v from 'valibot';

import { genesisHash as genesisHashSchema, useChainsMap } from '@/domains/network';

/**
 * The configured chain's display name for a genesis hash, or `null` when the host serves
 * no such chain.
 *
 * Feature-local on purpose: every review that names a network wants the name or the raw
 * hash as a fallback, and that choice is presentation, not a domain read.
 */
export const useChainName = (genesisHash: Nullable<string>): string | null => {
  const { data: chains } = useChainsMap();
  if (!genesisHash) return null;

  const parsed = v.safeParse(genesisHashSchema, genesisHash);

  return parsed.success ? (chains[parsed.output]?.name ?? null) : null;
};
